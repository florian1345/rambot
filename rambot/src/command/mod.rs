use crate::FrameworkTypeMapKey;
use crate::audio::{Mixer, PCMRead, Layer};
use crate::key_value::KeyValueDescriptor;
use crate::plugin::PluginManager;
use crate::state::{State, GuildStateGuard};

use rambot_api::{AudioSource, ModifierDocumentation};

use rambot_proc_macro::rambot_command;

use serenity::client::Context;
use serenity::framework::standard::{CommandGroup, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::channel::MessageType;
use serenity::model::id::GuildId;
use serenity::model::prelude::Message;

use songbird::Call;
use songbird::input::{Input, Reader};

use std::collections::hash_map::Keys;
use std::fmt::Write;
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::Mutex as TokioMutex;

pub mod adapter;
pub mod board;
pub mod effect;
pub mod layer;

pub use adapter::get_adapter_commands;
pub use board::get_board_commands;
pub use effect::get_effect_commands;
pub use layer::get_layer_commands;

#[group]
#[commands(connect, disconnect, cmd_do, play, skip, stop)]
struct Root;

/// Gets a [CommandGroup] for the root commands (which are not part of any
/// sub-group).
pub fn get_root_commands() -> &'static CommandGroup {
    &ROOT_GROUP
}

#[rambot_command(
    description = "Connects the bot to the voice channel to which the sender \
        of the command is currently connected.",
    usage = ""
)]
async fn connect(ctx: &Context, msg: &Message)
        -> CommandResult<Option<String>> {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;
    let channel_id_opt = guild.voice_states
        .get(&msg.author.id)
        .and_then(|v| v.channel_id);

    let channel_id = match channel_id_opt {
        Some(c) => c,
        None => {
            return Ok(Some(
                "I cannot see your voice channel. Are you connected?"
                .to_owned()));
        }
    };

    let songbird = songbird::get(ctx).await.unwrap();

    if let Some(call) = songbird.get(guild_id) {
        if let Some(channel) = call.lock().await.current_channel() {
            if channel.0 == channel_id.0 {
                return Ok(Some(
                    "I am already connected to your voice channel."
                    .to_owned()));
            }
        }
    }

    log::debug!("Joining channel {} on guild {}.", channel_id, guild_id);
    songbird.join(guild_id, channel_id).await.1.unwrap();
    Ok(None)
}

async fn get_songbird_call(ctx: &Context, msg: &Message)
        -> Option<Arc<TokioMutex<Call>>> {
    let guild = msg.guild(&ctx.cache)?;
    let guild_id = guild.id;
    let songbird = songbird::get(ctx).await?;
    songbird.get(guild_id)
}

const NOT_CONNECTED: &str = "I am not connected to a voice channel";

#[rambot_command(
    description = "Disconnects the bot from the voice channel to which it is \
        currently connected.",
    usage = ""
)]
async fn disconnect(ctx: &Context, msg: &Message)
        -> CommandResult<Option<String>> {
    match get_songbird_call(ctx, msg).await {
        Some(call) => {
            let mut guard = call.lock().await;
            let channel_id = match guard.current_channel() {
                Some(id) => id,
                None => return Ok(Some(NOT_CONNECTED.to_owned()))
            };

            let guild_id = msg.guild_id.unwrap();
            log::debug!("Leaving channel {} on guild {}.", channel_id, guild_id);
            guard.leave().await?;
            Ok(None)
        },
        None => Ok(Some(NOT_CONNECTED.to_owned()))
    }
}

fn to_input<S: AudioSource + Send + 'static>(source: Arc<Mutex<S>>) -> Input {
    let read = PCMRead::new(source);
    Input::float_pcm(true, Reader::Extension(Box::new(read)))
}

async fn get_mixer(ctx: &Context, msg: &Message)
        -> Arc<Mutex<Mixer>> {
    let mut data_guard = ctx.data.write().await;
    let plugin_manager =
        Arc::clone(data_guard.get::<PluginManager>().unwrap());
    let state = data_guard.get_mut::<State>().unwrap();
    state.guild_state(msg.guild_id.unwrap(), plugin_manager).mixer()
}

async fn with_guild_state<T, F>(ctx: &Context, guild_id: GuildId, f: F) -> T
where
    F: FnOnce(GuildStateGuard) -> T
{
    let mut data_guard = ctx.data.write().await;
    let plugin_manager =
        Arc::clone(data_guard.get::<PluginManager>().unwrap());
    let state = data_guard.get_mut::<State>().unwrap();
    let guild_state = state.guild_state_mut(guild_id, plugin_manager);
    f(guild_state)
}

async fn with_mixer<T, F>(ctx: &Context, msg: &Message, f: F) -> T
where
    F: FnOnce(MutexGuard<Mixer>) -> T
{
    with_guild_state(ctx, msg.guild_id.unwrap(),
        |gs| f(gs.mixer().lock().unwrap())).await
}

async fn with_mixer_and_layer<T, F>(ctx: &Context, msg: &Message, layer: &str,
    f: F) -> Option<T>
where
    F: FnOnce(MutexGuard<Mixer>) -> T
{
    with_mixer(ctx, msg, |mixer| {
        if mixer.contains_layer(layer) {
            Some(f(mixer))
        }
        else {
            None
        }
    }).await
}

async fn play_do(ctx: &Context, msg: &Message, layer: &str, command: &str,
        call: Arc<TokioMutex<Call>>) -> Option<String> {
    let mixer = get_mixer(ctx, msg).await;
    let mut call_guard = call.lock().await;

    if call_guard.current_channel().is_none() {
        return Some(NOT_CONNECTED.to_owned());
    }

    let active_before = {
        let mut mixer_guard = mixer.lock().unwrap();

        if !mixer_guard.contains_layer(layer) {
            return Some(format!("No layer of name {}.", &layer));
        }

        let result = mixer_guard.active();

        if let Err(e) = mixer_guard.play_on_layer(layer, command) {
            return Some(format!("{}", e));
        }
        
        result
    };

    if !active_before {
        call_guard.play_only_source(to_input(mixer));
    }

    None
}

