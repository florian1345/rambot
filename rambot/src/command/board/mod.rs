use crate::FrameworkTypeMapKey;
use crate::command::{
    get_guild_state,
    get_guild_state_mut,
    get_guild_state_mut_unguarded,
    GuildStateRef,
    GuildStateMut,
    GuildStateMutUnguarded,
    unwrap_or_return
};

use rambot_proc_macro::rambot_command;

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use serenity::builder::{CreateComponents, CreateActionRow, CreateButton};
use serenity::client::{Context, EventHandler};
use serenity::framework::standard::{CommandGroup, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::application::component::ButtonStyle;
use serenity::model::application::interaction::{
    Interaction,
    InteractionResponseType
};
use serenity::model::channel::MessageType;
use serenity::model::id::{MessageId, GuildId};
use serenity::model::prelude::{Message, ChannelId};

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
    let mut board_mgr = get_board_manager_mut(ctx, guild_id).await;
    let board = Board {
        name,
        buttons: Vec::new()
    };

    if board_mgr.add_board(board) {
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
    let mut board_mgr = get_board_manager_mut(ctx, guild_id).await;
    board_mgr.deactivate_board(ctx, &name).await?;

    if board_mgr.boards.remove(&name).is_some() {
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
    let channel_id = msg.channel_id;
    let mut board_mgr = unwrap_or_return!(
        get_board_manager_mut_unguarded(ctx, guild_id).await,
        Ok(Some(format!("I found no board with name `{}`.", name))));

    board_mgr.deactivate_board(ctx, &name).await?;
    let success = board_mgr.activate_board(ctx, &name, channel_id).await?;

    if success {
        Ok(None)
    }
    else {
        Ok(Some(format!("I found no board with name `{}`.", name)))
    }
}

#[rambot_command(
    description = "Lists all sound boards that are available on this guild.",
    usage = ""
)]
async fn list(ctx: &Context, msg: &Message) -> CommandResult<Option<String>> {
    let guild_id = msg.guild_id.unwrap();
    let board_mgr = unwrap_or_return!(get_board_manager(ctx, guild_id).await,
        Ok(Some("I found no sound boards in this guild.".to_owned())));
    let mut names = board_mgr.boards()
        .map(|b| b.name.clone())
        .collect::<Vec<_>>();
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
    command: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    deactivate_command: Option<String>,
    active: bool
}

impl Button {
    fn component_button(&self, index: usize) -> CreateButton {
        let mut button = CreateButton::default();
        let style = if self.active {
            ButtonStyle::Primary
        }
        else {
            ButtonStyle::Secondary
        };

        button.label(&self.label)
            .custom_id(format!("{}", index))
            .style(style);

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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct UniqueMessageId {
    channel_id: ChannelId,
    message_id: MessageId
}

/// Manages all sound boards of a guild.
pub struct BoardManager {
    boards: HashMap<String, Board>,
    active_board_messages: HashMap<String, Vec<UniqueMessageId>>,
    active_board_names: HashMap<UniqueMessageId, String>
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
            active_board_messages: HashMap::new(),
            active_board_names: HashMap::new()
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

    /// Removes all messages associated with any active instance of the board
    /// with the given name. Returns `true` if there was an active instance.
    /// Raises an error if deleting any message failed.
    async fn deactivate_board(&mut self, ctx: &Context, name: &str)
            -> CommandResult<bool> {
        if let Some(pages) = self.active_board_messages.remove(name) {
            for page in pages {
                let unique_id = UniqueMessageId {
                    channel_id: page.channel_id,
                    message_id: page.message_id
                };
                let channel_id = page.channel_id.0;
                let message_id = page.message_id.0;
                let message =
                    ctx.http.get_message(channel_id, message_id).await;

                self.active_board_names.remove(&unique_id);

                if let Ok(message) = message {
                    message.delete(ctx).await?;
                }
            }

            Ok(true)
        }
        else {
            Ok(false)
        }
    }

    /// Posts messages in the channel with the given ID representing the board
    /// with the given name. Returns `true` if there was a board with that
    /// name. Raises an error if sending any message failed.
    async fn activate_board(&mut self, ctx: &Context, name: &str,
            channel_id: ChannelId) -> CommandResult<bool> {
        let board = match self.boards.get(name) {
            Some(b) => b,
            _ => return Ok(false)
        };
        let content = format!("**{}**\n", name);
        let mut board_msgs = Vec::new();

        board_msgs.push(channel_id.send_message(ctx, |m| {
            m.content(content)
                .components(|c| board.add_as_components(c, 0))
        }).await?);

        for page in 1..board.page_count() {
            board_msgs.push(channel_id.send_message(ctx, |m| {
                m.components(|c| board.add_as_components(c, page))
            }).await?);
        }

        let mut unique_msg_ids = Vec::new();

        for msg in board_msgs {
            let unique_id = UniqueMessageId {
                channel_id,
                message_id: msg.id
            };

            self.active_board_names.insert(unique_id, name.to_owned());
            unique_msg_ids.push(unique_id);
        }

        self.active_board_messages.insert(name.to_owned(), unique_msg_ids);
        Ok(true)
    }

    fn active_board(&self, message_id: MessageId, channel_id: ChannelId)
            -> Option<&Board> {
        let unique_id = UniqueMessageId {
            channel_id,
            message_id
        };

        self.active_board_names.get(&unique_id)
            .and_then(|name| self.boards.get(name))
    }

    fn active_board_mut(&mut self, message_id: MessageId,
            channel_id: ChannelId) -> Option<&mut Board> {
        let unique_id = UniqueMessageId {
            channel_id,
            message_id
        };

        self.active_board_names.get(&unique_id)
            .and_then(|name| self.boards.get_mut(name))
    }
}

struct BoardManagerRef<'a> {
    guild_state: GuildStateRef<'a>
}

impl<'a> Deref for BoardManagerRef<'a> {
    type Target = BoardManager;

