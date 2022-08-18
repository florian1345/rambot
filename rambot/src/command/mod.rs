use crate::FrameworkTypeMapKey;
use crate::audio::{PCMRead, Layer, Mixer};
use crate::key_value::KeyValueDescriptor;
use crate::plugin::PluginManager;
use crate::state::{State, GuildState};

use rambot_api::{
    AudioSource,
    ModifierDocumentation,
    PluginGuildConfig,
    SampleDuration
};

use rambot_proc_macro::rambot_command;

use serenity::client::Context;
use serenity::framework::standard::{CommandGroup, CommandResult};
use serenity::framework::standard::macros::{command, group};
use serenity::model::channel::MessageType;
use serenity::model::id::GuildId;
use serenity::model::prelude::Message;

use serenity::prelude::TypeMap;
use songbird::Call;
use songbird::error::JoinError;
use songbird::input::{Input, Reader};

use std::collections::hash_map::Keys;
use std::fmt::{Display, Write};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, RwLock, RwLockWriteGuard};

use tokio::runtime::{Handle, Runtime};
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::MutexGuard as TokioMutexGuard;
use tokio::sync::RwLockReadGuard as TokioRwLockReadGuard;
use tokio::sync::RwLockWriteGuard as TokioRwLockWriteGuard;

pub mod adapter;
pub mod board;
pub mod effect;
pub mod layer;

pub use adapter::get_adapter_commands;
pub use board::get_board_commands;
pub use effect::get_effect_commands;
pub use layer::get_layer_commands;

macro_rules! unwrap_or_return {
    ($e:expr, $r:expr) => {
        match $e {
            Some(v) => v,
            None => return $r
        }
    }
}

pub(crate) use unwrap_or_return;

#[group]
#[commands(
    audio,
    connect,
    directory,
    disconnect,
    cmd_do,
    info,
    play,
    seek,
    skip,
    stop
)]
struct Root;

/// Gets a [CommandGroup] for the root commands (which are not part of any
/// sub-group).
pub fn get_root_commands() -> &'static CommandGroup {
    &ROOT_GROUP
}

async fn connect_do<'a>(ctx: &Context, msg: &Message,
        call: TokioMutexGuard<'a, Call>)
        -> CommandResult<Option<String>> {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;
    let channel_id_opt = guild.voice_states
        .get(&msg.author.id)
        .and_then(|v| v.channel_id);
    let channel_id = unwrap_or_return!(channel_id_opt,
        Ok(Some("I cannot see your voice channel. Are you connected?"
            .to_owned())));

    if let Some(channel) = call.current_channel() {
        if channel.0 == channel_id.0 {
            return Ok(Some(
                "I am already connected to your voice channel."
                .to_owned()));
        }
    }

    drop(call);

    log::debug!("Joining channel {} on guild {}.", channel_id, guild_id);

    let songbird = songbird::get(ctx).await.unwrap();
    songbird.join(guild_id, channel_id).await.1.unwrap();

    Ok(None)
}

#[rambot_command(
    description = "Connects the bot to the voice channel to which the sender \
        of the command is currently connected.",
    usage = ""
)]
async fn connect(ctx: &Context, msg: &Message)
        -> CommandResult<Option<String>> {
    let call = get_songbird_call(ctx, msg).await;
    connect_do(ctx, msg, call.lock().await).await
}

async fn get_songbird_call(ctx: &Context, msg: &Message)
        -> Arc<TokioMutex<Call>> {
    let guild_id = msg.guild_id.unwrap();
    songbird::get(ctx).await.unwrap().get_or_insert(guild_id)
}

const NOT_CONNECTED: &str = "I am not connected to a voice channel";

