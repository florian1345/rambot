use crate::audio::Layer;
use crate::command::{configure_layer, display_help, help_modifiers, list_layer_key_value_descriptors, respond, CommandResponse, CommandResult, Context};
use crate::key_value::KeyValueDescriptor;
use crate::plugin::PluginManager;

/// Collection of commands related to adapters.
///
/// Adapters are modifiers put on layers that control in which sequence elements of a playlist are
/// played.
#[poise::command(slash_command, prefix_command, subcommands("add", "clear", "list", "help"))]
pub async fn adapter(ctx: Context<'_>) -> CommandResult {
    display_help(ctx, Some("adapter")).await
}

/// Adds an adapter to the layer with the given name.
///
/// Adapters are given in the format `name(key1=value1,key2=value2,...)`, where the set of available
/// names and their associated required keys and value formats depends on the installed plugins. You
/// can use the shortcuts `name` for `name()` and `name=value` for `name(name=value)`.
///
/// Usage: `adapter add <layer> <adapter>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn add(ctx: Context<'_>, layer: String, #[rest] adapter: KeyValueDescriptor)
        -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let success = configure_layer(ctx, guild_id, &layer,
        |mut mixer| mixer.add_adapter(&layer, adapter)).await.is_some();

    let response = if !success {
        CommandResponse::Reply("Layer not found.")
    }
    else {
        CommandResponse::Confirm
    };

    respond(ctx, response).await
}

/// Clears all adapters from the layer with the given name.
///
/// As an optional second argument, this command takes an adapter name. If that is provided, only
/// adapters of that name are removed.
///
/// Usage: `adapter clear <layer> [adapter-type]`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn clear(ctx: Context<'_>, layer: String, name: Option<String>) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let removed = configure_layer(ctx, guild_id, &layer, |mut mixer|
        if let Some(name) = &name {
            mixer.retain_adapters(&layer,
                |descriptor| &descriptor.name != name)
        }
        else {
            mixer.clear_adapters(&layer)
        }).await;

    let response = match removed {
        Some(count) => {
            if count == 0 {
                if let Some(name) = name {
                    format!("Found no adapter with name {} on layer {}.", name, layer).into()
                }
                else {
                    format!("Found no adapter on layer {}.", layer).into()
                }
            }
            else {
                CommandResponse::Confirm
            }
        },
        None => {
            "Layer not found.".into()
        }
    };

    respond(ctx, response).await
}

/// Prints a list of all adapters on the layer with the given name.
///
/// Usage: `adapter list <layer>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn list(ctx: Context<'_>, layer: String) -> CommandResult {
    list_layer_key_value_descriptors(ctx, layer, "Adapters", Layer::adapters).await
}

/// Lists all available adapters with a short description.
///
/// If an adapter name is provided, a detailed description of the adapter and its parameters is
/// given.
///
/// Usage: `adapter help [adapter]`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn help(ctx: Context<'_>, adapter: Option<String>) -> CommandResult {
    help_modifiers(ctx, adapter, "Adapters", "adapter",
        PluginManager::get_adapter_documentation, PluginManager::adapter_names)
        .await
}
