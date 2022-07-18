use crate::FrameworkTypeMapKey;
use crate::command::{get_single_string_arg, with_guild_state};

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use serenity::client::{Context, EventHandler};
use serenity::framework::standard::{Args, CommandGroup, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::channel::{ReactionType, Reaction};
use serenity::model::id::{MessageId, GuildId};
use serenity::model::prelude::Message;

mod button;

use button::BUTTONCMD_GROUP;

#[group]
#[prefix("board")]
#[commands(add, remove, display, list)]
#[sub_groups(buttoncmd)]
struct BoardCmd;

pub fn get_board_commands() -> &'static CommandGroup {
    &BOARDCMD_GROUP
}

async fn get_board_arg(ctx: &Context, msg: &Message, args: Args)
        -> CommandResult<Option<String>> {
    get_single_string_arg(ctx, msg, args, "Expected only the board name.")
        .await
}

#[command]
#[only_in(guilds)]
#[description("Adds a new, empty board with the given name. If there is \
    already a board with the same name, nothing is changed and an appropriate \
    reply is sent.")]
async fn add(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    if let Some(board_name) = get_board_arg(ctx, msg, args).await? {
        let guild_id = msg.guild_id.unwrap();

        let added = with_board_manager_mut(ctx, guild_id, |board_mgr| {
            board_mgr.add_board(Board {
                name: board_name,
                buttons: Vec::new()
            })
        }).await;

        if !added {
            msg.reply(ctx, "There is already a board with that name.").await?;
        }
    }

    Ok(())
}

#[command]
#[only_in(guilds)]
#[description("Removes the sound board with the given name.")]
async fn remove(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    if let Some(board_name) = get_board_arg(ctx, msg, args).await? {
        let guild_id = msg.guild_id.unwrap();

        let found = with_board_manager_mut(ctx, guild_id, |board_mgr| {
            if board_mgr.boards.remove(&board_name).is_some() {
                board_mgr.active_boards.retain(|_, v| v != &board_name);
                true
            }
            else {
                false
            }
        }).await;

        if !found {
            msg.reply(ctx, "I did not find any board with that name.").await?;
        }
    }

    Ok(())
}

#[command]
#[only_in(guilds)]
#[description("Displays the sound board with the given name.")]
async fn display(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    if let Some(board_name) = get_board_arg(ctx, msg, args).await? {
        let guild_id = msg.guild_id.unwrap();

        let board_res = with_board_manager(ctx, guild_id, |board_mgr| {
            if let Some(board) = board_mgr.boards.get(&board_name) {
                Ok(board.clone())
            }
            else {
                Err(format!("I found no board with name `{}`.", board_name))
            }
        }).await;

        match board_res {
            Ok(board) => {
                let mut content = format!("Sound board `{}`:", board_name);

                for button in &board.buttons {
                    if !button.description.is_empty() {
                        content.push_str(&format!("\n{} : {}", &button.emote,
                            &button.description));
                    }
                }

                let board_msg = msg.channel_id.say(ctx, content).await?;

                for button in &board.buttons {
                    board_msg.react(ctx, button.emote.clone()).await?;
                }

                with_board_manager_mut(ctx, guild_id, |board_mgr| {
                    board_mgr.active_boards.insert(board_msg.id, board_name);
                }).await;
            },
            Err(e) => {
                msg.reply(ctx, e).await?;
            }
        }
    }

    Ok(())
}

#[command]
#[only_in(guilds)]
#[description("Lists all sound boards that are available on this guild.")]
async fn list(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    let guild_id = msg.guild_id.unwrap();
    let mut names = with_board_manager(ctx, guild_id, |board_mgr| {
        board_mgr.boards()
            .map(|b| b.name.clone())
            .collect::<Vec<_>>()
    }).await;
    names.sort();
    let mut reply = "Sound boards:".to_owned();

    for name in names {
        reply.push_str("\n - `");
        reply.push_str(&name);
        reply.push('`');
    }

    msg.reply(ctx, reply).await?;
    Ok(())
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Button {
    emote: ReactionType,
    description: String,
    command: String
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Board {
    name: String,
    buttons: Vec<Button>
}

pub struct BoardManager {
    boards: HashMap<String, Board>,
    active_boards: HashMap<MessageId, String>
}

impl BoardManager {
    pub fn new() -> BoardManager {
        BoardManager {
            boards: HashMap::new(),
            active_boards: HashMap::new()
        }
    }

    pub fn boards(&self) -> impl Iterator<Item = &Board> {
        self.boards.values()
    }

    /// Adds the given board to this manager, if there is no other board with
    /// the same name. Returns true if the addition was successful, i.e. there
    /// was no other board with the same name, and false otherwise.
    pub fn add_board(&mut self, board: Board) -> bool {
        if !self.boards.contains_key(&board.name) {
            self.boards.insert(board.name.clone(), board);
            true
        }
        else {
            false
        }
    }

    fn active_board(&self, message_id: MessageId) -> Option<&Board> {
        self.active_boards.get(&message_id)
            .and_then(|name| self.boards.get(name))
    }
}

async fn with_board_manager<T, F>(ctx: &Context, guild_id: GuildId, f: F) -> T
where
    F: FnOnce(&BoardManager) -> T
{
    with_guild_state(ctx, guild_id, |gs| f(gs.board_manager())).await
}

async fn with_board_manager_mut<T, F>(ctx: &Context, guild_id: GuildId, f: F) -> T
where
    F: FnOnce(&mut BoardManager) -> T
{
    with_guild_state(ctx, guild_id, |mut gs| f(gs.board_manager_mut())).await
}

pub struct BoardButtonEventHandler;

impl EventHandler for BoardButtonEventHandler {
    fn reaction_add<'life0, 'async_trait>(&self, ctx: Context,
        add_reaction: Reaction)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait
    {
        Box::pin(async move {
            let sender = match add_reaction.user(&ctx).await {
                Ok(u) => u,
                Err(e) => {
                    log::error!("Could not find reaction sender: {}", e);
                    return;
                }
            };

            if sender.id == ctx.cache.current_user_id().await {
                return;
            }

            if let Some(guild_id) = add_reaction.guild_id {
                let command = with_board_manager(&ctx, guild_id, |board_mgr|
                    board_mgr.active_board(add_reaction.message_id)
                        .and_then(|b| b.buttons.iter()
                            .find(|btn| &btn.emote == &add_reaction.emoji)
                            .map(|btn| &btn.command))
                        .cloned()).await;

                if let Some(command) = command {
                    if let Err(e) = add_reaction.delete(&ctx).await {
                        log::error!("Could not remove reaction of sound board: {}", e);
                        return;
                    }

                    let channel_id = add_reaction.channel_id.0;
                    let message_id = add_reaction.message_id.0;
                    let mut msg = ctx.http.get_message(channel_id, message_id)
                        .await.unwrap();

                    msg.content = command;
                    msg.author = sender;
                    msg.webhook_id = None;

                    // For some reason, this becomes unset
                    msg.guild_id = Some(guild_id);

                    let framework = Arc::clone(&ctx.data
                        .read()
                        .await
                        .get::<FrameworkTypeMapKey>()
                        .unwrap());

                    framework.dispatch(ctx, msg).await;
                }
            }
        })
    }
}