#[rambot_command(
    description = "Disconnects the bot from the voice channel to which it is \
        currently connected.",
    usage = ""
)]
async fn disconnect(ctx: &Context, msg: &Message)
        -> CommandResult<Option<String>> {
    if let Some(songbird) = songbird::get(ctx).await {
        let guild_id = msg.guild_id.unwrap();

        match songbird.remove(guild_id).await {
            Ok(_) => {
                stop_all_do(ctx, msg).await; // drop audio sources
                log::debug!("Left voice on guild {}.", guild_id);
                Ok(None)
            },
            Err(JoinError::NoCall) => Ok(Some(NOT_CONNECTED.to_owned())),
            Err(e) => Err(e.into())
        }
    }
    else {
        log::error!("No songbird instance found.");
        Ok(Some("Internal error: No songbird instance found.".to_owned()))
    }
}

fn to_input<S>(source: Arc<RwLock<S>>) -> Input
where
    S: AudioSource + Send + Sync + 'static
{
    let read = PCMRead::new(source);
    Input::float_pcm(true, Reader::Extension(Box::new(read)))
}

struct GuildStateRef<'a> {
    data_guard: TokioRwLockReadGuard<'a, TypeMap>,
    guild_id: GuildId
}

impl<'a> Deref for GuildStateRef<'a> {
    type Target = GuildState;

    fn deref(&self) -> &GuildState {
        let state = self.data_guard.get::<State>().unwrap();
        state.guild_state(self.guild_id).unwrap()
    }
}

struct GuildStateMutUnguarded<'a> {
    data_guard: TokioRwLockWriteGuard<'a, TypeMap>,
    guild_id: GuildId
}

impl<'a> Deref for GuildStateMutUnguarded<'a> {
    type Target = GuildState;

    fn deref(&self) -> &GuildState {
        let state = self.data_guard.get::<State>().unwrap();
        state.guild_state(self.guild_id).unwrap()
    }
}

impl<'a> DerefMut for GuildStateMutUnguarded<'a> {
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

impl<'a> Deref for GuildStateMut<'a> {
    type Target = GuildState;

    fn deref(&self) -> &GuildState {
        let state = self.data_guard.get::<State>().unwrap();
        state.guild_state(self.guild_id).unwrap()
    }
}

impl<'a> DerefMut for GuildStateMut<'a> {
    fn deref_mut(&mut self) -> &mut GuildState {
        // Sadly, we cannot use a GuildStateGuard here for two reasons.
        // 1. We would need to deref it to obtain an actual &mut refernce,
        //    which would mean we return a reference to a temporary variable.
        // 2. Multiple mutable accesses would result in saving the state file
        //    each time, which is inefficient.
        // To resolve this, we create a dummy guard whenever GuildStateMut is
        // dropped. Dropping this dummy guard ensures any changes are committed
        // to the state file.

        let state_mut = self.data_guard.get_mut::<State>().unwrap();
        state_mut.ensure_guild_state_exists(
            self.guild_id, &self.plugin_manager);
        state_mut.guild_state_mut_unguarded(self.guild_id).unwrap()
    }
}

impl<'a> Drop for GuildStateMut<'a> {
    fn drop(&mut self) {
        let state_mut = self.data_guard.get_mut::<State>().unwrap();
        let g = state_mut.guild_state_mut(self.guild_id, &self.plugin_manager);

        drop(g);
    }
}

async fn get_guild_state(ctx: &Context, guild_id: GuildId)
        -> Option<GuildStateRef<'_>> {
    let data_guard = ctx.data.read().await;

    if data_guard.get::<State>().unwrap().guild_state(guild_id).is_some() {
        Some(GuildStateRef {
            data_guard,
            guild_id
        })
    }
    else {
        None
    }
}

async fn get_guild_state_mut_unguarded(ctx: &Context, guild_id: GuildId) -> Option<GuildStateMutUnguarded<'_>> {
    let data_guard = ctx.data.write().await;

    if data_guard.get::<State>().unwrap().guild_state(guild_id).is_some() {
        Some(GuildStateMutUnguarded {
            data_guard,
            guild_id
        })
    }
    else {
        None
    }
}

async fn get_guild_state_mut(ctx: &Context, guild_id: GuildId) -> GuildStateMut<'_> {
    let data_guard = ctx.data.write().await;
    let plugin_manager =
        Arc::clone(data_guard.get::<PluginManager>().unwrap());

    GuildStateMut {
        data_guard,
        guild_id,
        plugin_manager
    }
}

