use crate::audio::Layer;
use crate::command::{list_layer_key_value_descriptors, with_mixer_and_layer};
use crate::key_value::KeyValueDescriptor;

use rambot_proc_macro::rambot_command;

use serenity::client::Context;
use serenity::framework::standard::{CommandGroup, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::prelude::Message;

#[group]
#[prefix("effect")]
#[commands(add, clear, list)]
struct Effect;

pub fn get_effect_commands() -> &'static CommandGroup {
    &EFFECT_GROUP
}

#[rambot_command(
    description = "Adds an effect to the layer with the given name.",
    rest
)]
async fn add(ctx: &Context, msg: &Message, layer: String,
        effect: KeyValueDescriptor) -> CommandResult {
    let res = with_mixer_and_layer(ctx, msg, &layer,
        |mut mixer| mixer.add_effect(&layer, effect)).await?;

    if let Some(Err(e)) = res {
        msg.reply(ctx, e).await?;
    }

    Ok(())
}

#[rambot_command(
    description = "Clears all effects from the layer with the given name. As \
        an optional second argument, this command takes an effect name. If \
        that is provided, only effects of that name are removed."
)]
async fn clear(ctx: &Context, msg: &Message, layer: String,
        name: Option<String>) -> CommandResult {
    let res = with_mixer_and_layer(ctx, msg, &layer, |mut mixer|
        if let Some(name) = &name {
            mixer.retain_effects(&layer,
                |descriptor| &descriptor.name != name)
        }
        else {
            Ok(mixer.clear_effects(&layer))
        }).await?;

    if let Some(res) = res {
        match res {
            Ok(count) => {
                if count == 0 {
                    if let Some(name) = name {
                        msg.reply(ctx, format!(
                            "Found no effect with name {} on layer {}.", name,
                            layer)).await?;
                    }
                    else {
                        msg.reply(ctx, format!(
                            "Found no effect on layer {}.", layer)).await?;
                    }
                }
            },
            Err(e) => {
                msg.reply(ctx, e).await?;
            }
        }
    }

    Ok(())
}

#[rambot_command(
    description = "Prints a list of all effects on the layer with the given \
        name."
)]
async fn list(ctx: &Context, msg: &Message, layer: String) -> CommandResult {
    list_layer_key_value_descriptors(
        ctx, msg, layer, "Effects", Layer::effects).await
}
