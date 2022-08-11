use crate::FrameworkTypeMapKey;
use crate::command::{
    configure_guild_state,
    with_guild_state,
    with_guild_state_mut_unguarded,
    unwrap_or_return
};

use rambot_proc_macro::rambot_command;

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use serenity::builder::{CreateComponents, CreateActionRow, CreateButton};
use serenity::client::{Context, EventHandler};
use serenity::framework::standard::{CommandGroup, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::application::interaction::{
    Interaction,
    InteractionResponseType
};
use serenity::model::channel::MessageType;
use serenity::model::id::{MessageId, GuildId};
use serenity::model::prelude::Message;

mod button;

use button::BUTTONCMD_GROUP;

#[group("Board")]
#[prefix("board")]
#[commands(add, remove, display, list)]
#[sub_groups(buttoncmd)]
struct BoardCmd;

/// Gets a [CommandGroup] for the commands with prefix `board`.
pub fn get_board_commands() -> &'static CommandGroup {
    &BOARDCMD_GROUP
}

#[rambot_command(
    description = "Adds a new, empty board with the given name. If there is \
        already a board with the same name, nothing is changed and an \
        appropriate reply is sent.",
    usage = "name",
    confirm
)]
async fn add(ctx: &Context, msg: &Message, name: String)
        -> CommandResult<Option<String>> {
    let guild_id = msg.guild_id.unwrap();
    let added = configure_board_manager(ctx, guild_id, |board_mgr| {
        board_mgr.add_board(Board {
            name,
            buttons: Vec::new()
        })
    }).await;

    if added {
        Ok(None)
    }
    else {
        Ok(Some("There is already a board with that name.".to_owned()))
    }
}

#[rambot_command(
    description = "Removes the sound board with the given name.",
    usage = "name",
    confirm
)]
async fn remove(ctx: &Context, msg: &Message, name: String)
        -> CommandResult<Option<String>> {
    let guild_id = msg.guild_id.unwrap();
    let found = configure_board_manager(ctx, guild_id, |board_mgr| {
        if board_mgr.boards.remove(&name).is_some() {
            board_mgr.active_boards.retain(|_, v| v.name != name);
            true
        }
        else {
            false
        }
    }).await;

    if found {
        Ok(None)
    }
    else {
        Ok(Some("I did not find any board with that name.".to_owned()))
    }
}

#[rambot_command(
    description = "Displays the sound board with the given name.",
    usage = "name"
)]
async fn display(ctx: &Context, msg: &Message, name: String)
        -> CommandResult<Option<String>> {
    let guild_id = msg.guild_id.unwrap();
    let board_res = unwrap_or_return!(with_board_manager(ctx, guild_id,
        |board_mgr| {
            if let Some(board) = board_mgr.boards.get(&name) {
                Ok(board.clone())
            }
            else {
                Err(format!("I found no board with name `{}`.", name))
            }
        }).await, Ok(Some(format!("I found no board with name `{}`.", name))));

    match board_res {
        Ok(board) => {
            let content = format!("**{}**\n", name);
            let mut board_msgs = Vec::new();

            board_msgs.push(msg.channel_id.send_message(ctx, |m| {
                m.content(content)
                    .components(|c| board.add_as_components(c, 0))
            }).await?);

            for page in 1..board.page_count() {
                board_msgs.push(msg.channel_id.send_message(ctx, |m| {
                    m.components(|c| board.add_as_components(c, page))
                }).await?);
            }

            for board_msg in board_msgs {
                let name = name.clone();

                with_guild_state_mut_unguarded(ctx, guild_id, |gs| {
                    gs.board_manager_mut().activate(&name, board_msg.id);
                }).await;
            }

            Ok(None)
        },
        Err(e) => Ok(Some(e))
    }
}

#[rambot_command(
    description = "Lists all sound boards that are available on this guild.",
    usage = ""
)]
async fn list(ctx: &Context, msg: &Message) -> CommandResult<Option<String>> {
    let guild_id = msg.guild_id.unwrap();
    let mut names = unwrap_or_return!(with_board_manager(ctx, guild_id,
        |board_mgr| {
            board_mgr.boards()
                .map(|b| b.name.clone())
                .collect::<Vec<_>>()
        }).await,
        Ok(Some("I found no sound boards in this guild.".to_owned())));
    names.sort();
    let mut reply = "Sound boards:".to_owned();

    for name in names {
        reply.push_str("\n - `");
        reply.push_str(&name);
        reply.push('`');
    }

    msg.reply(ctx, reply).await?;
    Ok(None)
}

/// A single button on a sound board, which is represented by a single reaction
/// and executes a single command when pressed.
#[derive(Clone, Deserialize, Serialize)]
pub struct Button {
    label: String,
    command: String
}

impl Button {
    fn component_button(&self, index: usize) -> CreateButton {
        let mut button = CreateButton::default();

        button.label(&self.label)
            .custom_id(format!("{}", index));

        button
    }
}

