use crate::audio::{PCMRead, Layer, Mixer};
use crate::key_value::KeyValueDescriptor;
use crate::plugin::PluginManager;
use crate::state::{State, GuildState};

use rambot_api::{AudioSource, ModifierDocumentation, PluginGuildConfig, SampleDuration, SAMPLES_PER_SECOND};

use serenity::model::id::GuildId;
use serenity::prelude::TypeMap;
use serenity::prelude::Context as SerenityContext;
use serenity::model::channel::Message as SerenityMessage;

use songbird::Call;
use songbird::error::JoinError;
use songbird::input::{Input, RawAdapter};

use poise::{builtins, Command, FrameworkContext, MessageDispatchTrigger};

use std::any::Any;
use std::clone::Clone;
use std::collections::hash_map::Keys;
use std::fmt::{Display, Write};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, RwLock, RwLockWriteGuard};
use poise::builtins::HelpConfiguration;
use serenity::all::{CreateInteractionResponse, CreateInteractionResponseMessage};
use tokio::runtime::{Handle, Runtime};
use tokio::sync::{Mutex as TokioMutex, Mutex};
use tokio::sync::MutexGuard as TokioMutexGuard;
use tokio::sync::RwLock as TokioRwLock;
use tokio::sync::RwLockReadGuard as TokioRwLockReadGuard;
use tokio::sync::RwLockWriteGuard as TokioRwLockWriteGuard;

mod adapter;
pub mod board;
mod effect;
mod layer;

pub use board::BoardButtonEventHandler;

// TODO TypeMap is no longer necessary
pub type CommandData = TypeMap;
pub type CommandError = Box<dyn std::error::Error + Send + Sync>;
pub type CommandResult<T = ()> = Result<T, CommandError>;
pub type Context<'a> = poise::Context<'a, TokioRwLock<CommandData>, CommandError>;

macro_rules! unwrap_or_return {
    ($e:expr, $r:expr) => {
        match $e {
            Some(v) => v,
            None => return $r
        }
    }
}

pub(crate) use unwrap_or_return;

/// Gets a vector of the root commands. Sub-commands are not included, but their parent is.
pub fn commands() -> Vec<Command<TokioRwLock<CommandData>, CommandError>> {
    vec![
        adapter::adapter(),
        audio(),
        board::board(),
        connect(),
        directory(),
        disconnect(),
        cmd_do(),
        effect::effect(),
        help(),
        info(),
        layer::layer(),
        play(),
        seek(),
        skip(),
        stop()
    ]
}

macro_rules! unwrap_or_reply {
    ($matched_expression:expr, $ctx:expr, $reply:expr) => {
        match $matched_expression {
            Some(v) => v,
            None => {
                $ctx.reply($reply).await?;
                return Ok(());
            }
        }
    }
}

pub(crate) use unwrap_or_reply;

/// Connects to the channel of the command author. Returns `true` if and only if the bot is
/// connected afterward.
async fn connect_do(ctx: Context<'_>, call: TokioMutexGuard<'_, Call>) -> (bool, CommandResponse) {
    let guild_id = ctx.guild_id().unwrap();
    let channel_id_opt = ctx.guild().unwrap().voice_states
        .get(&ctx.author().id)
        .and_then(|v| v.channel_id);
    let channel_id = match channel_id_opt{
        Some(id) => id,
        None => {
            return (false, "I cannot see your voice channel. Are you connected?".into());
        }
    };

    if let Some(channel) = call.current_channel() {
        if channel.0.get() == channel_id.get() {
            return (true, "I am already connected to your voice channel.".into());
        }
    }

    drop(call);

    log::debug!("Joining channel {} on guild {}.", channel_id, guild_id);

    let songbird = songbird::get(ctx.serenity_context()).await.unwrap();
    songbird.join(guild_id, channel_id).await.unwrap();

    (true, CommandResponse::Confirm)
}

/// Connects the bot to the voice channel to which the sender of the command is currently connected.
///
/// Usage: `connect`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn connect(ctx: Context<'_>) -> CommandResult {
    let call = get_songbird_call(ctx).await;
    let (_, response) = connect_do(ctx, call.lock().await).await;
    respond(ctx, response).await
}

