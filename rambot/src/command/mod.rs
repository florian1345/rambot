use crate::audio::PCMRead;
use crate::config::Config;
use crate::plugin::PluginManager;
use crate::state::State;

use rambot_api::audio::AudioSource;

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
#[commands(connect, disconnect, play)]
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

#[command]
#[only_in(guilds)]
#[description(
    "Plays the given audio. Possible formats for the input depend on the \
    installed plugins.")]
async fn play(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let mixer = {
        let mut data_guard = ctx.data.write().await;
        let state = data_guard.get_mut::<State>().unwrap();
        state.guild_state(msg.guild_id.unwrap()).mixer()
    };
    let layer = args.single::<String>()?;

    if !mixer.lock().unwrap().contains_layer(&layer) {
        msg.reply(ctx, format!("No layer of name {}.", &layer)).await?;
        return Ok(());
    }

    match get_songbird_call(ctx, msg).await {
        Some(call) => {
            let mut call_guard = call.lock().await;
            let command = args.rest();
            let data_guard = ctx.data.read().await;
            let plugin_manager = data_guard.get::<PluginManager>().unwrap();
            let config = data_guard.get::<Config>().unwrap();
            let source = match plugin_manager.resolve_source(command, config) {
                Ok(s) => s,
                Err(e) => {
                    msg.reply(ctx, format!("Could not resolve audio: {}", e))
                        .await?;
                    return Ok(());
                }
            };

            let active_before = {
                let mut mixer_guard = mixer.lock().unwrap();
                let result = mixer_guard.active();
                mixer_guard.play_on_layer(&layer, source);
                result
            };

            if !active_before {
                call_guard.play_source(to_input(mixer));
            }
        },
        None => {
            msg.reply(ctx, "I am not connected to a voice channel").await?;
        }
    }

    Ok(())
}
