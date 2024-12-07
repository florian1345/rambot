use crate::command::{
    display_help,
    get_guild_state,
    get_guild_state_mut,
    get_guild_state_mut_unguarded,
    respond,
    CommandData,
    CommandResult,
    Context,
    GuildStateMut,
    GuildStateMutUnguarded,
    GuildStateRef,
    unwrap_or_return,
    CommandError,
    CommandResponse
};

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use poise::FrameworkContext;

use serde::{Deserialize, Serialize};

use serenity::all::{ButtonStyle, ComponentInteraction, CreateInteractionResponse, CreateMessage, EditMessage, FullEvent, Interaction};
use serenity::builder::{CreateActionRow, CreateButton};
use serenity::model::id::{MessageId, GuildId};
use serenity::model::prelude::ChannelId;
use serenity::prelude::Context as SerenityContext;

use tokio::sync::RwLock;

mod button;

use button::button;
use crate::command;
use crate::event::FrameworkEventHandler;

/// Collection of commands for managing sound boards.
#[poise::command(slash_command, prefix_command,
    subcommands("add", "button", "remove", "display", "list"))]
pub async fn board(ctx: Context<'_>) -> CommandResult {
    display_help(ctx, Some("board")).await
}

/// Adds a new, empty board with the given name.
///
/// If there is already a board with the same name, nothing is changed and an appropriate reply is
/// sent.
///
/// Usage: `board add <name>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn add(ctx: Context<'_>, name: String) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let mut board_mgr = get_board_manager_mut(ctx.data(), guild_id).await;
    let board = Board {
        name,
        buttons: Vec::new()
    };

    let response = if board_mgr.add_board(board) {
        CommandResponse::Confirm
    }
    else {
        CommandResponse::Reply("There is already a board with that name.")
    };

    respond(ctx, response).await
}

/// Removes the sound board with the given name.
///
/// Usage: `board remove <name>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn remove(ctx: Context<'_>, name: String) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let mut board_mgr = get_board_manager_mut(ctx.data(), guild_id).await;
    board_mgr.deactivate_board(ctx, &name).await?;

    let response = if board_mgr.boards.remove(&name).is_some() {
        CommandResponse::Confirm
    }
    else {
        CommandResponse::Reply("I did not find any board with that name.")
    };

    respond(ctx, response).await
}

async fn display_do(ctx: Context<'_>, name: String) -> CommandResult<CommandResponse> {
    let guild_id = ctx.guild_id().unwrap();
    let channel_id = ctx.channel_id();
    let mut board_mgr = unwrap_or_return!(
        get_board_manager_mut_unguarded(ctx.data(), guild_id).await,
        Ok(format!("I found no board with name `{}`.", name).into()));

    board_mgr.deactivate_board(ctx, &name).await?;
    let success = board_mgr.activate_board(ctx, &name, channel_id).await?;

    if success {
        Ok(CommandResponse::Confirm)
    }
    else {
        Ok(format!("I found no board with name `{}`.", name).into())
    }
}

/// Displays the sound board with the given name.
///
/// Usage: `board display <name>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn display(ctx: Context<'_>, name: String) -> CommandResult {
    let response = display_do(ctx, name).await?;
    respond(ctx, response).await
}

async fn get_list_message(ctx: Context<'_>) -> String {
    let guild_id = ctx.guild_id().unwrap();
    let board_mgr = unwrap_or_return!(get_board_manager(ctx.data(), guild_id).await,
        "I found no sound boards in this guild.".to_owned());
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

    reply
}