async fn get_songbird_call(ctx: Context<'_>) -> Arc<TokioMutex<Call>> {
    let guild_id = ctx.guild_id().unwrap();
    songbird::get(ctx.serenity_context()).await.unwrap().get_or_insert(guild_id)
}

const NOT_CONNECTED: &str = "I am not connected to a voice channel";

/// Disconnects the bot from the voice channel to which it is currently connected.
///
/// Usage: `disconnect`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn disconnect(ctx: Context<'_>) -> CommandResult {
    let response = if let Some(songbird) = songbird::get(ctx.serenity_context()).await {
        let guild_id = ctx.guild_id().unwrap();

        match songbird.remove(guild_id).await {
            Ok(_) => {
                stop_all(ctx).await; // drop audio sources
                log::debug!("Left voice on guild {}.", guild_id);
                CommandResponse::Confirm
            },
            Err(JoinError::NoCall) => CommandResponse::Reply(NOT_CONNECTED),
            Err(e) => return Err(e.into())
        }
    }
    else {
        log::error!("No songbird instance found.");
        CommandResponse::Reply("Internal error: No songbird instance found.")
    };

    respond(ctx, response).await
}

fn to_input<S>(source: Arc<RwLock<S>>) -> Input
where
    S: AudioSource + Send + Sync + 'static
{
    let read = PCMRead::new(source);
    RawAdapter::new(read, SAMPLES_PER_SECOND as u32, 2).into()
}

struct GuildStateRef<'a> {
    data: TokioRwLockReadGuard<'a, CommandData>,
    guild_id: GuildId
}

impl Deref for GuildStateRef<'_> {
    type Target = GuildState;

    fn deref(&self) -> &GuildState {
        let state = self.data.get::<State>().unwrap();
        state.guild_state(self.guild_id).unwrap()
    }
}

struct GuildStateMutUnguarded<'a> {
    data_guard: TokioRwLockWriteGuard<'a, TypeMap>,
    guild_id: GuildId
}

impl Deref for GuildStateMutUnguarded<'_> {
    type Target = GuildState;

    fn deref(&self) -> &GuildState {
        let state = self.data_guard.get::<State>().unwrap();
        state.guild_state(self.guild_id).unwrap()
    }
}

impl DerefMut for GuildStateMutUnguarded<'_> {
    fn deref_mut(&mut self) -> &mut GuildState {
        let state_mut = self.data_guard.get_mut::<State>().unwrap();
        state_mut.guild_state_mut_unguarded(self.guild_id).unwrap()
    }
}

struct GuildStateMut<'a> {
    data_guard: TokioRwLockWriteGuard<'a, TypeMap>,
    guild_id: GuildId,
    plugin_manager: Arc<PluginManager>
}

impl Deref for GuildStateMut<'_> {
    type Target = GuildState;

    fn deref(&self) -> &GuildState {
        let state = self.data_guard.get::<State>().unwrap();
        state.guild_state(self.guild_id).unwrap()
    }
}

impl DerefMut for GuildStateMut<'_> {
    fn deref_mut(&mut self) -> &mut GuildState {
        // Sadly, we cannot use a GuildStateGuard here for two reasons.
        // 1. We would need to deref it to obtain an actual &mut reference,
        //    which would mean we return a reference to a temporary variable.
        // 2. Multiple mutable accesses would result in saving the state file
        //    each time, which is inefficient.
        // To resolve this, we create a dummy guard whenever GuildStateMut is
        // dropped. Dropping this dummy guard ensures any changes are committed
        // to the state file.

        let state_mut = self.data_guard.get_mut::<State>().unwrap();
        state_mut.guild_state_mut_unguarded(self.guild_id).unwrap()
    }
}

impl Drop for GuildStateMut<'_> {
    fn drop(&mut self) {
        let state_mut = self.data_guard.get_mut::<State>().unwrap();
        let g = state_mut.guild_state_mut(self.guild_id, &self.plugin_manager);

        drop(g);
    }
}

async fn get_guild_state(data: &TokioRwLock<CommandData>, guild_id: GuildId)
        -> Option<GuildStateRef<'_>> {
    let data = data.read().await;

    if data.get::<State>()?.guild_state(guild_id).is_some() {
        Some(GuildStateRef {
            data,
            guild_id
        })
    }
    else {
        None
    }
}

