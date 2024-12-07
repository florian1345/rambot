use crate::command::{
    display_help,
    get_guild_state,
    get_guild_state_mut,
    respond,
    CommandResponse,
    CommandResult,
    Context
};

/// Collection of commands for managing audio layers.
#[poise::command(slash_command, prefix_command, subcommands("add", "remove", "list"))]
pub async fn layer(ctx: Context<'_>) -> CommandResult {
    display_help(ctx, Some("layer")).await
}

/// Adds a layer with the given name to the mixer in this guild.
///
/// Usage: `layer add <name>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn add(ctx: Context<'_>, layer: String) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let guild_state = get_guild_state_mut(ctx.data(), guild_id).await;
    let added = guild_state.mixer_mut().add_layer(layer);

    let response = if added {
        CommandResponse::Confirm
    }
    else {
        CommandResponse::Reply("A layer with the same name already exists.")
    };

    respond(ctx, response).await
}

/// Removes a layer with the given name from the mixer in this guild.
///
/// Usage: `layer remove <name>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn remove(ctx: Context<'_>, layer: String) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let guild_state = get_guild_state_mut(ctx.data(), guild_id).await;
    let removed = guild_state.mixer_mut().remove_layer(&layer);

    let response = if removed {
        CommandResponse::Confirm
    }
    else {
        CommandResponse::Reply("Layer not found.")
    };

    respond(ctx, response).await
}

/// Prints a list of the names of all layers of the mixer in this guild.
///
/// Usage: `layer list`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn list(ctx: Context<'_>) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let layers = get_guild_state(ctx.data(), guild_id).await
        .map(|gs| gs.mixer_blocking().layers().iter()
            .map(|l| l.name().to_owned())
            .collect::<Vec<_>>()
        ).unwrap_or_default();

    let response = if layers.is_empty() {
        "No layers registered in this guild.".into()
    }
    else {
        format!("Layer list:\n- {}", layers.join("\n- ")).into()
    };

    respond(ctx, response).await
}
