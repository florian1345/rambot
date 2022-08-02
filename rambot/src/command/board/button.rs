use crate::command::board::{with_board_manager_mut, Button, Board};

use rambot_proc_macro::rambot_command;

use serenity::client::Context;
use serenity::framework::standard::CommandResult;
use serenity::framework::standard::macros::{command, group};
use serenity::model::channel::ReactionType;
use serenity::model::prelude::Message;

use std::fmt::Write;

#[group("Button")]
#[prefix("button")]
#[commands(add, command, description, get, remove)]
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
    button_emote: ReactionType, f: F) -> CommandResult<Option<String>>
where
    F: FnOnce(&mut Button) -> Option<String>
{
    with_board(ctx, msg, board_name, |board| {
        let button = board.buttons.iter_mut()
            .find(|btn| btn.emote == button_emote);

        if let Some(button) = button {
            f(button)
        }
        else {
            Some(format!("I found no button with the emote {}.", button_emote))
        }
    }).await
}

#[rambot_command(
    description = "Adds a button of the board with the given name represented \
        by the given emote that, when pressed, executes the given command.",
    usage = "board emote command",
    rest,
    confirm
)]
async fn add(ctx: &Context, msg: &Message, board_name: String,
        emote: ReactionType, command: String)
        -> CommandResult<Option<String>> {
    with_board(ctx, msg, board_name, |board| {
        if board.buttons.iter().any(|btn| btn.emote == emote) {
            Some(format!("Duplicate button: {}.", emote))
        }
        else {
            board.buttons.push(Button {
                emote,
                description: String::new(),
                command
            });
            None
        }
    }).await
}

#[rambot_command(
    description = "Assigns the given description to the button represented by \
        the given emote on the board with the given name. Omit description to \
        remove it from the button.",
    usage = "board emote [description]",
    rest,
    confirm
)]
async fn description(ctx: &Context, msg: &Message, board_name: String,
        emote: ReactionType, description: String)
        -> CommandResult<Option<String>> {
    with_button(ctx, msg, board_name, emote, |button| {
        button.description = description;
        None
    }).await
}

#[rambot_command(
    description = "Assigns a new command to the button represented by the \
        given emote on the board with the given name.",
    usage = "board emote command",
    rest,
    confirm
)]
async fn command(ctx: &Context, msg: &Message, board_name: String,
        emote: ReactionType, command: String)
        -> CommandResult<Option<String>> {
    if command.is_empty() {
        return Ok(Some("Command may not be empty.".to_owned()));
    }

    with_button(ctx, msg, board_name, emote, |button| {
        button.command = command;
        None
    }).await
}

#[rambot_command(
    description = "Gets the current command and description of the button \
        represented by the given emote on the board with the given name.",
    usage = "board emote"
)]
async fn get(ctx: &Context, msg: &Message, board_name: String,
        emote: ReactionType) -> CommandResult<Option<String>> {
    let mut response = String::new();
    let with_button_res = with_button(ctx, msg, board_name, emote, |button| {
        response = format!("Command: `{}`\n", button.command);

        if button.description.is_empty() {
            write!(response, "No description.").unwrap();
        }
        else {
            write!(response, "Description: {}", button.description).unwrap();
        }

        None
    }).await?;

    if let Some(e) = with_button_res {
        return Ok(Some(e));
    }

    msg.reply(ctx, response).await?;
    Ok(None)
}

#[rambot_command(
    description = "Removes the button represented by the given emote from the \
        sound board with the given name.",
    usage = "board emote",
    confirm
)]
async fn remove(ctx: &Context, msg: &Message, board_name: String,
        emote: ReactionType) -> CommandResult<Option<String>> {
    with_board(ctx, msg, board_name, |board| {
        let old_len = board.buttons.len();

        board.buttons.retain(|btn| btn.emote != emote);

        if board.buttons.len() == old_len {
            Some(format!("I found no button with the emote {}.", emote))
        }
        else {
            None
        }
    }).await
}
