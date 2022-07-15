use crate::command::with_mixer;

use serenity::client::Context;
use serenity::framework::standard::{Args, CommandGroup, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::prelude::Message;

#[group]
#[prefix("adapter")]
#[commands(add, clear)]
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
    with_mixer(ctx, msg, |mut mixer| mixer.add_adapter(&layer, adapter)).await;

    Ok(())
}

#[command]
#[only_in(guilds)]
#[description("Clears all adapters from the layer with the given name.")]
async fn clear(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let layer = args.single::<String>()?;

    if !args.is_empty() {
        msg.reply(ctx, "Expected only the layer name.").await?;
    }

    with_mixer(ctx, msg, |mut mixer| mixer.clear_adapters(&layer)).await;

    Ok(())
}
