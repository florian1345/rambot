use crate::audio::Mixer;
use crate::command::board::{BoardManager, Board};
use crate::key_value::KeyValueDescriptor;
use crate::plugin::PluginManager;

use rambot_api::PluginGuildConfig;

use serde::{Deserialize, Serialize, Serializer};

use serenity::model::id::GuildId;
use serenity::prelude::TypeMapKey;

use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::num::ParseIntError;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// The bot's state for one specific guild.
pub struct GuildState {
    mixer: Arc<RwLock<Mixer>>,
    board_manager: BoardManager,
    root_directory: Option<String>
}

impl GuildState {
    fn new(plugin_manager: Arc<PluginManager>) -> GuildState {
        log::info!("New guild state created.");

        GuildState {
            mixer: Arc::new(RwLock::new(Mixer::new(plugin_manager))),
            board_manager: BoardManager::new(),
            root_directory: None
        }
    }

    fn from_serde(plugin_manager: Arc<PluginManager>,
            serde: SerdeGuildState) -> GuildState {
        let mut mixer = Mixer::new(plugin_manager);
        
        for layer in serde.mixer.layers {
            mixer.add_layer(&layer.name);

            for effect in layer.effects {
                mixer.add_effect(&layer.name, effect).unwrap();
            }

            for adapter in layer.adapters {
                mixer.add_adapter(&layer.name, adapter);
            }
        }

        let mut board_manager = BoardManager::new();

        for board in serde.boards {
            board_manager.add_board(board);
        }
        
        GuildState {
            mixer: Arc::new(RwLock::new(mixer)),
            board_manager,
            root_directory: serde.directory
        }
    }

    /// Read-locks the [Mixer] for this guild and returns an appropriate guard.
    pub fn mixer_blocking(&self) -> RwLockReadGuard<Mixer> {
        self.mixer.read().unwrap()
    }

    /// Write-locks the [Mixer] for this guild and returns an appropriate
    /// guard. Note that while this method only requires an immutable
    /// reference, modifying any configuration of the mixer should only be done
    /// while the guild state is behind a [GuildStateGuard]. This ensures that
    /// any changes in the configuration are propagated to the associated file
    /// on the hard drive.
    pub fn mixer_mut(&self) -> RwLockWriteGuard<Mixer> {
        self.mixer.write().unwrap()
    }

    pub fn mixer_arc(&self) -> Arc<RwLock<Mixer>> {
        Arc::clone(&self.mixer)
    }

    /// Gets a reference to the [BoardManager] for the sound boards in this
    /// guild.
    pub fn board_manager(&self) -> &BoardManager {
        &self.board_manager
    }

    /// Gets a mutable reference to the [BoardManager] for the sound boards in
    /// this guild.
    pub fn board_manager_mut(&mut self) -> &mut BoardManager {
        &mut self.board_manager
    }

    /// Constructs a [PluginGuildConfig] from the information stored in this
    /// guild state.
    pub fn build_plugin_guild_config(&self) -> PluginGuildConfig {
        PluginGuildConfig::new(self.root_directory.as_ref())
    }

    /// Sets a guild-specific root directory.
    pub fn set_root_directory(&mut self, directory: impl Into<String>) {
        self.root_directory = Some(directory.into());
    }

    /// Unsets the guild-specific root directory, if set, indicating that the
    /// global root directory shall be used from now on.
    pub fn unset_root_directory(&mut self) {
        self.root_directory = None;
    }

    fn serde(&self) -> SerdeGuildState {
        let mut layers = Vec::new();

        for layer in self.mixer.read().unwrap().layers() {
            layers.push(SerdeLayer {
                name: layer.name().to_owned(),
                effects: layer.effects().to_vec(),
                adapters: layer.adapters().to_vec()
            });
        }

        SerdeGuildState {
            mixer: SerdeMixer {
                layers
            },
            boards: self.board_manager.boards().cloned().collect(),
            directory: self.root_directory.clone()
        }
    }
}

impl Serialize for GuildState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer
    {
        self.serde().serialize(serializer)
    }
}

#[derive(Deserialize, Serialize)]
struct SerdeLayer {
    name: String,
    effects: Vec<KeyValueDescriptor>,
    adapters: Vec<KeyValueDescriptor>
}

#[derive(Deserialize, Serialize)]
struct SerdeMixer {
    layers: Vec<SerdeLayer>
}

#[derive(Deserialize, Serialize)]
struct SerdeGuildState {
    mixer: SerdeMixer,
    boards: Vec<Board>,

    #[serde(skip_serializing_if = "Option::is_none")]
    directory: Option<String>
}

/// An enumeration of the errors that may occur while loading or saving the
/// state.
pub enum StateError {

    /// Indicates that the state directory is occupied by a file of the same
    /// name.
    OccupiedByFile,

    /// Indicates that the state file for some guild had a name which could not
    /// be parsed to a guild ID.
    InvalidId(ParseIntError),

    /// Indicates that something went wrong while reading or writing files or
    /// directories.
    IoError(io::Error),

    /// Indicates that something went wrong while deerializing or serializing
    /// the JSON files.
    JsonError(serde_json::Error)
}

impl From<ParseIntError> for StateError {
    fn from(e: ParseIntError) -> StateError {
        StateError::InvalidId(e)
    }
}

impl From<io::Error> for StateError {
    fn from(e: io::Error) -> StateError {
        StateError::IoError(e)
    }
}

impl From<serde_json::Error> for StateError {
    fn from(e: serde_json::Error) -> StateError {
        StateError::JsonError(e)
    }
}