fn play_mixer(ctx: &Context, msg: &Message, mixer: Arc<RwLock<Mixer>>,
        layer: &str, audio: &str, plugin_guild_config: PluginGuildConfig)
        -> (bool, Option<String>) {
    let mut mixer_guard = mixer.write().unwrap();

    if !mixer_guard.contains_layer(layer) {
        return (false, Some(format!("No layer of name {}.", &layer)));
    }

    let active_before = mixer_guard.active();
    let ctx_clone = ctx.clone();
    let msg_clone = msg.clone();
    let error_callback = move |layer, e| {
        // TODO this is just asking for trouble.

        let content = format!("Error on layer {}: {}", layer, e);
        let future = msg_clone.reply(&ctx_clone, content);

        if let Ok(handle) = Handle::try_current() {
            handle.block_on(future).unwrap();
        }
        else {
            let runtime = Runtime::new().unwrap();
            runtime.block_on(future).unwrap();
        }
    };
    let play_res = mixer_guard.play_on_layer(
        layer, audio, plugin_guild_config, error_callback);

    if let Err(e) = play_res {
        (active_before, Some(format!("{}", e)))
    }
    else {
        (active_before, None)
    }
}

#[rambot_command(
    description = "Plays the given audio on the given layer. Possible formats \
        for the input depend on the installed plugins.",
    usage = "layer audio",
    rest
)]
async fn play(ctx: &Context, msg: &Message, layer: String, audio: String)
        -> CommandResult<Option<String>> {
    let guild_id = msg.guild_id.unwrap();
    let guild_state = unwrap_or_return!(
        get_guild_state(ctx, guild_id).await,
        Ok(Some(format!("No layer of name {}.", &layer))));
    let plugin_guild_config = guild_state.build_plugin_guild_config();
    let mixer = guild_state.mixer_arc();
    let call = get_songbird_call(ctx, msg).await;
    let (active_before, err_msg) = play_mixer(
        ctx, msg, Arc::clone(&mixer), &layer, &audio, plugin_guild_config);

    if let Some(err_msg) = err_msg {
        return Ok(Some(err_msg));
    }

    let mut call_guard = call.lock().await;

    if !active_before {
        call_guard.play_only_source(to_input(Arc::clone(&mixer)));
    }

    if call_guard.current_channel().is_none() {
        if let Some(err_msg) = connect_do(ctx, msg, call_guard).await? {
            mixer.write().unwrap().stop_all();
            return Ok(Some(err_msg));
        }
    }

    Ok(None)
}

async fn with_layer_mut<F, E>(ctx: &Context, msg: &Message, layer: &str, f: F)
    -> CommandResult<Option<String>>
where
    F: FnOnce(RwLockWriteGuard<Mixer>, &str) -> Result<(), E>,
    E: Display
{
    let guild_id = msg.guild_id.unwrap();
    let guild_state = unwrap_or_return!(get_guild_state(ctx, guild_id).await,
        Ok(Some(format!("Found no layer with name {}.", layer))));
    let mixer = guild_state.mixer_mut();

    if mixer.contains_layer(layer) {
        match f(mixer, layer) {
            Ok(()) => Ok(None),
            Err(e) => Ok(Some(format!("{}", e)))
        }
    }
    else {
        Ok(Some(format!("Found no layer with name {}.", layer)))
    }
}

#[rambot_command(
    description = "Plays the next piece of the list currently played on the \
        given layer. If the last piece of the list is active, this stops \
        audio on the layer.",
    usage = "layer"
)]
async fn skip(ctx: &Context, msg: &Message, layer: String)
        -> CommandResult<Option<String>> {
    with_layer_mut(ctx, msg, &layer,
        |mut mixer, layer| mixer.skip_on_layer(layer)).await
}

