use serenity::client::Context;
use serenity::framework::standard::{CommandGroup, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::prelude::Message;

#[group]
#[commands(connect, disconnect)]
struct Commands;

pub fn get_commands() -> &'static CommandGroup {
    &COMMANDS_GROUP
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
                .await.unwrap();
            return Ok(());
        }
    };

    let songbird = songbird::get(ctx).await.unwrap();

    if let Some(call) = songbird.get(guild_id) {
        if let Some(channel) = call.lock().await.current_channel() {
            if channel.0 == channel_id.0 {
                msg.reply(ctx, "I am already connected to your voice channel.")
                    .await.unwrap();
                return Ok(());
            }
        }
    }

    songbird.join(guild_id, channel_id).await.1.unwrap();
    Ok(())
}

#[command]
#[only_in(guilds)]
#[description(
    "Disconnects the bot from the voice channel to which it is currently \
    connected.")]
async fn disconnect(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;
    let songbird = songbird::get(ctx).await.unwrap();

    match songbird.get(guild_id) {
        Some(call) => {
            let mut guard = call.lock().await;
            guard.leave().await.unwrap();
        },
        None => {
            msg.reply(ctx,
                "I am not connected to a voice channel").await.unwrap();
        }
    }

    Ok(())
}
