use crate::FrameworkTypeMapKey;
use crate::audio::{Mixer, PCMRead, Layer};
use crate::key_value::KeyValueDescriptor;
use crate::plugin::PluginManager;
use crate::state::{State, GuildStateGuard};

use rambot_api::AudioSource;

use rambot_proc_macro::rambot_command;

use serenity::client::Context;
use serenity::framework::standard::{CommandGroup, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::id::GuildId;
use serenity::model::prelude::Message;

use songbird::Call;
use songbird::input::{Input, Reader};

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

pub fn get_root_commands() -> &'static CommandGroup {
    &ROOT_GROUP
}

#[rambot_command(
    description = "Connects the bot to the voice channel to which the sender \
        of the command is currently connected.",
    usage = ""
)]
async fn connect(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;
    let channel_id_opt = guild.voice_states
        .get(&msg.author.id)
        .and_then(|v| v.channel_id);

    let channel_id = match channel_id_opt {
        Some(c) => c,
        None => {
            msg.reply(ctx,
                "I cannot see your voice channel. Are you connected?")
                .await?;
            return Ok(());
        }
    };

    let songbird = songbird::get(ctx).await.unwrap();

    if let Some(call) = songbird.get(guild_id) {
        if let Some(channel) = call.lock().await.current_channel() {
            if channel.0 == channel_id.0 {
                msg.reply(ctx, "I am already connected to your voice channel.")
                    .await?;
                return Ok(());
            }
        }
    }

    songbird.join(guild_id, channel_id).await.1.unwrap();
    Ok(())
}

async fn get_songbird_call(ctx: &Context, msg: &Message)
        -> Option<Arc<TokioMutex<Call>>> {
    let guild = msg.guild(&ctx.cache).await?;
    let guild_id = guild.id;
    let songbird = songbird::get(ctx).await?;
    songbird.get(guild_id)
}

#[rambot_command(
    description = "Disconnects the bot from the voice channel to which it is \
        currently connected.",
    usage = ""
)]
async fn disconnect(ctx: &Context, msg: &Message) -> CommandResult {
    match get_songbird_call(ctx, msg).await {
        Some(call) => {
            let mut guard = call.lock().await;
            guard.leave().await?;
        },
        None => {
            msg.reply(ctx, "I am not connected to a voice channel").await?;
        }
    }

    Ok(())
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
    f: F) -> CommandResult<Option<T>>
where
    F: FnOnce(MutexGuard<Mixer>) -> T
{
    let result = with_mixer(ctx, msg, |mixer| {
        if mixer.contains_layer(&layer) {
            Some(f(mixer))
        }
        else {
            None
        }
    }).await;

    if result.is_none() {
        msg.reply(ctx, "I could not find that layer.").await?;
    }

    Ok(result)
}

async fn play_do(ctx: &Context, msg: &Message, layer: &str, command: &str,
        call: Arc<TokioMutex<Call>>) -> Option<String> {
    let mixer = get_mixer(ctx, msg).await;
    let mut call_guard = call.lock().await;

    let active_before = {
        let mut mixer_guard = mixer.lock().unwrap();

        if !mixer_guard.contains_layer(&layer) {
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
        -> CommandResult {
    match get_songbird_call(ctx, msg).await {
        Some(call) => {
            let reply = play_do(ctx, msg, &layer, &command, call).await;

            if let Some(reply) = reply {
                msg.reply(ctx, reply).await?;
            }
        },
        None => {
            msg.reply(ctx, "I am not connected to a voice channel").await?;
        }
    }

    Ok(())
}

#[rambot_command(
    description = "Plays the next piece of the list currently played on the \
        given layer. If the last piece of the list is active, this stops \
        audio on the layer.",
    usage = "layer"
)]
async fn skip(ctx: &Context, msg: &Message, layer: String) -> CommandResult {
    let result = with_mixer_and_layer(ctx, msg, &layer,
        |mut mixer| mixer.skip_on_layer(&layer)).await?;

    if let Some(Err(e)) = result {
        msg.reply(ctx, e).await?;
    }

    Ok(())
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
        -> CommandResult {
    let reply = if let Some(layer) = layer {
        stop_do(ctx, msg, &layer).await
    }
    else {
        stop_all_do(ctx, msg).await
    };

    if let Some(reply) = reply {
        msg.reply(ctx, reply).await?;
    }

    Ok(())
}

#[rambot_command(
    name = "do",
    description = "Takes as input a list of quoted strings separated by spaces. \
        These are then executed as commands in order.",
    usage = "[command] [command] ..."
)]
async fn cmd_do(ctx: &Context, msg: &Message, commands: Vec<String>)
        -> CommandResult {
    let framework = Arc::clone(
        ctx.data.read().await.get::<FrameworkTypeMapKey>().unwrap());

    for command in commands {
        let mut msg = msg.clone();
        msg.content = command.to_owned();
        framework.dispatch(ctx.clone(), msg).await;
    }

    Ok(())
}

async fn list_layer_key_value_descriptors<F>(ctx: &Context, msg: &Message,
    layer: String, name_plural_capital: &str, get: F) -> CommandResult
where
    F: FnOnce(&Layer) -> &[KeyValueDescriptor]
{
    let descriptors = with_mixer_and_layer(ctx, msg, &layer, |mixer|
        get(mixer.layer(&layer)).iter()
            .map(|e| format!("{}", e))
            .collect::<Vec<_>>()).await?;

    if let Some(descriptors) = descriptors {
        let mut reply =
            format!("{} on layer `{}`:", name_plural_capital, &layer);

        for (i, descriptor) in descriptors.iter().enumerate() {
            reply.push_str(&format!("\n{}. {}", i + 1, descriptor));
        }

        msg.reply(ctx, reply).await?;
    }

    Ok(())
}