async fn get_guild_state_mut_unguarded(data: &TokioRwLock<CommandData>, guild_id: GuildId)
        -> Option<GuildStateMutUnguarded<'_>> {
    let data_guard = data.write().await;

    if data_guard.get::<State>()?.guild_state(guild_id).is_some() {
        Some(GuildStateMutUnguarded {
            data_guard,
            guild_id
        })
    }
    else {
        None
    }
}

async fn get_guild_state_mut(data: &TokioRwLock<CommandData>, guild_id: GuildId)
        -> GuildStateMut<'_> {
    let mut data_guard = data.write().await;
    let plugin_manager = Arc::clone(data_guard.get::<PluginManager>().unwrap());
    let state = data_guard.get_mut::<State>().unwrap();
    state.ensure_guild_state_exists(guild_id, &plugin_manager);

    GuildStateMut {
        data_guard,
        guild_id,
        plugin_manager
    }
}

fn play_mixer(
    ctx: Context<'_>,
    mixer: Arc<RwLock<Mixer>>,
    layer: &str,
    audio: &str,
    plugin_guild_config: PluginGuildConfig
) -> Result<bool, String> {
    let mut mixer_guard = mixer.write().unwrap();

    if !mixer_guard.contains_layer(layer) {
        return Err(format!("No layer of name {}.", &layer));
    }

    let active_before = mixer_guard.active();
    let serenity_ctx = ctx.serenity_context().clone();
    let channel_id = ctx.channel_id();
    let error_callback = move |layer, e| {
        // TODO this is just asking for trouble.

        let content = format!("Error on layer {}: {}", layer, e);
        let future = channel_id.say(&serenity_ctx, content);

        if let Ok(handle) = Handle::try_current() {
            handle.block_on(future).unwrap();
        }
        else {
            let runtime = Runtime::new().unwrap();
            runtime.block_on(future).unwrap();
        }
    };
    let play_res = mixer_guard.play_on_layer(layer, audio, plugin_guild_config, error_callback);

    if let Err(e) = play_res {
        Err(format!("{}", e))
    }
    else {
        Ok(active_before)
    }
}

async fn play_do(ctx: Context<'_>, layer: String, audio: String) -> CommandResult<CommandResponse> {
    let guild_id = ctx.guild_id().unwrap();
    let guild_state = unwrap_or_return!(get_guild_state(ctx.data(), guild_id).await,
        Ok(CommandResponse::Reply(format!("No layer of name {}.", &layer))));
    let plugin_guild_config = guild_state.build_plugin_guild_config();
    let mixer = guild_state.mixer_arc();
    let play_res = play_mixer(ctx, Arc::clone(&mixer), &layer, &audio, plugin_guild_config);
    let active_before = match play_res {
        Ok(active_before) => active_before,
        Err(message) => return Ok(CommandResponse::Reply(message))
    };
    let call = get_songbird_call(ctx).await;
    let mut call_guard = call.lock().await;

    if !active_before {
        call_guard.play_input(to_input(Arc::clone(&mixer)));
    }

    if call_guard.current_channel().is_none() {
        let (connected, response) = connect_do(ctx, call_guard).await;

        if !connected {
            mixer.write().unwrap().stop_all();
        }

        Ok(response)
    }
    else {
        Ok(CommandResponse::Confirm)
    }
}

/// Plays the given audio on the given layer.
/// 
/// Possible formats for the input depend on the installed plugins.
///
/// Usage: `play <layer> <audio>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn play(ctx: Context<'_>, layer: String, #[rest] audio: String) -> CommandResult {
    let response = play_do(ctx, layer, audio).await?;
    respond(ctx, response).await
}