#[rambot_command(
    description = "Plays the given audio on the given layer. Possible formats \
        for the input depend on the installed plugins.",
    usage = "layer audio",
    rest
)]
async fn play(ctx: &Context, msg: &Message, layer: String, command: String)
        -> CommandResult<Option<String>> {
    match get_songbird_call(ctx, msg).await {
        Some(call) => Ok(play_do(ctx, msg, &layer, &command, call).await),
        None => Ok(Some(NOT_CONNECTED.to_owned()))
    }
}

#[rambot_command(
    description = "Plays the next piece of the list currently played on the \
        given layer. If the last piece of the list is active, this stops \
        audio on the layer.",
    usage = "layer"
)]
async fn skip(ctx: &Context, msg: &Message, layer: String)
        -> CommandResult<Option<String>> {
    let result = with_mixer_and_layer(ctx, msg, &layer,
        |mut mixer| mixer.skip_on_layer(&layer)).await;

    match result {
        Some(Ok(())) => Ok(None),
        Some(Err(e)) => Ok(Some(format!("{}", e))),
        None => Ok(Some(format!("Found no layer with name {}.", layer)))
    }
}

async fn stop_do(ctx: &Context, msg: &Message, layer: &str) -> Option<String> {
    let mixer = get_mixer(ctx, msg).await;
    let mut mixer_guard = mixer.lock().unwrap();
    
    if !mixer_guard.contains_layer(layer) {
        Some(format!("No layer of name {}.", layer))
    }
    else if !mixer_guard.stop_layer(layer) {
        Some("No audio to stop.".to_owned())
    }
    else {
        None
    }
}

async fn stop_all_do(ctx: &Context, msg: &Message) -> Option<String> {
    let mixer = get_mixer(ctx, msg).await;
    let mut mixer_guard = mixer.lock().unwrap();

    if mixer_guard.stop_all() {
        None
    }
    else {
        Some("No audio to stop.".to_owned())
    }
}

#[rambot_command(
    description = "Stops the audio currently playing on the given layer. If \
        no layer is given, all audio is stopped.",
    usage = "[layer]"
)]
async fn stop(ctx: &Context, msg: &Message, layer: Option<String>)
        -> CommandResult<Option<String>> {
    let reply = if let Some(layer) = layer {
        stop_do(ctx, msg, &layer).await
    }
    else {
        stop_all_do(ctx, msg).await
    };

    Ok(reply)
}

#[rambot_command(
    name = "do",
    description = "Takes as input a list of quoted strings separated by spaces. \
        These are then executed as commands in order.",
    usage = "[command] [command] ..."
)]
async fn cmd_do(ctx: &Context, msg: &Message, commands: Vec<String>)
        -> CommandResult<Option<String>> {
    let framework = Arc::clone(
        ctx.data.read().await.get::<FrameworkTypeMapKey>().unwrap());

    for command in commands {
        let mut msg = msg.clone();
        msg.content = command.to_owned();
        msg.kind = MessageType::Unknown; // Prevents :ok_hand:
        framework.dispatch(ctx.clone(), msg).await;
    }

    Ok(None)
}

async fn list_layer_key_value_descriptors<F>(ctx: &Context, msg: &Message,
    layer: String, name_plural_capital: &str, get: F)
    -> CommandResult<Option<String>>
where
    F: FnOnce(&Layer) -> &[KeyValueDescriptor]
{
    let descriptors = with_mixer_and_layer(ctx, msg, &layer, |mixer|
        get(mixer.layer(&layer)).iter()
            .map(|e| format!("{}", e))
            .collect::<Vec<_>>()).await;

    if let Some(descriptors) = descriptors {
        let mut reply =
            format!("{} on layer `{}`:", name_plural_capital, &layer);

        for (i, descriptor) in descriptors.iter().enumerate() {
            write!(reply, "\n{}. {}", i + 1, descriptor).unwrap();
        }

        msg.reply(ctx, reply).await?;
        Ok(None)
    }
    else {
        Ok(Some("Layer not found.".to_owned()))
    }
}

async fn help_modifiers<D, N, R>(ctx: &Context, msg: &Message,
    modifier: Option<String>, name_plural_upper: &str,
    name_singular_lower: &str, mut get_documentation: D, get_names: N)
    -> CommandResult<Option<String>>
where
    D: FnMut(&PluginManager, &str) -> Option<ModifierDocumentation>,
    N: FnOnce(&PluginManager) -> Keys<'_, String, R>
{
    let data_guard = ctx.data.read().await;
    let plugin_manager =
        Arc::clone(data_guard.get::<PluginManager>().unwrap());

    if let Some(name) = modifier {
        if let Some(documentation) =
                get_documentation(plugin_manager.as_ref(), &name) {
            msg.reply(ctx, format!("**{}**\n\n{}", name, documentation))
                .await?;
            Ok(None)
        }
        else {
            Ok(Some(format!("No {} of name {}.", name_singular_lower, name)))
        }
    }
    else {
        let mut response = format!("{} provided by plugins:", name_plural_upper);

        for name in get_names(plugin_manager.as_ref()) {
            let doc =
                get_documentation(plugin_manager.as_ref(), name.as_str())
                .unwrap();

            write!(&mut response, "\n- **{}**: {}", name, doc.short_summary())
                .unwrap();
        }

        msg.reply(ctx, response)
            .await?;
        Ok(None)
    }
}
