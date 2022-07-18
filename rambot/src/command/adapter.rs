use crate::audio::Layer;
use crate::command::{list_layer_key_value_descriptors, with_mixer_and_layer};

use serenity::client::Context;
use serenity::framework::standard::{Args, CommandGroup, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::prelude::Message;

#[group]
#[prefix("adapter")]
#[commands(add, clear, list)]
struct Adapter;

pub fn get_adapter_commands() -> &'static CommandGroup {
    &ADAPTER_GROUP
}

#[command]
#[only_in(guilds)]
#[description("Adds an adapter to the layer with the given name.")]
async fn add(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let layer = args.single::<String>()?;
    let adapter = match args.rest().parse() {
        Ok(e) => e,
        Err(e) => {
            msg.reply(ctx, e).await?;
            return Ok(());
        }
    };
    with_mixer_and_layer(ctx, msg, &layer,
        |mut mixer| mixer.add_adapter(&layer, adapter)).await?;

    Ok(())
}

#[command]
#[only_in(guilds)]
#[description("Clears all adapters from the layer with the given name. As an \
    optional second argument, this command takes an adapter name. If that is \
    provided, only adapters of that name are removed.")]
async fn clear(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let layer = args.single::<String>()?;
    let name = if args.is_empty() {
        None
    }
    else {
        Some(args.single::<String>()?)
    };

    if !args.is_empty() {
        msg.reply(ctx, "Expected only the layer name.").await?;
    }

    let removed = with_mixer_and_layer(ctx, msg, &layer, |mut mixer|
        if let Some(name) = &name {
            mixer.retain_adapters(&layer,
                |descriptor| &descriptor.name != name)
        }
        else {
            mixer.clear_adapters(&layer)
        }).await?;

    if let Some(removed) = removed {
        if removed == 0 {
            if let Some(name) = name {
                msg.reply(ctx, format!(
                    "Found no adapter with name {} on layer {}.", name,
                    layer)).await?;
            }
            else {
                msg.reply(ctx, format!(
                    "Found no adapter on layer {}.", layer)).await?;
            }
        }
    }

    Ok(())
}

#[command]
#[only_in(guilds)]
#[description(
    "Prints a list of all adapters on the layer with the given name.")]
async fn list(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    list_layer_key_value_descriptors(ctx, msg, args, "Adapters",
        Layer::adapters).await
}
