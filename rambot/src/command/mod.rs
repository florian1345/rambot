use crate::FrameworkTypeMapKey;
use crate::audio::{PCMRead, Layer, Mixer};
use crate::key_value::KeyValueDescriptor;
use crate::plugin::PluginManager;
use crate::state::{State, GuildStateGuard, GuildState};

use rambot_api::{AudioSource, ModifierDocumentation};

use rambot_proc_macro::rambot_command;

use serenity::client::Context;
use serenity::framework::standard::{CommandGroup, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::channel::MessageType;
use serenity::model::id::GuildId;
use serenity::model::prelude::Message;

use songbird::Call;
use songbird::error::JoinError;
use songbird::input::{Input, Reader};

use std::collections::hash_map::Keys;
use std::fmt::Write;
use std::sync::{Arc, RwLock, RwLockWriteGuard};

use tokio::sync::Mutex as TokioMutex;

pub mod adapter;
pub mod board;
pub mod effect;
pub mod layer;

pub use adapter::get_adapter_commands;
pub use board::get_board_commands;
pub use effect::get_effect_commands;
pub use layer::get_layer_commands;

macro_rules! unwrap_or_return {
    ($e:expr, $r:expr) => {
        match $e {
            Some(v) => v,
            None => return $r
        }
    }
}

pub(crate) use unwrap_or_return;

#[group]
#[commands(audio, connect, directory, disconnect, cmd_do, play, skip, stop)]
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
    if let Some(songbird) = songbird::get(ctx).await {
        let guild_id = msg.guild_id.unwrap();

        match songbird.remove(guild_id).await {
            Ok(_) => {
                stop_all_do(ctx, msg).await; // drop audio sources
                log::debug!("Left voice on guild {}.", guild_id);
                Ok(None)
            },
            Err(JoinError::NoCall) => Ok(Some(NOT_CONNECTED.to_owned())),
            Err(e) => Err(e.into())
        }
    }
    else {
        log::error!("No songbird instance found.");
        Ok(Some("Internal error: No songbird instance found.".to_owned()))
    }
}

fn to_input<S>(source: Arc<RwLock<S>>) -> Input
where
    S: AudioSource + Send + Sync + 'static
{
    let read = PCMRead::new(source);
    Input::float_pcm(true, Reader::Extension(Box::new(read)))
}

async fn with_guild_state<T, F>(ctx: &Context, guild_id: GuildId, f: F)
    -> Option<T>
where
    F: FnOnce(&GuildState) -> T
{
    let data_guard = ctx.data.read().await;
    let state = data_guard.get::<State>().unwrap();
    state.guild_state(guild_id).map(f)
}

async fn with_guild_state_mut_unguarded<T, F>(ctx: &Context, guild_id: GuildId,
    f: F) -> Option<T>
where
    F: FnOnce(&mut GuildState) -> T
{
    let mut data_guard = ctx.data.write().await;
    let state = data_guard.get_mut::<State>().unwrap();
    state.guild_state_mut_unguarded(guild_id).map(f)
}

async fn configure_guild_state<T, F>(ctx: &Context, guild_id: GuildId, f: F)
    -> T
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

