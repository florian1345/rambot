use crate::command::board::{with_board_manager_mut, Button};

use rambot_proc_macro::rambot_command;

use serenity::client::Context;
use serenity::framework::standard::CommandResult;
use serenity::framework::standard::macros::{command, group};
use serenity::model::channel::ReactionType;
use serenity::model::prelude::Message;

#[group("Button")]
#[prefix("button")]
#[commands(add, description, remove)]
struct ButtonCmd;

// TODO reduce code duplication among these commands

#[rambot_command(
    description = "Adds a button of the board with the given name represented \
        by the given emote that, when pressed, executes the given command.",
    usage = "board emote command",
    rest
)]
async fn add(ctx: &Context, msg: &Message, board_name: String,
            emote: ReactionType, command: String) -> CommandResult {
    let err = with_board_manager_mut(ctx, msg.guild_id.unwrap(), |board_mgr| {
        if let Some(board) = board_mgr.boards.get_mut(&board_name) {
            if board.buttons.iter().any(|btn| &btn.emote == &emote) {
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
        }
        else {
            Some(format!(
                "I could not find a board with name `{}`.", board_name))
        }
    }).await;

    if let Some(err) = err {
        msg.reply(ctx, err).await?;
    }

    Ok(())
}

#[rambot_command(
    description = "Assigned the given description to the button represented \
        by the given emote on the board with the given name. Omit description \
        to remove it from the button.",
    usage = "board emote [description]",
    rest
)]
async fn description(ctx: &Context, msg: &Message, board_name: String,
        emote: ReactionType, description: String) -> CommandResult {
    let err = with_board_manager_mut(ctx, msg.guild_id.unwrap(), |board_mgr| {
        if let Some(board) = board_mgr.boards.get_mut(&board_name) {
            let button = board.buttons.iter_mut()
                .find(|btn| &btn.emote == &emote);

            if let Some(button) = button {
                button.description = description;
                None
            }
            else {
                Some(format!("I found no button with the emote {}.", emote))
            }
        }
        else {
            Some(format!(
                "I could not find a board with name `{}`.", board_name))
        }
    }).await;

    if let Some(err) = err {
        msg.reply(ctx, err).await?;
    }

    Ok(())
}

#[rambot_command(
    description = "Removes the button represented by the given emote from the \
        sound board with the given name.",
    usage = "board emote"
)]
async fn remove(ctx: &Context, msg: &Message, board_name: String,
        emote: ReactionType) -> CommandResult {
    let err = with_board_manager_mut(ctx, msg.guild_id.unwrap(), |board_mgr| {
        if let Some(board) = board_mgr.boards.get_mut(&board_name) {
            let old_len = board.buttons.len();

            board.buttons.retain(|btn| &btn.emote != &emote);

            if board.buttons.len() == old_len {
                Some(format!("I found no button with the emote {}.", emote))
            }
            else {
                None
            }
        }
        else {
            Some(format!(
                "I could not find a board with name `{}`.", board_name))
        }
    }).await;

    if let Some(err) = err {
        msg.reply(ctx, err).await?;
    }

    Ok(())
}