impl Display for StateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            StateError::OccupiedByFile =>
                write!(f,
                    "The state directory is occupied by a file of the same \
                    name."),
            StateError::InvalidId(e) =>
                write!(f,
                    "Could not parse the name of a state file as a guild ID: \
                    {}", e),
            StateError::IoError(e) =>
                write!(f, "Error while loading or saving state files: {}", e),
            StateError::JsonError(e) =>
                write!(f,
                    "Error while deserializing or serializing state files: {}",
                    e)
        }
    }
}

/// A guard for mutable access to a [GuildState], which saves it to its
/// corresponding file after modification has finished.
pub struct GuildStateGuard<'a> {
    guild_state: &'a mut GuildState,
    path: PathBuf,
    id: GuildId
}

impl<'a> Deref for GuildStateGuard<'a> {
    type Target = GuildState;

    fn deref(&self) -> &GuildState {
        self.guild_state
    }
}

impl<'a> DerefMut for GuildStateGuard<'a> {
    fn deref_mut(&mut self) -> &mut GuildState {
        self.guild_state
    }
}

impl<'a> Drop for GuildStateGuard<'a> {
    fn drop(&mut self) {
        let file_res = if self.path.exists() {
            OpenOptions::new().write(true).truncate(true).open(&self.path)
        }
        else {
            File::create(&self.path)
        };

        let file = match file_res {
            Ok(f) => f,
            Err(e) => {
                log::warn!("Could not save changed state: {}", e);
                return;
            }
        };

        if let Err(e) = serde_json::to_writer(file, &self.guild_state) {
            log::warn!("Could not save changed state: {}", e);
        }

        log::debug!("Saved state for guild {}.", self.id);
    }
}

/// The global state of the bot.
pub struct State {
    guild_states: HashMap<GuildId, GuildState>,
    directory: String
}

fn is_json(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }

    let extension = p.extension().and_then(|o| o.to_str());
    
    if let Some(extension) = extension {
        extension.to_lowercase() == "json"
    }
    else {
        false
    }
}

impl State {
    fn new(directory: &str) -> Result<State, StateError> {
        let path = Path::new(&directory);

        if !path.exists() {
            fs::create_dir(path)?;
        }

        Ok(State {
            guild_states: HashMap::new(),
            directory: directory.to_owned()
        })
    }

    /// Loads the state from the given directory. If the directory does not
    /// exist, it will be created and the returned state will be empty. Once
    /// the state is dropped, the (potentially modified) state will be stored
    /// in the same directory.
    pub fn load(directory: &str, plugin_manager: Arc<PluginManager>)
            -> Result<State, StateError> {
        let path = Path::new(directory);

        if path.is_file() {
            Err(StateError::OccupiedByFile)
        }
        else if path.exists() {
            let matches = fs::read_dir(path)?
                .flat_map(|e| e.into_iter())
                .map(|e| e.path())
                .filter(|p| is_json(p));
            let mut state = State::new(directory)?;

            for json_path in matches {
                let guild_id_str_opt =
                    json_path.file_stem().and_then(|o| o.to_str());

                if let Some(guild_id_str) = guild_id_str_opt {
                    let guild_id = GuildId::from(guild_id_str.parse::<u64>()?);
                    let topology =
                        serde_json::from_reader(File::open(json_path)?)?;
                    let guild_state =
                        GuildState::from_serde(Arc::clone(&plugin_manager), topology);
                    state.guild_states.insert(guild_id, guild_state);
                }
            }

            Ok(state)
        }
        else {
            State::new(directory)
        }
    }

    /// Gets an immutable reference to the [GuildState] with the given ID. This
    /// is intended to be used whenever any potential state changes do not need
    /// to be saved. If no guild state for the given ID exists yet, `None` is
    /// returned.
    pub fn guild_state(&self, id: GuildId) -> Option<&GuildState> {
        self.guild_states.get(&id)
    }

    /// Gets a [GuildStateGuard] to the [GuildState] with the given ID. Any
    /// changes in the configuration will be comitted to the hard drive once
    /// the guard goes out of scope. If no state for the given guild ID exists,
    /// a new one is created.
    pub fn guild_state_mut(&mut self, id: GuildId,
            plugin_manager: &Arc<PluginManager>) -> GuildStateGuard<'_> {
        let (guild_state, path) =
            self.ensure_guild_state_exists_do(id, plugin_manager);

        GuildStateGuard {
            path,
            guild_state,
            id
        }
    }

    /// Gets a raw mutable reference to the [GuildState] with the given ID.
    /// Only use this if you do not intend any changes to be stored on the hard
    /// drive! If you alter the configuration in any way, use
    /// [State::guild_state_mut] instead.
    pub fn guild_state_mut_unguarded(&mut self, id: GuildId)
            -> Option<&mut GuildState> {
        self.guild_states.get_mut(&id)
    }

    fn ensure_guild_state_exists_do(&mut self, id: GuildId,
            plugin_manager: &Arc<PluginManager>) -> (&mut GuildState, PathBuf) {
        let path = Path::new(&self.directory);
        let file_path = path.join(format!("{}.json", id));
        let guild_state = self.guild_states.entry(id)
            .or_insert_with(|| GuildState::new(Arc::clone(plugin_manager)));

        (guild_state, file_path)
    }

    /// Ensures that a guild state for the guild with the given ID exists. That
    /// is, a new one is created if none exists yet.
    pub fn ensure_guild_state_exists(&mut self, id: GuildId,
            plugin_manager: &Arc<PluginManager>) {
        self.ensure_guild_state_exists_do(id, plugin_manager);
    }

    /// Gets the number of guilds for which a state is registered.
    pub fn guild_count(&self) -> usize {
        self.guild_states.len()
    }
}

impl TypeMapKey for State {
    type Value = State;
}