async fn stop_do(ctx: &Context, msg: &Message, layer: &str) -> Option<String> {
    let guild_id = msg.guild_id.unwrap();
    let guild_state = unwrap_or_return!(
        get_guild_state(ctx, guild_id).await,
        Some(format!("No layer of name {}.", &layer)));
    let mut mixer = guild_state.mixer_mut();

    if !mixer.contains_layer(layer) {
        Some(format!("No layer of name {}.", layer))
    }
    else if !mixer.stop_layer(layer) {
        Some("No audio to stop.".to_owned())
    }
    else {
        None
    }
}

async fn stop_all_do(ctx: &Context, msg: &Message) -> Option<String> {
    let guild_id = msg.guild_id.unwrap();
    let guild_state = unwrap_or_return!(
        get_guild_state(ctx, guild_id).await,
        Some(format!("No audio to stop.")));

    if guild_state.mixer_mut().stop_all() {
        None
    }
    else {
        Some("No audio to stop.".to_owned())
    }
}

#[rambot_command(
    description = "Stops the audio currently playing on the given layer. If \
        no layer is given, all audio is stopped.",
    usage = "[layer]"
)]
async fn stop(ctx: &Context, msg: &Message, layer: Option<String>)
        -> CommandResult<Option<String>> {
    let reply = if let Some(layer) = layer {
        stop_do(ctx, msg, &layer).await
    }
    else {
        stop_all_do(ctx, msg).await
    };

    Ok(reply)
}

#[rambot_command(
    description = "Moves the current position in the audio of the layer with \
        the given by the given amount of time. The `delta` is of the format \
        `AhBmCsDmsEsam`, representing `A` hours, `B` minutes, `C` seconds, \
        `D` milliseconds, and `E` samples (at 48 kHz). Omitting and \
        reordering these terms is permitted. Negative deltas are used to seek \
        backwards in time.",
    usage = "layer delta"
)]
async fn seek(ctx: &Context, msg: &Message, layer: String, delta: SampleDuration) -> CommandResult<Option<String>> {
    with_layer_mut(ctx, msg, &layer,
        |mut mixer, layer| mixer.seek_on_layer(layer, delta)).await
}

#[rambot_command(
    name = "do",
    description = "Takes as input a list of quoted strings separated by spaces. \
        These are then executed as commands in order.",
    usage = "[command] [command] ..."
)]
async fn cmd_do(ctx: &Context, msg: &Message, commands: Vec<String>)
        -> CommandResult<Option<String>> {
    let framework = Arc::clone(
        ctx.data.read().await.get::<FrameworkTypeMapKey>().unwrap());

    for command in commands {
        let mut msg = msg.clone();
        msg.content = command.to_owned();
        msg.kind = MessageType::Unknown; // Prevents :ok_hand:
        framework.dispatch(ctx.clone(), msg).await;
    }

    Ok(None)
}

#[rambot_command(
    description = "Lists all plugin-provided types of audio with a short \
        summary. If an audio name is provided, a more detailed documentation \
        page for that audio is displayed.",
    usage = "[audio]",
    rest
)]
async fn audio(ctx: &Context, msg: &Message, audio: String)
        -> CommandResult<Option<String>> {
    let data_guard = ctx.data.read().await;
    let plugin_manager = data_guard.get::<PluginManager>().unwrap();

    if audio.is_empty() {
        let mut message = "Audio types:".to_owned();
        let mut first = true;

        for doc in plugin_manager.get_audio_documentations() {
            if first {
                writeln!(message).unwrap();
                first = false;
            }

            write!(message, "\n- {}", doc.overview_entry()).unwrap();
        }

        msg.reply(ctx, message).await?;
        Ok(None)
    }
    else {
        let audio_lower = audio.to_lowercase();
        let doc = plugin_manager.get_audio_documentations()
            .find(|d| d.name().to_lowercase() == audio_lower);

        if let Some(doc) = doc {
            msg.reply(ctx, doc).await?;
            Ok(None)
        }
        else {
            Ok(Some(format!("I found no audio of name {}.", audio)))
        }
    }
}