async fn play_do(ctx: &Context, msg: &Message, layer: &str, command: &str,
        call: Arc<TokioMutex<Call>>) -> Option<String> {
    let guild_id = msg.guild_id.unwrap();
    let (plugin_guild_config, mixer) = unwrap_or_return!(
        with_guild_state(ctx, guild_id, |gs| {
            (gs.build_plugin_guild_config(), gs.mixer_arc())
        }).await, Some(format!("No layer of name {}.", &layer)));
    let mut call_guard = call.lock().await;

    if call_guard.current_channel().is_none() {
        return Some(NOT_CONNECTED.to_owned());
    }

    let active_before = {
        let mut mixer_guard = mixer.write().unwrap();

        if !mixer_guard.contains_layer(layer) {
            return Some(format!("No layer of name {}.", &layer));
        }

        let result = mixer_guard.active();
        let play_res = mixer_guard.play_on_layer(
            layer, command, plugin_guild_config);

        if let Err(e) = play_res {
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
    let result = with_guild_state(ctx, msg.guild_id.unwrap(),
        |gs| {
            let mut mixer = gs.mixer_mut();

            if mixer.contains_layer(&layer) {
                Some(mixer.skip_on_layer(&layer))
            }
            else {
                None
            }
        }).await;

    match result.flatten() {
        Some(Ok(())) => Ok(None),
        Some(Err(e)) => Ok(Some(format!("{}", e))),
        None => Ok(Some(format!("Found no layer with name {}.", layer)))
    }
}

async fn stop_do(ctx: &Context, msg: &Message, layer: &str) -> Option<String> {
    let guild_id = msg.guild_id.unwrap();

    unwrap_or_return!(with_guild_state(ctx, guild_id, |gs| {
        let mut mixer = gs.mixer_mut();

        if !mixer.contains_layer(layer) {
            Some(format!("No layer of name {}.", layer))
        }
        else if !mixer.stop_layer(layer) {
            Some("No audio to stop.".to_owned())
        }
        else {
            None
        }
    }).await, Some(format!("No layer of name {}.", layer)))
}

async fn stop_all_do(ctx: &Context, msg: &Message) -> Option<String> {
    let guild_id = msg.guild_id.unwrap();

    unwrap_or_return!(with_guild_state(ctx, guild_id, |gs| {
        if gs.mixer_mut().stop_all() {
            None
        }
        else {
            Some("No audio to stop.".to_owned())
        }
    }).await, Some("No audio to stop.".to_owned()))
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

#[rambot_command(
    description = "Lists all plugin-provided types of audio with a short \
        summary. If an audio name is provided, a more detailed documentation \
        page for that audio is displayed.",
    usage = "[audio]",
    rest
)]
async fn audio(ctx: &Context, msg: &Message, audio: String)
        -> CommandResult<Option<String>> {
    let data_guard = ctx.data.read().await;
    let plugin_manager = data_guard.get::<PluginManager>().unwrap();

    if audio.is_empty() {
        let mut message = "Audio types:".to_owned();
        let mut first = true;

        for doc in plugin_manager.get_audio_documentations() {
            if first {
                writeln!(message).unwrap();
                first = false;
            }

            write!(message, "\n- {}", doc.overview_entry()).unwrap();
        }

        msg.reply(ctx, message).await?;
        Ok(None)
    }
    else {
        let audio_lower = audio.to_lowercase();
        let doc = plugin_manager.get_audio_documentations()
            .find(|d| d.name().to_lowercase() == audio_lower);

        if let Some(doc) = doc {
            msg.reply(ctx, doc).await?;
            Ok(None)
        }
        else {
            Ok(Some(format!("I found no audio of name {}.", audio)))
        }
    }
}

#[rambot_command(
    description = "Specify a guild-specific root directory for file system \
        based plugins. Omit directory argument to reset to the default root \
        directory specified in the config. Any pieces in playlists that are \
        currently active will continue to be resolved according to the old \
        root directrory.",
    usage = "[directory]",
    rest,
    confirm,
    owners_only
)]
async fn directory(ctx: &Context, msg: &Message, directory: String)
        -> CommandResult<Option<String>> {
    configure_guild_state(ctx, msg.guild_id.unwrap(), |mut guild_state|
        if directory.is_empty() {
            guild_state.unset_root_directory()
        }
        else {
            guild_state.set_root_directory(directory)
        }).await;

    Ok(None)
}

async fn configure_layer<F, T>(ctx: &Context, guild_id: GuildId, layer: &str,
    f: F) -> Option<T>
where
    F: FnOnce(RwLockWriteGuard<Mixer>) -> T
{
    configure_guild_state(ctx, guild_id, |gs| {
        let mixer = gs.mixer_mut();

        if mixer.contains_layer(layer) {
            Some(f(mixer))
        }
        else {
            None
        }
    }).await
}

async fn list_layer_key_value_descriptors<F>(ctx: &Context, msg: &Message,
    layer: String, name_plural_capital: &str, get: F)
    -> CommandResult<Option<String>>
where
    F: FnOnce(&Layer) -> &[KeyValueDescriptor]
{
    let guild_id = msg.guild_id.unwrap();
    let descriptors = with_guild_state(ctx, guild_id, |gs| {
        let mixer = gs.mixer();

        if mixer.contains_layer(&layer) {
            Some(get(mixer.layer(&layer)).iter()
                .map(|e| format!("{}", e))
                .collect::<Vec<_>>())
        }
        else {
            None
        }
    }).await.flatten();

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

        msg.reply(ctx, response).await?;
        Ok(None)
    }
}
