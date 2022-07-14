use crate::audio::{Mixer, PCMRead};
use crate::plugin::{PluginManager, Audio};
use crate::state::State;

use rambot_api::AudioSource;

use serenity::client::Context;
use serenity::framework::standard::{Args, CommandGroup, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::prelude::Message;

use songbird::Call;
use songbird::input::{Input, Reader};

use std::sync::{Arc, Mutex};

use tokio::sync::Mutex as TokioMutex;

pub mod layer;

pub use layer::get_layer_commands;

#[group]
#[commands(connect, disconnect, play, stop)]
struct Root;

pub fn get_root_commands() -> &'static CommandGroup {
    &ROOT_GROUP
}

#[command]
#[only_in(guilds)]
#[description(
    "Connects the bot to the voice channel to which the sender of the command \
    is currently connected.")]
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

#[command]
#[only_in(guilds)]
#[description(
    "Disconnects the bot from the voice channel to which it is currently \
    connected.")]
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

async fn play_do(ctx: &Context, msg: &Message, layer: &str, command: &str,
        call: Arc<TokioMutex<Call>>) -> Option<String> {
    let mixer = get_mixer(ctx, msg).await;
    let mut call_guard = call.lock().await;
    let data_guard = ctx.data.read().await;
    let plugin_manager = data_guard.get::<PluginManager>().unwrap();
    let audio = match plugin_manager.resolve_audio(command) {
        Ok(a) => a,
        Err(e) =>
            return Some(format!("Could not resolve audio: {}", e))
    };

    let active_before = {
        let mut mixer_guard = mixer.lock().unwrap();

        if !mixer_guard.contains_layer(&layer) {
            return Some(format!("No layer of name {}.", &layer));
        }

        let result = mixer_guard.active();

        match audio {
            Audio::Single(source) => mixer_guard.play_on_layer(layer, source),
            Audio::List(list) => {
                if let Err(e) = mixer_guard.play_list_on_layer(layer, list) {
                    return Some(format!("Error initiating playlist: {}", e));
                }
            }
        }
        
        result
    };

    if !active_before {
        call_guard.play_only_source(to_input(mixer));
    }

    None
}

#[command]
#[only_in(guilds)]
#[description(
    "Plays the given audio on the given layer. Possible formats for the input \
    depend on the installed plugins.")]
#[usage("layer audio")]
async fn play(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let layer = args.single::<String>()?;
    let command = args.rest();

    match get_songbird_call(ctx, msg).await {
        Some(call) => {
            let reply = play_do(ctx, msg, &layer, command, call).await;

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

#[command]
#[only_in(guilds)]
#[description("Stops the audio currently playing on the given layer.")]
#[usage("layer")]
async fn stop(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let reply = if args.is_empty() {
        stop_all_do(ctx, msg).await
    }
    else {
        let layer = args.single::<String>()?;
        stop_do(ctx, msg, &layer).await
    };

    if let Some(reply) = reply {
        msg.reply(ctx, reply).await?;
    }

    Ok(())
}