#[rambot_command(
    description = "Prints information about the audio currently played on the \
        layer with the given name.",
    usage = "layer"
)]
async fn info(ctx: &Context, msg: &Message, layer: String) -> CommandResult<Option<String>> {
    let guild_id = msg.guild_id.unwrap();
    let guild_state = unwrap_or_return!(
        get_guild_state(ctx, guild_id).await,
        Ok(Some(format!("No layer of name `{}`.", layer))));
    let metadata = guild_state.mixer().layer_metadata(&layer);

    match metadata {
        Ok(metadata) => {
            let mut message = String::new();

            if let Some(title) = metadata.title() {
                writeln!(message, "Title: {}", title).unwrap();
            }

            if let Some(artist) = metadata.artist() {
                writeln!(message, "Artist: {}", artist).unwrap();
            }

            if let Some(album) = metadata.album() {
                writeln!(message, "Album: {}", album).unwrap();
            }

            if let Some(year) = metadata.year() {
                writeln!(message, "Year: {}", year).unwrap();
            }

            let mut message = message.trim_end().to_owned();

            if message.is_empty() {
                message = "No information available.".to_owned();
            }

            msg.reply(ctx, message).await?;
            Ok(None)
        },
        Err(e) => Ok(Some(format!("{}", e)))
    }
}

#[rambot_command(
    description = "Specify a guild-specific root directory for file system \
        based plugins. Omit directory argument to reset to the default root \
        directory specified in the config. Any pieces in playlists that are \
        currently active will continue to be resolved according to the old \
        root directrory.",
    usage = "[directory]",
    rest,
    confirm,
    owners_only
)]
async fn directory(ctx: &Context, msg: &Message, directory: String)
        -> CommandResult<Option<String>> {
    let guild_id = msg.guild_id.unwrap();
    let mut guild_state = get_guild_state_mut(ctx, guild_id).await;

    if directory.is_empty() {
        guild_state.unset_root_directory()
    }
    else {
        guild_state.set_root_directory(directory)
    }

    Ok(None)
}

async fn configure_layer<F, T>(ctx: &Context, guild_id: GuildId, layer: &str,
    f: F) -> Option<T>
where
    F: FnOnce(RwLockWriteGuard<Mixer>) -> T
{
    let guild_state = get_guild_state_mut(ctx, guild_id).await;
    let mixer = guild_state.mixer_mut();

    if mixer.contains_layer(layer) {
        Some(f(mixer))
    }
    else {
        None
    }
}

async fn list_layer_key_value_descriptors<F>(ctx: &Context, msg: &Message,
    layer: String, name_plural_capital: &str, get: F)
    -> CommandResult<Option<String>>
where
    F: FnOnce(&Layer) -> &[KeyValueDescriptor]
{
    let guild_id = msg.guild_id.unwrap();
    let descriptors = get_guild_state(ctx, guild_id).await
        .and_then(|gs| {
            let mixer = gs.mixer();
    
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

        msg.reply(ctx, reply).await?;
        Ok(None)
    }
    else {
        Ok(Some("Layer not found.".to_owned()))
    }
}

async fn help_modifiers<D, N, R>(ctx: &Context, msg: &Message,
    modifier: Option<String>, name_plural_upper: &str,
    name_singular_lower: &str, mut get_documentation: D, get_names: N)
    -> CommandResult<Option<String>>
where
    D: FnMut(&PluginManager, &str) -> Option<ModifierDocumentation>,
    N: FnOnce(&PluginManager) -> Keys<'_, String, R>
{
    let data_guard = ctx.data.read().await;
    let plugin_manager =
        Arc::clone(data_guard.get::<PluginManager>().unwrap());

    if let Some(name) = modifier {
        if let Some(documentation) =
                get_documentation(plugin_manager.as_ref(), &name) {
            msg.reply(ctx, format!("**{}**\n\n{}", name, documentation))
                .await?;
            Ok(None)
        }
        else {
            Ok(Some(format!("No {} of name {}.", name_singular_lower, name)))
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

        msg.reply(ctx, response).await?;
        Ok(None)
    }
}
