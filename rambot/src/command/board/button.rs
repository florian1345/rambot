use crate::command::board::{
    get_board_manager_mut,
    unwrap_or_return,
    Button,
    Board
};

use crate::command::{display_help, respond, CommandResponse, CommandResult, Context};

/// Collection of commands for managing buttons on a sound board.
#[poise::command(slash_command, prefix_command,
    subcommands("add", "command", "deactivate", "remove", "swap"))]
pub async fn button(ctx: Context<'_>) -> CommandResult {
    display_help(ctx, Some("board button")).await
}

async fn configure_board<F>(ctx: Context<'_>, board_name: String, f: F)
    -> CommandResult<CommandResponse>
where
    F: FnOnce(&mut Board) -> CommandResponse
{
    let guild_id = ctx.guild_id().unwrap();
    let mut board_mgr = get_board_manager_mut(ctx.data(), guild_id).await;
    board_mgr.deactivate_board(ctx, &board_name).await?;

    if let Some(board) = board_mgr.boards.get_mut(&board_name) {
        Ok(f(board))
    }
    else {
        Ok(format!("I could not find a board with name `{}`.", board_name).into())
    }
}

async fn configure_button<F>(ctx: Context<'_>, board_name: String, label: String, f: F)
    -> CommandResult<CommandResponse>
where
    F: FnOnce(&mut Button) -> CommandResponse
{
    configure_board(ctx, board_name, |board| {
        let button = board.buttons.iter_mut()
            .find(|btn| btn.label == label);

        if let Some(button) = button {
            f(button)
        }
        else {
            format!("I found no button with the label {}.", label).into()
        }
    }).await
}

/// Adds a button of the board with the given name that, when pressed, executes the given command.
///
/// The button is represented by the given label.
///
/// Usage: `board button add <board> <label> <command>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn add(ctx: Context<'_>, board_name: String, label: String, #[rest] command: String)
        -> CommandResult {
    if command.is_empty() {
        ctx.reply("Command may not be empty.").await?;
        return Ok(());
    }

    let response = configure_board(ctx, board_name, |board| {
        if board.buttons.iter().any(|btn| btn.label == label) {
            format!("Duplicate button: {}.", label).into()
        }
        else {
            board.buttons.push(Button {
                label,
                command,
                deactivate_command: None,
                active: false
            });

            CommandResponse::Confirm
        }
    }).await?;

    respond(ctx, response).await
}

/// Assigns a new command to the given button on the board with the given name.
///
/// Usage: `board button command <board> <label> <command>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn command(ctx: Context<'_>, board_name: String, label: String, #[rest] command: String)
        -> CommandResult {
    if command.is_empty() {
        ctx.reply("Command may not be empty.").await?;
        return Ok(());
    }

    let response = configure_button(ctx, board_name, label, |button| {
        button.command = command;
        CommandResponse::Confirm
    }).await?;

    respond(ctx, response).await
}

/// Assigns a new deactivation command to the given button on the board with the given name.
///
/// If such a command is assigned to a button, the button will from now on be either active or
/// inactive, the latter by default. When pressed in the inactive state, it will execute the regular
/// command and move into the active state. Otherwise, it will execute the deactivation command
/// specified here and move into the inactive state. Omit the command to make the button stateless
/// again.
///
/// Usage: `board button deactivate <board> <label> [command]`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn deactivate(ctx: Context<'_>, board_name: String, label: String, #[rest] command: String)
        -> CommandResult {
    let response = configure_button(ctx, board_name, label, |button| {
        if command.is_empty() {
            button.deactivate_command = None;
            button.active = false;
        }
        else {
            button.deactivate_command = Some(command);
        }

        CommandResponse::Confirm
    }).await?;

    respond(ctx, response).await
}

fn get_button_idx(board: &Board, label: &str) -> Option<usize> {
    board.buttons.iter().enumerate()
        .find(|(_, button)| button.label == label)
        .map(|(idx, _)| idx)
}

/// Swaps the position of the buttons with `label_1` and `label_2` on the board with the given name.
///
/// Usage: `board button swap <board> <label_1>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn swap(ctx: Context<'_>, board_name: String, label_1: String, label_2: String)
        -> CommandResult {
    if label_1 == label_2 {
        ctx.reply("The button labels must not be the same.").await?;
        return Ok(());
    }

    let response = configure_board(ctx, board_name, |board| {
        let idx_1 = unwrap_or_return!(get_button_idx(board, &label_1),
            format!("I found no button with the label {}.", label_1).into());
        let idx_2 = unwrap_or_return!(get_button_idx(board, &label_2),
            format!("I found no button with the label {}.", label_2).into());

        board.buttons.swap(idx_1, idx_2);

        CommandResponse::Confirm
    }).await?;

    respond(ctx, response).await
}

/// Removes the button represented by the given label from the sound board with the given name.
///
/// Usage: `board button remove <board> <label>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn remove(ctx: Context<'_>, board_name: String, label: String) -> CommandResult {
    let response = configure_board(ctx, board_name, |board| {
        let old_len = board.buttons.len();

        board.buttons.retain(|btn| btn.label != label);

        if board.buttons.len() == old_len {
            format!("I found no button with the label {}.", label).into()
        }
        else {
            CommandResponse::Confirm
        }
    }).await?;

    respond(ctx, response).await
}