async fn with_layer_mut<F, E>(ctx: Context<'_>, layer: &str, f: F) -> CommandResponse
where
    F: FnOnce(RwLockWriteGuard<Mixer>, &str) -> Result<(), E>,
    E: Display
{
    let guild_id = ctx.guild_id().unwrap();
    let guild_state = unwrap_or_return!(get_guild_state(ctx.data(), guild_id).await,
        format!("Found no layer with name {}.", layer).into());
    let mixer = guild_state.mixer_mut();

    if mixer.contains_layer(layer) {
        if let Err(e) = f(mixer, layer) {
            CommandResponse::Reply(format!("{}", e))
        }
        else {
            CommandResponse::Confirm
        }
    }
    else {
        CommandResponse::Reply(format!("Found no layer with name {}.", layer))
    }
}

/// Plays the next piece of the list currently played on the given layer.
///
/// If the last piece of the list is active, this stops audio on the layer.
///
/// Usage: `skip <layer>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn skip(ctx: Context<'_>, layer: String)
        -> CommandResult<()> {
    let response = with_layer_mut(ctx, &layer, |mut mixer, layer| mixer.skip_on_layer(layer)).await;

    respond(ctx, response).await
}

async fn stop_layer(ctx: Context<'_>, layer: &str) -> CommandResponse {
    let guild_id = ctx.guild_id().unwrap();
    let guild_state = unwrap_or_return!(get_guild_state(ctx.data(), guild_id).await,
        format!("No layer of name {}.", &layer).into());
    let mut mixer = guild_state.mixer_mut();

    if !mixer.contains_layer(layer) {
        format!("No layer of name {}.", layer).into()
    }
    else if !mixer.stop_layer(layer) {
        "No audio to stop.".into()
    }
    else {
        CommandResponse::Confirm
    }
}

async fn stop_all(ctx: Context<'_>) -> CommandResponse {
    let guild_id = ctx.guild_id().unwrap();
    let guild_state =
        unwrap_or_return!(get_guild_state(ctx.data(), guild_id).await, "No audio to stop.".into());

    if guild_state.mixer_mut().stop_all() {
        CommandResponse::Confirm
    }
    else {
        "No audio to stop.".into()
    }
}

/// Stops the audio currently playing on the given layer or all layers.
///
/// If no layer is given, all audio is stopped.
///
/// Usage: `stop [layer]`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn stop(ctx: Context<'_>, layer: Option<String>) -> CommandResult {
    let response = if let Some(layer) = layer {
        stop_layer(ctx, &layer).await
    }
    else {
        stop_all(ctx).await
    };

    respond(ctx, response).await
}

/// Moves the current position in the audio of the layer with the given by the given amount of time.
///
/// The `delta` is of the format `AhBmCsDmsEsam`, representing `A` hours, `B` minutes, `C` seconds,
/// `D` milliseconds, and `E` samples (at 48 kHz). Omitting and reordering these terms is permitted.
/// Negative deltas are used to seek backwards in time.
///
/// Usage: `seek <layer> <delta>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn seek(ctx: Context<'_>, layer: String, delta: SampleDuration) -> CommandResult {
    let response = with_layer_mut(ctx, &layer,
        |mut mixer, layer| mixer.seek_on_layer(layer, delta)).await;

    respond(ctx, response).await
}

/// Executes commands provided as quoted strings.
///
/// Takes as input a list of quoted strings separated by spaces. These are then executed as commands
/// in order.
///
/// Usage: `do [command] [command] ...`
#[poise::command(prefix_command, guild_only, rename = "do")]
async fn cmd_do(ctx: Context<'_>, commands: Vec<String>) -> CommandResult {
    for command in commands {
        match ctx {
            Context::Application(_) => {
                // TODO figure out how to do in a slash command
            },
            Context::Prefix(ctx) => {
                let mut msg = ctx.msg.clone();
                msg.content = command.to_owned();
                dispatch_command_as_message(ctx.framework(), ctx.serenity_context(), &msg).await?;
            }
        }
    }

    Ok(())
}

/// Lists all plugin-provided types of audio with a short summary.
///
/// If an audio name is provided, a more detailed documentation page for that audio is displayed.
///
/// Usage: `audio [audio]`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn audio(ctx: Context<'_>, audio: Option<String>) -> CommandResult {
    let data_guard = ctx.data().read().await;
    let plugin_manager = data_guard.get::<PluginManager>().unwrap();

    let reply = if let Some(audio) = audio {
        let audio_lower = audio.to_lowercase();
        let doc = plugin_manager.get_audio_documentations()
            .find(|d| d.name().to_lowercase() == audio_lower);

        if let Some(doc) = doc {
            format!("{}", doc)
        }
        else {
            format!("I found no audio of name {}.", audio)
        }
    }
    else {
        let mut message = "Audio types:".to_owned();
        let mut first = true;

        for doc in plugin_manager.get_audio_documentations() {
            if first {
                writeln!(message).unwrap();
                first = false;
            }

            write!(message, "\n- {}", doc.overview_entry()).unwrap();
        }

        message
    };

    respond(ctx, reply.into()).await
}