    fn deref(&self) -> &BoardManager {
        self.guild_state.board_manager()
    }
}

struct BoardManagerMutUnguarded<'a> {
    guild_state: GuildStateMutUnguarded<'a>
}

impl<'a> Deref for BoardManagerMutUnguarded<'a> {
    type Target = BoardManager;

    fn deref(&self) -> &BoardManager {
        self.guild_state.board_manager()
    }
}

impl<'a> DerefMut for BoardManagerMutUnguarded<'a> {
    fn deref_mut(&mut self) -> &mut BoardManager {
        self.guild_state.board_manager_mut()
    }
}

struct BoardManagerMut<'a> {
    guild_state: GuildStateMut<'a>
}

impl<'a> Deref for BoardManagerMut<'a> {
    type Target = BoardManager;

    fn deref(&self) -> &BoardManager {
        self.guild_state.board_manager()
    }
}

impl<'a> DerefMut for BoardManagerMut<'a> {
    fn deref_mut(&mut self) -> &mut BoardManager {
        self.guild_state.board_manager_mut()
    }
}

async fn get_board_manager(ctx: &Context, guild_id: GuildId)
        -> Option<BoardManagerRef<'_>> {
    get_guild_state(ctx, guild_id).await.map(|guild_state|
        BoardManagerRef {
            guild_state
        })
}

async fn get_board_manager_mut_unguarded(ctx: &Context, guild_id: GuildId)
        -> Option<BoardManagerMutUnguarded<'_>> {
    get_guild_state_mut_unguarded(ctx, guild_id).await.map(|guild_state|
        BoardManagerMutUnguarded {
            guild_state
        })
}

async fn get_board_manager_mut(ctx: &Context, guild_id: GuildId)
        -> BoardManagerMut<'_> {
    BoardManagerMut {
        guild_state: get_guild_state_mut(ctx, guild_id).await
    }
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
            // Find the button

            let channel_id = interaction.channel_id;
            let message_id = interaction.message.id;
            let button_id: usize = interaction.data.custom_id.parse().unwrap();
            let board_manager =
                unwrap_or_return!(get_board_manager(&ctx, guild_id).await, ());
            let button = unwrap_or_return!(
                board_manager.active_board(message_id, channel_id)
                    .and_then(|b| b.buttons.get(button_id))
                    .cloned(), ());

            drop(board_manager);

            // Determine the command to execute

            let mut command = button.command;
            let mut msg = ctx.http.get_message(channel_id.0, message_id.0)
                .await.unwrap();

            if let Some(deactivate_command) = button.deactivate_command {
                // The button is a toggle button => if active, run the
                // deactivate command instead, and switch the toggle state.

                if button.active {
                    command = deactivate_command;
                }

                let mut board_manager =
                    get_board_manager_mut(&ctx, guild_id).await;
                let board = board_manager
                    .active_board_mut(message_id, channel_id).unwrap();
                let button = board.buttons.get_mut(button_id).unwrap();

                button.active = !button.active;

                // Refresh the message to account for new toggle state.

                let page = button_id / MAX_BUTTONS_PER_MESSAGE;
                let res = msg.edit(&ctx, |edit| {
                    edit.components(|components| {
                        board.add_as_components(components, page)
                    })
                }).await;

                if let Err(e) = res {
                    log::warn!("Error updating sound board components: {}", e);
                    return;
                }
            }

            // Execute the command

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

            let res = interaction.create_interaction_response(&ctx, |r|
                r.kind(InteractionResponseType::DeferredUpdateMessage)).await;

            if let Err(e) = res {
                log::warn!("Error posting interaction response: {}", e);
                return;
            }

            framework.dispatch(ctx, msg).await;
        }
    }
}