/// Lists all sound boards that are available on this guild.
///
/// Usage: `board list`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn list(ctx: Context<'_>) -> CommandResult {
    ctx.reply(get_list_message(ctx).await).await?;
    Ok(())
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
        let style = if self.active {
            ButtonStyle::Primary
        }
        else {
            ButtonStyle::Secondary
        };

        CreateButton::new(format!("{}", index))
            .label(&self.label)
            .style(style)
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
        let mut buttons = Vec::new();
    
        for (d_idx, button) in button_row.iter().enumerate() {
            let index = base_idx * MAX_BUTTONS_PER_ROW + d_idx;
            buttons.push(button.component_button(index));
        }
    
        CreateActionRow::Buttons(buttons)
    }

    fn as_components(&self, page: usize) -> Vec<CreateActionRow> {
        let row_offset = page * MAX_ROWS_PER_MESSAGE;

        self.buttons.chunks(MAX_BUTTONS_PER_ROW)
            .skip(row_offset)
            .take(MAX_ROWS_PER_MESSAGE)
            .enumerate()
            .map(|(row_idx, button_row)| {
                let base_idx = row_idx + row_offset;
                self.action_row(button_row, base_idx)
            })
            .collect()
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
    async fn deactivate_board(&mut self, ctx: Context<'_>, name: &str)
            -> CommandResult<bool> {
        if let Some(pages) = self.active_board_messages.remove(name) {
            for page in pages {
                let unique_id = UniqueMessageId {
                    channel_id: page.channel_id,
                    message_id: page.message_id
                };
                let channel_id = page.channel_id;
                let message_id = page.message_id;
                let message =
                    ctx.http().get_message(channel_id, message_id).await;

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
    async fn activate_board(&mut self, ctx: Context<'_>, name: &str,
            channel_id: ChannelId) -> CommandResult<bool> {
        let board = match self.boards.get(name) {
            Some(b) => b,
            _ => return Ok(false)
        };
        let content = format!("**{}**\n", name);
        let mut board_msgs = Vec::new();

        board_msgs.push(channel_id.send_message(ctx, CreateMessage::new()
            .content(content)
            .components(board.as_components(0))).await?);

        for page in 1..board.page_count() {
            board_msgs.push(channel_id.send_message(ctx, CreateMessage::new()
                .components(board.as_components(page))).await?);
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

async fn get_board_manager(data: &RwLock<CommandData>, guild_id: GuildId)
        -> Option<BoardManagerRef<'_>> {
    get_guild_state(data, guild_id).await.map(|guild_state|
        BoardManagerRef {
            guild_state
        })
}

async fn get_board_manager_mut_unguarded(data: &RwLock<CommandData>, guild_id: GuildId)
        -> Option<BoardManagerMutUnguarded<'_>> {
    get_guild_state_mut_unguarded(data, guild_id).await.map(|guild_state|
        BoardManagerMutUnguarded {
            guild_state
        })
}

async fn get_board_manager_mut(data: &RwLock<CommandData>, guild_id: GuildId)
        -> BoardManagerMut<'_> {
    BoardManagerMut {
        guild_state: get_guild_state_mut(data, guild_id).await
    }
}

/// An [EventHandler] which listens for reactions added to sound board messages
/// and determines whether these constitute button presses. If such events are
/// detected, the commands associated with the pressed button are executed and
/// the reaction is removed, making the button pressable again.
pub struct BoardButtonEventHandler;

impl FrameworkEventHandler for BoardButtonEventHandler {
    async fn handle_event(&self, serenity_ctx: &SerenityContext, event: &FullEvent,
            framework_ctx: FrameworkContext<'_, RwLock<CommandData>, CommandError>)
            -> CommandResult {
        if let FullEvent::InteractionCreate {
                    interaction: interaction @ Interaction::Component(ComponentInteraction {
                        guild_id: Some(guild_id), ..
                    })
                } = event {
            // Find the button

            let guild_id = *guild_id;
            let message_component = interaction.as_message_component().unwrap();
            let channel_id = message_component.channel_id;
            let message_id = message_component.message.id;
            let button_id: usize = message_component.data.custom_id.parse().unwrap();
            let board_manager = unwrap_or_return!(
                get_board_manager(framework_ctx.user_data, guild_id).await, Ok(()));
            let button = unwrap_or_return!(
                board_manager.active_board(message_id, channel_id)
                    .and_then(|b| b.buttons.get(button_id))
                    .cloned(), Ok(()));

            drop(board_manager);

            // Determine the command to execute

            let mut command = button.command;
            let mut msg = serenity_ctx.http.get_message(channel_id, message_id)
                .await.unwrap();

            if let Some(deactivate_command) = button.deactivate_command {
                // The button is a toggle button => if active, run the
                // deactivate command instead, and switch the toggle state.

                if button.active {
                    command = deactivate_command;
                }

                let mut board_manager =
                    get_board_manager_mut(framework_ctx.user_data, guild_id).await;
                let board = board_manager
                    .active_board_mut(message_id, channel_id).unwrap();
                let button = board.buttons.get_mut(button_id).unwrap();

                button.active = !button.active;

                // Refresh the message to account for new toggle state.

                let page = button_id / MAX_BUTTONS_PER_MESSAGE;
                let res = msg.edit(&serenity_ctx, EditMessage::new()
                    .components(board.as_components(page)))
                    .await;

                if let Err(e) = res {
                    log::warn!("Error updating sound board components: {}", e);
                    return Ok(());
                }
            }

            // Execute the command

            msg.content = command;
            msg.author = message_component.user.clone();
            msg.webhook_id = None;

            // For some reason, this becomes unset
            msg.guild_id = Some(guild_id);

            command::dispatch_command_as_message(framework_ctx, serenity_ctx, &msg).await?;

            let res = message_component.create_response(&serenity_ctx,
                CreateInteractionResponse::Acknowledge).await;

            if let Err(e) = res {
                log::warn!("Error posting interaction response: {}", e);
            }
        }

        Ok(())
    }
}