/// A sound board which constitutes one message when displayed. The message
/// will get one reaction for each button.
#[derive(Clone, Deserialize, Serialize)]
pub struct Board {
    name: String,
    buttons: Vec<Button>
}

const MAX_BUTTONS_PER_ROW: usize = 5;
const MAX_ROWS_PER_MESSAGE: usize = 5;
const MAX_BUTTONS_PER_MESSAGE: usize =
    MAX_ROWS_PER_MESSAGE * MAX_BUTTONS_PER_ROW;

impl Board {
    fn page_count(&self) -> usize {
        (self.buttons.len() + MAX_BUTTONS_PER_MESSAGE - 1) / MAX_BUTTONS_PER_MESSAGE
    }

    fn action_row(&self, button_row: &[Button], base_idx: usize) -> CreateActionRow {
        let mut action_row = CreateActionRow::default();
    
        for (d_idx, button) in button_row.iter().enumerate() {
            let index = base_idx * MAX_BUTTONS_PER_ROW + d_idx;
            action_row.add_button(button.component_button(index));
        }
    
        action_row
    }

    fn add_as_components<'comp>(&self, c: &'comp mut CreateComponents,
            page: usize) -> &'comp mut CreateComponents {
        let row_offset = page * MAX_ROWS_PER_MESSAGE;
        let chunks = self.buttons.chunks(MAX_BUTTONS_PER_ROW)
            .skip(row_offset)
            .take(MAX_ROWS_PER_MESSAGE);

        for (row_idx, button_row) in chunks.enumerate() {
            let base_idx = row_idx + row_offset;
            c.add_action_row(self.action_row(button_row, base_idx));
        }

        c
    }
}

/// Manages all sound boards of a guild.
pub struct BoardManager {
    boards: HashMap<String, Board>,
    active_boards: HashMap<MessageId, Board>
}

impl Default for BoardManager {
    fn default() -> BoardManager {
        BoardManager::new()
    }
}

impl BoardManager {

    /// Creates a new board manager with initially no sound boards.
    pub fn new() -> BoardManager {
        BoardManager {
            boards: HashMap::new(),
            active_boards: HashMap::new()
        }
    }

    /// Gets an iterator over all sound [Board]s managed by this board manager.
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

    fn activate(&mut self, name: &str, message_id: MessageId) {
        if let Some(board) = self.boards.get(name) {
            self.active_boards.insert(message_id, board.clone());
        }
    }

    fn active_board(&self, message_id: MessageId) -> Option<&Board> {
        self.active_boards.get(&message_id)
    }
}

async fn with_board_manager<T, F>(ctx: &Context, guild_id: GuildId, f: F)
    -> Option<T>
where
    F: FnOnce(&BoardManager) -> T
{
    with_guild_state(ctx, guild_id, |gs| f(gs.board_manager())).await
}

async fn configure_board_manager<T, F>(ctx: &Context, guild_id: GuildId, f: F)
    -> T
where
    F: FnOnce(&mut BoardManager) -> T
{
    configure_guild_state(ctx, guild_id, |mut gs| f(gs.board_manager_mut())).await
}

/// An [EventHandler] which listens for reactions added to sound board messages
/// and determines whether these constitute button presses. If such events are
/// detected, the commands associated with the pressed button are executed and
/// the reaction is removed, making the button pressable again.
pub struct BoardButtonEventHandler;

#[async_trait::async_trait]
impl EventHandler for BoardButtonEventHandler {
    async fn interaction_create(&self, ctx: Context,
            interaction: Interaction) {
        let interaction = match interaction {
            Interaction::MessageComponent(c) => c,
            _ => return
        };

        if let Some(guild_id) = interaction.guild_id {
            let command = with_board_manager(&ctx, guild_id, |board_mgr|
                board_mgr.active_board(interaction.message.id)
                    .and_then(|b| {
                        let id: usize = interaction.data.custom_id.parse()
                            .unwrap();
                        b.buttons.get(id)
                    })
                    .map(|b| &b.command)
                    .cloned()).await.flatten();

            if let Some(command) = command {
                let channel_id = interaction.channel_id.0;
                let message_id = interaction.message.id.0;
                let mut msg = ctx.http.get_message(channel_id, message_id)
                    .await.unwrap();

                msg.content = command;
                msg.author = interaction.user.clone();
                msg.webhook_id = None;
                msg.kind = MessageType::Unknown; // Prevents :ok_hand:

                // For some reason, this becomes unset
                msg.guild_id = Some(guild_id);

                let framework = Arc::clone(ctx.data
                    .read()
                    .await
                    .get::<FrameworkTypeMapKey>()
                    .unwrap());

                interaction.create_interaction_response(&ctx, |r|
                    r.kind(InteractionResponseType::DeferredUpdateMessage)).await.unwrap();
                framework.dispatch(ctx, msg).await;
            }
        }
    }
}
