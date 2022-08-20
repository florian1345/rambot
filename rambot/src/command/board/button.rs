use crate::command::board::{
    get_board_manager_mut,
    unwrap_or_return,
    Button,
    Board
};

use rambot_proc_macro::rambot_command;

use serenity::client::Context;
use serenity::framework::standard::CommandResult;
use serenity::framework::standard::macros::{command, group};
use serenity::model::prelude::Message;

#[group("Button")]
#[prefix("button")]
#[commands(add, command, deactivate, remove, swap)]
struct ButtonCmd;

async fn configure_board<F>(ctx: &Context, msg: &Message, board_name: String,
    f: F) -> CommandResult<Option<String>>
where
    F: FnOnce(&mut Board) -> Option<String>
{
    let guild_id = msg.guild_id.unwrap();
    let mut board_mgr = get_board_manager_mut(ctx, guild_id).await;
    board_mgr.deactivate_board(ctx, &board_name).await?;

    if let Some(board) = board_mgr.boards.get_mut(&board_name) {
        Ok(f(board))
    }
    else {
        Ok(Some(format!(
            "I could not find a board with name `{}`.", board_name)))
    }
}

async fn configure_button<F>(ctx: &Context, msg: &Message, board_name: String,
    label: String, f: F) -> CommandResult<Option<String>>
where
    F: FnOnce(&mut Button) -> Option<String>
{
    configure_board(ctx, msg, board_name, |board| {
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
    if command.is_empty() {
        return Ok(Some("Command may not be empty.".to_owned()));
    }

    configure_board(ctx, msg, board_name, |board| {
        if board.buttons.iter().any(|btn| btn.label == label) {
            Some(format!("Duplicate button: {}.", label))
        }
        else {
            board.buttons.push(Button {
                label,
                command,
                deactivate_command: None,
                active: false
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

    configure_button(ctx, msg, board_name, label, |button| {
        button.command = command;
        None
    }).await
}

#[rambot_command(
    description = "Assigns a new deactivation command to the button \
        represented by the given label on the board with the given name. If \
        such a command is assigned to a button, the button will from now on \
        be either active or inactive, the latter by default. When pressed in \
        the inactive state, it will execute the regular command and move into \
        the active state. Otherwise, it will execute the deactivation command \
        specified here and move into the inactive state. Omit the command to \
        make the button stateless again.",
    usage = "board label [command]",
    rest,
    confirm
)]
async fn deactivate(ctx: &Context, msg: &Message, board_name: String,
        label: String, command: String) -> CommandResult<Option<String>> {
    configure_button(ctx, msg, board_name, label, |button| {
        if command.is_empty() {
            button.deactivate_command = None;
            button.active = false;
        }
        else {
            button.deactivate_command = Some(command);
        }

        None
    }).await
}

fn get_button_idx(board: &Board, label: &str) -> Option<usize> {
    board.buttons.iter().enumerate()
        .find(|(_, button)| button.label == label)
        .map(|(idx, _)| idx)
}

#[rambot_command(
    description = "Swaps the position of the button with `label_1` and the \
        one with `label_2` on the board with the given name.",
    usage = "board label_1 label_2",
    confirm
)]
async fn swap(ctx: &Context, msg: &Message, board_name: String,
        label_1: String, label_2: String) -> CommandResult<Option<String>> {
    if label_1 == label_2 {
        return Ok(Some("The button labels must not be the same.".to_owned()));
    }

    configure_board(ctx, msg, board_name, |board| {
        let idx_1 = unwrap_or_return!(get_button_idx(board, &label_1),
            Some(format!("I found no button with the label {}.", label_1)));
        let idx_2 = unwrap_or_return!(get_button_idx(board, &label_2),
            Some(format!("I found no button with the label {}.", label_2)));

        board.buttons.swap(idx_1, idx_2);
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
    configure_board(ctx, msg, board_name, |board| {
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
