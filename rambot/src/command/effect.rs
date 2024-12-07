use crate::audio::Layer;
use crate::command::{
    configure_layer,
    display_help,
    help_modifiers,
    list_layer_key_value_descriptors,
    respond,
    CommandResponse,
    CommandResult,
    Context
};
use crate::key_value::KeyValueDescriptor;
use crate::plugin::PluginManager;

/// Collection of commands related to effects.
///
/// Effects are modifiers put on layers that alter the audio in some way, such as volume or filters.
#[poise::command(slash_command, prefix_command, subcommands("add", "clear", "list", "help"))]
pub async fn effect(ctx: Context<'_>) -> CommandResult {
    display_help(ctx, Some("effect")).await
}

/// Adds an effect to the layer with the given name.
///
/// Effects are given in the format `name(key1=value1,key2=value2,...)`, where the set of available
/// names and their associated required keys and value formats depends on the installed plugins. You
/// can use the shortcuts `name` for `name()` and `name=value` for `name(name=value)`.
///
/// Usage: `effect add <layer> <effect>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn add(ctx: Context<'_>, layer: String, #[rest] effect: KeyValueDescriptor) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let res = configure_layer(ctx, guild_id, &layer,
        |mut mixer| mixer.add_effect(&layer, effect)).await;

    let response = match res {
        Some(Ok(())) => {
            CommandResponse::Confirm
        },
        Some(Err(e)) => {
            format!("{}", e).into()
        },
        None => {
            "Layer not found.".into()
        }
    };

    respond(ctx, response).await
}

/// Clears all effects from the layer with the given name.
/// 
/// As an optional second argument, this command takes an effect name. If that is provided, only
/// effects of that name are removed.
/// 
/// Usage: `effect clear <layer> [effect-type]`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn clear(ctx: Context<'_>, layer: String, name: Option<String>) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let res = configure_layer(ctx, guild_id, &layer, |mut mixer|
        if let Some(name) = &name {
            mixer.retain_effects(&layer,
                |descriptor| &descriptor.name != name)
        }
        else {
            Ok(mixer.clear_effects(&layer))
        }).await;

    let response = match res {
        Some(Ok(count)) => {
            if count == 0 {
                if let Some(name) = name {
                    format!("Found no effect with name {} on layer {}.", name, layer).into()
                }
                else {
                    format!("Found no effect on layer {}.", layer).into()
                }
            }
            else {
                CommandResponse::Confirm
            }
        },
        Some(Err(e)) => {
            format!("{}", e).into()
        },
        None => {
            "Layer not found.".into()
        }
    };

    respond(ctx, response).await
}

/// Prints a list of all effects on the layer with the given name.
/// 
/// Usage: `effect list <layer>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn list(ctx: Context<'_>, layer: String) -> CommandResult {
    list_layer_key_value_descriptors(ctx, layer, "Effects", Layer::effects).await
}

/// Lists all available effects with a short description.
/// 
/// If an effect name is provided, a detailed description of the effect and its parameters is given.
/// 
/// Usage: `effect help [effect]`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn help(ctx: Context<'_>, effect: Option<String>) -> CommandResult {
    help_modifiers(ctx, effect, "Effects", "effect",
        PluginManager::get_effect_documentation, PluginManager::effect_names)
        .await
}