fn add_line(message: &mut String, name: &str, entry: Option<impl Display>) {
    if let Some(entry) = entry {
        writeln!(message, "{}: {}", name, entry).unwrap();
    }
}

/// Prints information about the audio currently played on the layer with the given name.
///
/// Usage: `info <layer>`
#[poise::command(slash_command, prefix_command, guild_only)]
async fn info(ctx: Context<'_>, layer: String) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let guild_state = unwrap_or_reply!(get_guild_state(ctx.data(), guild_id).await, ctx,
        format!("No layer of name `{}`.", layer));
    let metadata = guild_state.mixer_blocking().layer_metadata(&layer);

    let reply = match metadata {
        Ok(metadata) => {
            let mut message = String::new();

            if let (Some(title), Some(sub_title)) =
                    (metadata.title(), metadata.sub_title()) {
                add_line(&mut message, "Title",
                    Some(format!("{} - {}", title, sub_title)));
            }
            else {
                add_line(&mut message, "Title", metadata.title());
            }

            add_line(&mut message, "From", metadata.super_title());
            add_line(&mut message, "Artist", metadata.artist());
            add_line(&mut message, "Composer", metadata.composer());
            add_line(&mut message, "Lead Performer", metadata.lead_performer());
            add_line(&mut message, "Band/Orchestra", metadata.group_name());
            add_line(&mut message, "Conductor", metadata.conductor());
            add_line(&mut message, "Lyricist", metadata.lyricist());
            add_line(&mut message, "Interpreter", metadata.interpreter());
            add_line(&mut message, "Publisher", metadata.publisher());
            add_line(&mut message, "Album", metadata.album());
            add_line(&mut message, "Track Number", metadata.track());
            add_line(&mut message, "Year", metadata.year());
            add_line(&mut message, "Genre", metadata.genre());

            let mut message = message.trim_end().to_owned();

            if message.is_empty() {
                message = "No information available.".to_owned();
            }

            message
        },
        Err(e) => format!("{}", e)
    };

    respond(ctx, reply.into()).await
}

/// Specify or reset a guild-specific root directory for file system based plugins.
///
/// Omit directory argument to reset to the default root directory specified in the config. Any
/// pieces in playlists that are currently active will continue to be resolved according to the old
/// root directory.
///
/// Usage: `directory [directory]`
#[poise::command(slash_command, prefix_command, guild_only, owners_only)]
async fn directory(ctx: Context<'_>, #[rest] directory: String) -> CommandResult {
    let guild_id = ctx.guild_id().unwrap();
    let mut guild_state = get_guild_state_mut(ctx.data(), guild_id).await;

    if directory.is_empty() {
        guild_state.unset_root_directory()
    }
    else {
        guild_state.set_root_directory(directory)
    }

    confirm(ctx).await
}

