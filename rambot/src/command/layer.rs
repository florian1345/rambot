use crate::audio::Mixer;
use crate::plugin::PluginManager;
use crate::state::State;

use serenity::client::Context;
use serenity::framework::standard::{Args, CommandGroup, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::prelude::Message;

use std::sync::{MutexGuard, Arc};

#[group]
#[prefix("layer")]
#[commands(add, remove, list)]
struct Layer;

pub fn get_layer_commands() -> &'static CommandGroup {
    &LAYER_GROUP
}

async fn get_layer(ctx: &Context, msg: &Message, mut args: Args)
        -> CommandResult<Option<String>> {
    let layer = args.single::<String>()?;

    if !args.is_empty() {
        msg.reply(ctx, "Expected only the layer name.").await?;
        return Ok(None);
    }

    Ok(Some(layer))
}

async fn with_mixer<T>(ctx: &Context, msg: &Message,
        f: impl FnOnce(MutexGuard<Mixer>) -> T) -> T {
    let mut data_guard = ctx.data.write().await;
    let plugin_manager =
        Arc::clone(data_guard.get::<PluginManager>().unwrap());
    let state = data_guard.get_mut::<State>().unwrap();
    let guild_state =
        state.guild_state_mut(msg.guild_id.unwrap(), plugin_manager);
    let mixer = guild_state.mixer();
    f(mixer.lock().unwrap())
}

#[command]
#[only_in(guilds)]
#[description("Adds a layer with the given name to the mixer in this guild.")]
async fn add(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    if let Some(layer) = get_layer(ctx, msg, args).await? {
        let added = with_mixer(ctx, msg, move |mut mixer|
            mixer.add_layer(layer)).await;

        if !added {
            msg.reply(ctx, "A layer with the same name already exists.").await?;
        }
    }

    Ok(())
}

#[command]
#[only_in(guilds)]
#[description(
    "Removes a layer with the given name from the mixer in this guild.")]
async fn remove(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    if let Some(layer) = get_layer(ctx, msg, args).await? {
        let removed = with_mixer(ctx, msg, move |mut mixer|
            mixer.remove_layer(&layer)).await;

        if !removed {
            msg.reply(ctx, "Layer not found.").await?;
        }
    }

    Ok(())
}

#[command]
#[only_in(guilds)]
#[description(
    "Prints a list of the names of all layers of the mixer in this guild.")]
async fn list(ctx: &Context, msg: &Message) -> CommandResult {
    let layers = with_mixer(ctx, msg, |mixer| {
        mixer.layers().cloned().collect::<Vec<_>>()
    }).await;

    let response = if layers.is_empty() {
        "No layers registered in this guild.".to_owned()
    }
    else {
        format!("Layer list:\n- {}", layers.join("\n- "))
    };

    msg.reply(ctx, response).await?;
    Ok(())
}
