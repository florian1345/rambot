use crate::command::board::{with_board_manager_mut, Button, Board};

use rambot_proc_macro::rambot_command;

use serenity::client::Context;
use serenity::framework::standard::CommandResult;
use serenity::framework::standard::macros::{command, group};
use serenity::model::prelude::Message;

#[group("Button")]
#[prefix("button")]
#[commands(add, command, remove)]
struct ButtonCmd;

async fn with_board<F>(ctx: &Context, msg: &Message, board_name: String, f: F)
    -> CommandResult<Option<String>>
where
    F: FnOnce(&mut Board) -> Option<String>
{
    Ok(with_board_manager_mut(ctx, msg.guild_id.unwrap(), |board_mgr| {
        if let Some(board) = board_mgr.boards.get_mut(&board_name) {
            f(board)
        }
        else {
            Some(format!(
                "I could not find a board with name `{}`.", board_name))
        }
    }).await)
}

async fn with_button<F>(ctx: &Context, msg: &Message, board_name: String,
    label: String, f: F) -> CommandResult<Option<String>>
where
    F: FnOnce(&mut Button) -> Option<String>
{
    with_board(ctx, msg, board_name, |board| {
        let button = board.buttons.iter_mut()
            .find(|btn| btn.label == label);

        if let Some(button) = button {
            f(button)
        }
        else {
            Some(format!("I found no button with the label {}.", label))
        }
    }).await
}

#[rambot_command(
    description = "Adds a button of the board with the given name represented \
        by the given label that, when pressed, executes the given command.",
    usage = "board label command",
    rest,
    confirm
)]
async fn add(ctx: &Context, msg: &Message, board_name: String,
        label: String, command: String)
        -> CommandResult<Option<String>> {
    with_board(ctx, msg, board_name, |board| {
        if board.buttons.iter().any(|btn| btn.label == label) {
            Some(format!("Duplicate button: {}.", label))
        }
        else {
            board.buttons.push(Button {
                label,
                command
            });
            None
        }
    }).await
}

#[rambot_command(
    description = "Assigns a new command to the button represented by the \
        given label on the board with the given name.",
    usage = "board label command",
    rest,
    confirm
)]
async fn command(ctx: &Context, msg: &Message, board_name: String,
        label: String, command: String)
        -> CommandResult<Option<String>> {
    if command.is_empty() {
        return Ok(Some("Command may not be empty.".to_owned()));
    }

    with_button(ctx, msg, board_name, label, |button| {
        button.command = command;
        None
    }).await
}

#[rambot_command(
    description = "Removes the button represented by the given label from the \
        sound board with the given name.",
    usage = "board label",
    confirm
)]
async fn remove(ctx: &Context, msg: &Message, board_name: String,
        label: String) -> CommandResult<Option<String>> {
    with_board(ctx, msg, board_name, |board| {
        let old_len = board.buttons.len();

        board.buttons.retain(|btn| btn.label != label);

        if board.buttons.len() == old_len {
            Some(format!("I found no button with the label {}.", label))
        }
        else {
            None
        }
    }).await
}
