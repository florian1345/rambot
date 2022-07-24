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
    description = "Adds an effect to the layer with the given name. Effects \
        are given in the format `name(key1=value1,key2=value2,...)`, where \
        the set of available names and their associated required keys and \
        value formats depends on the installed plugins. You can use the \
        shortcuts `name` for `name()` and `name=value` for \
        `name(name=value)`.",
    usage = "layer effect",
    rest,
    confirm
)]
async fn add(ctx: &Context, msg: &Message, layer: String,
        effect: KeyValueDescriptor) -> CommandResult<Option<String>> {
    let res = with_mixer_and_layer(ctx, msg, &layer,
        |mut mixer| mixer.add_effect(&layer, effect)).await;

    match res {
        Some(Ok(())) => Ok(None),
        Some(Err(e)) => Ok(Some(format!("{}", e))),
        None => Ok(Some("Layer not found.".to_owned()))
    }
}

#[rambot_command(
    description = "Clears all effects from the layer with the given name. As \
        an optional second argument, this command takes an effect name. If \
        that is provided, only effects of that name are removed.",
    usage = "layer [effect-type]",
    confirm
)]
async fn clear(ctx: &Context, msg: &Message, layer: String,
        name: Option<String>) -> CommandResult<Option<String>> {
    let res = with_mixer_and_layer(ctx, msg, &layer, |mut mixer|
        if let Some(name) = &name {
            mixer.retain_effects(&layer,
                |descriptor| &descriptor.name != name)
        }
        else {
            Ok(mixer.clear_effects(&layer))
        }).await;

    match res {
        Some(Ok(count)) => {
            if count == 0 {
                let message = if let Some(name) = name {
                    format!("Found no effect with name {} on layer {}.", name,
                        layer)
                }
                else {
                    format!("Found no effect on layer {}.", layer)
                };

                Ok(Some(message))
            }
            else {
                Ok(None)
            }
        },
        Some(Err(e)) => Ok(Some(format!("{}", e))),
        None => Ok(Some("Layer not found.".to_owned()))
    }
}

#[rambot_command(
    description = "Prints a list of all effects on the layer with the given \
        name.",
    usage = "layer"
)]
async fn list(ctx: &Context, msg: &Message, layer: String)
        -> CommandResult<Option<String>> {
    list_layer_key_value_descriptors(
        ctx, msg, layer, "Effects", Layer::effects).await
}
