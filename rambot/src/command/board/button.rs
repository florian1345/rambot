use crate::command::board::with_board_manager_mut;

use serenity::client::Context;
use serenity::framework::standard::{Args, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::channel::ReactionType;
use serenity::model::prelude::Message;

#[group]
#[prefix("button")]
#[commands(add, remove)]
struct Button;

#[command]
#[only_in(guilds)]
#[description("Takes as first argument the board name, as second argument an \
    emote, and as third argument a command. Adds a button of the board with \
    the given name represented by the given emote that, when pressed, \
    executes the given command.")]
async fn add(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let board_name = args.single::<String>()?;
    let emote = args.single::<ReactionType>()?;
    let command = args.rest().to_owned();

    let err = with_board_manager_mut(ctx, msg.guild_id.unwrap(), |board_mgr| {
        if let Some(board) = board_mgr.boards.get_mut(&board_name) {
            if board.buttons.iter().any(|(r, _)| r == &emote) {
                Some(format!("Duplicate button: {}.", emote))
            }
            else {
                board.buttons.push((emote, command));
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

#[command]
#[only_in(guilds)]
#[description("Takes as first argument the board name and as second argument \
    an emote. Removes the button represented by the given emote from the \
    sound board with the given name.")]
async fn remove(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let board_name = args.single::<String>()?;
    let emote = args.single::<ReactionType>()?;

    let err = with_board_manager_mut(ctx, msg.guild_id.unwrap(), |board_mgr| {
        if let Some(board) = board_mgr.boards.get_mut(&board_name) {
            let old_len = board.buttons.len();

            board.buttons.retain(|b| &b.0 != &emote);

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
