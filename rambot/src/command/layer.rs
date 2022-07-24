use crate::command::with_mixer;

use rambot_proc_macro::rambot_command;

use serenity::client::Context;
use serenity::framework::standard::{CommandGroup, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::prelude::Message;

#[group]
#[prefix("layer")]
#[commands(add, remove, list)]
struct Layer;

pub fn get_layer_commands() -> &'static CommandGroup {
    &LAYER_GROUP
}

#[rambot_command(
    description = "Adds a layer with the given name to the mixer in this \
        guild.",
    usage = "name",
    confirm
)]
async fn add(ctx: &Context, msg: &Message, layer: String)
        -> CommandResult<Option<String>> {
    let added = with_mixer(ctx, msg, move |mut mixer|
        mixer.add_layer(layer)).await;

    if added {
        Ok(None)
    }
    else {
        Ok(Some("A layer with the same name already exists.".to_owned()))
    }
}

#[rambot_command(
    description = "Removes a layer with the given name from the mixer in this 
        guild.",
    usage = "name",
    confirm
)]
async fn remove(ctx: &Context, msg: &Message, layer: String)
        -> CommandResult<Option<String>> {
    let removed = with_mixer(ctx, msg, move |mut mixer|
        mixer.remove_layer(&layer)).await;

    if removed {
        Ok(None)
    }
    else {
        Ok(Some("Layer not found.".to_owned()))
    }
}

#[rambot_command(
    description = "Prints a list of the names of all layers of the mixer in \
        this guild.",
    usage = ""
)]
async fn list(ctx: &Context, msg: &Message) -> CommandResult {
    let layers = with_mixer(ctx, msg, |mixer| {
        mixer.layers().iter().map(|l| l.name().to_owned()).collect::<Vec<_>>()
    }).await;

    let response = if layers.is_empty() {
        "No layers registered in this guild.".to_owned()
    }
    else {
        format!("Layer list:\n- {}", layers.join("\n- "))
    };

    msg.reply(ctx, response).await?;
    Ok(None)
}
