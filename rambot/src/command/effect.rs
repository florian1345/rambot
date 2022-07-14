use crate::command::with_mixer;

use serenity::client::Context;
use serenity::framework::standard::{Args, CommandGroup, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::prelude::Message;

#[group]
#[prefix("effect")]
#[commands(add, clear)]
struct Effect;

pub fn get_effect_commands() -> &'static CommandGroup {
    &EFFECT_GROUP
}

#[command]
#[only_in(guilds)]
#[description("Adds an effect to the layer with the given name.")]
async fn add(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let layer = args.single::<String>()?;
    let effect = args.rest();
    let res = with_mixer(ctx, msg,
        |mut mixer| mixer.add_effect(&layer, effect)).await;

    if let Err(e) = res {
        msg.reply(ctx, e).await?;
    }

    Ok(())
}

#[command]
#[only_in(guilds)]
#[description("Clears all effects from the layer with the given name.")]
async fn clear(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let layer = args.single::<String>()?;

    if !args.is_empty() {
        msg.reply(ctx, "Expected only the layer name.").await?;
    }

    with_mixer(ctx, msg, |mut mixer| mixer.clear_effects(&layer)).await;

    Ok(())
}
