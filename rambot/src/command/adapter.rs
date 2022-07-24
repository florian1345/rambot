use crate::audio::Layer;
use crate::command::{list_layer_key_value_descriptors, with_mixer_and_layer};
use crate::key_value::KeyValueDescriptor;

use rambot_proc_macro::rambot_command;

use serenity::client::Context;
use serenity::framework::standard::{CommandGroup, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::prelude::Message;

#[group]
#[prefix("adapter")]
#[commands(add, clear, list)]
struct Adapter;

pub fn get_adapter_commands() -> &'static CommandGroup {
    &ADAPTER_GROUP
}

#[rambot_command(
    description = "Adds an adapter to the layer with the given name. Adapters \
        are given in the format `name(key1=value1,key2=value2,...)`, where \
        the set of available names and their associated required keys and \
        value formats depends on the installed plugins. You can use the \
        shortcuts `name` for `name()` and `name=value` for \
        `name(name=value)`.",
    usage = "layer adapter",
    rest,
    confirm
)]
async fn add(ctx: &Context, msg: &Message, layer: String,
        adapter: KeyValueDescriptor) -> CommandResult<Option<String>> {
    let success = with_mixer_and_layer(ctx, msg, &layer,
        |mut mixer| mixer.add_adapter(&layer, adapter)).await.is_some();

    if success {
        Ok(None)
    }
    else {
        Ok(Some("Layer not found.".to_owned()))
    }
}

#[rambot_command(
    description = "Clears all adapters from the layer with the given name. As \
        an optional second argument, this command takes an adapter name. If \
        that is provided, only adapters of that name are removed.",
    usage = "layer [adapter-type]",
    confirm
)]
async fn clear(ctx: &Context, msg: &Message, layer: String,
        name: Option<String>) -> CommandResult<Option<String>> {
    let removed = with_mixer_and_layer(ctx, msg, &layer, |mut mixer|
        if let Some(name) = &name {
            mixer.retain_adapters(&layer,
                |descriptor| &descriptor.name != name)
        }
        else {
            mixer.clear_adapters(&layer)
        }).await;

    match removed {
        Some(count) => {
            if count == 0 {
                let message = if let Some(name) = name {
                    format!("Found no adapter with name {} on layer {}.", name,
                        layer)
                }
                else {
                    format!("Found no adapter on layer {}.", layer)
                };

                Ok(Some(message))
            }
            else {
                Ok(None)
            }
        },
        None => Ok(Some("Layer not found.".to_owned()))
    }
}

#[rambot_command(
    description = "Prints a list of all adapters on the layer with the given \
        name.",
    usage = "layer"
)]
async fn list(ctx: &Context, msg: &Message, layer: String)
        -> CommandResult<Option<String>> {
    list_layer_key_value_descriptors(ctx, msg, layer, "Adapters",
        Layer::adapters).await
}