/// Display help about all or a specific command.
#[poise::command(prefix_command, slash_command)]
pub async fn help(ctx: Context<'_>,
        #[description = "Specific command to show help about"] command: Option<String>)
        -> CommandResult {
    display_help(ctx, command.as_deref()).await
}

async fn display_help(ctx: Context<'_>, command: Option<&str>) -> CommandResult {
    let config = HelpConfiguration::default();
    builtins::help(ctx, command, config).await?;
    Ok(())
}

async fn configure_layer<F, T>(ctx: Context<'_>, guild_id: GuildId, layer: &str,
    f: F) -> Option<T>
where
    F: FnOnce(RwLockWriteGuard<Mixer>) -> T
{
    let guild_state = get_guild_state_mut(ctx.data(), guild_id).await;
    let mixer = guild_state.mixer_mut();

    if mixer.contains_layer(layer) {
        Some(f(mixer))
    }
    else {
        None
    }
}

async fn list_layer_key_value_descriptors<F>(ctx: Context<'_>,
    layer: String, name_plural_capital: &str, get: F) -> CommandResult
where
    F: FnOnce(&Layer) -> &[KeyValueDescriptor]
{
    let guild_id = ctx.guild_id().unwrap();
    let descriptors = get_guild_state(ctx.data(), guild_id).await
        .and_then(|gs| {
            let mixer = gs.mixer_blocking();
    
            if mixer.contains_layer(&layer) {
                Some(get(mixer.layer(&layer)).iter()
                    .map(|e| format!("{}", e))
                    .collect::<Vec<_>>())
            }
            else {
                None
            }
        });

    if let Some(descriptors) = descriptors {
        let mut reply =
            format!("{} on layer `{}`:", name_plural_capital, &layer);

        for (i, descriptor) in descriptors.iter().enumerate() {
            write!(reply, "\n{}. {}", i + 1, descriptor).unwrap();
        }

        ctx.reply(reply).await?;
    }
    else {
        ctx.reply("Layer not found.").await?;
    }

    Ok(())
}

async fn help_modifiers<D, N, R>(ctx: Context<'_>, modifier: Option<String>,
    name_plural_upper: &str, name_singular_lower: &str, mut get_documentation: D, get_names: N)
    -> CommandResult
where
    D: FnMut(&PluginManager, &str) -> Option<ModifierDocumentation>,
    N: FnOnce(&PluginManager) -> Keys<'_, String, R>
{
    let data_guard = ctx.data().read().await;
    let plugin_manager =
        Arc::clone(data_guard.get::<PluginManager>().unwrap());

    if let Some(name) = modifier {
        if let Some(documentation) =
                get_documentation(plugin_manager.as_ref(), &name) {
            ctx.reply(format!("**{}**\n\n{}", name, documentation)).await?;
        }
        else {
            ctx.reply(format!("No {} of name {}.", name_singular_lower, name)).await?;
        }
    }
    else {
        let mut response = format!("{} provided by plugins:", name_plural_upper);

        for name in get_names(plugin_manager.as_ref()) {
            let doc =
                get_documentation(plugin_manager.as_ref(), name.as_str())
                .unwrap();

            write!(&mut response, "\n- **{}**: {}", name, doc.short_summary())
                .unwrap();
        }

        ctx.reply(response).await?;
    }

    Ok(())
}

/// Indicates that the message which caused a command to be executed is not a real message but
/// synthetically created as part of some bot-internal command dispatch (sound board or do-command).
/// This will be supplied as `invocation_data`.
struct SyntheticMessageMarker;

async fn dispatch_command_as_message(
        framework_ctx: FrameworkContext<'_, TokioRwLock<CommandData>, CommandError>,
        serenity_ctx: &SerenityContext,
        msg: &SerenityMessage) -> CommandResult {
    let trigger = MessageDispatchTrigger::MessageCreate;
    let invocation_data: Mutex<Box<dyn Any + Send + Sync>> =
        Mutex::new(Box::new(SyntheticMessageMarker));
    poise::dispatch_message(
        framework_ctx,
        serenity_ctx,
        msg,
        trigger,
        &invocation_data,
        &mut vec![]).await.map_err(|err| format!("{}", err).into())
}

async fn confirm(ctx: Context<'_>) -> CommandResult {
    match ctx {
        Context::Application(ctx) => {
            let response = CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new().ephemeral(true).content("\u{1f44c}")
            );

            ctx.interaction.create_response(ctx, response).await?;
        },
        Context::Prefix(ctx) => {
            let invocation_data = ctx.invocation_data.lock().await;

            if !invocation_data.is::<SyntheticMessageMarker>() {
                ctx.msg.react(ctx, '\u{1f44c}').await?;
            }
        }
    }

    Ok(())
}

enum CommandResponse<R: Into<String> = String> {
    Confirm,
    Reply(R)
}

impl<V, E: Into<String>> From<Result<V, E>> for CommandResponse<E> {
    fn from(result: Result<V, E>) -> CommandResponse<E> {
        match result {
            Ok(_) => CommandResponse::Confirm,
            Err(e) => CommandResponse::Reply(e)
        }
    }
}

impl<R: Into<String>> From<R> for CommandResponse<R> {
    fn from(value: R) -> Self {
        CommandResponse::Reply(value)
    }
}

impl<'s> From<&'s str> for CommandResponse<String> {
    fn from(value: &'s str) -> Self {
        CommandResponse::Reply(value.to_owned())
    }
}

async fn respond<R: Into<String>>(ctx: Context<'_>, response: CommandResponse<R>) -> CommandResult {
    match response {
        CommandResponse::Confirm => {
            confirm(ctx).await?;
        },
        CommandResponse::Reply(reply) => {
            ctx.reply(reply).await?;
        }
    }

    Ok(())
}
