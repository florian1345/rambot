use crate::audio::Mixer;
use crate::command::board::{BoardManager, Board};
use crate::key_value::KeyValueDescriptor;
use crate::plugin::PluginManager;

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
use std::sync::{Arc, Mutex};

/// The bot's state for one specific guild.
pub struct GuildState {
    mixer: Arc<Mutex<Mixer>>,
    board_manager: BoardManager
}

impl GuildState {
    fn new(plugin_manager: Arc<PluginManager>) -> GuildState {
        log::info!("New guild state created.");

        GuildState {
            mixer: Arc::new(Mutex::new(Mixer::new(plugin_manager))),
            board_manager: BoardManager::new()
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
            mixer: Arc::new(Mutex::new(mixer)),
            board_manager
        }
    }

    /// Gets an [Arc] to a [Mutex]ed audio [Mixer] for audio playback in this
    /// guild. This also manages the layers.
    pub fn mixer(&self) -> Arc<Mutex<Mixer>> {
        Arc::clone(&self.mixer)
    }

    pub fn board_manager(&self) -> &BoardManager {
        &self.board_manager
    }

    pub fn board_manager_mut(&mut self) -> &mut BoardManager {
        &mut self.board_manager
    }

    fn serde(&self) -> SerdeGuildState {
        let mut layers = Vec::new();

        for layer in self.mixer.lock().unwrap().layers() {
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
            boards: self.board_manager.boards().cloned().collect()
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
    boards: Vec<Board>
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
    path: PathBuf
}

impl<'a> Deref for GuildStateGuard<'a> {
    type Target = GuildState;

    fn deref(&self) -> &GuildState {
        &self.guild_state
    }
}

impl<'a> DerefMut for GuildStateGuard<'a> {
    fn deref_mut(&mut self) -> &mut GuildState {
        &mut self.guild_state
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
    }
}

/// The global state of the bot.
pub struct State {
    guild_states: HashMap<GuildId, GuildState>,
    directory: String
}

fn is_json(p: &PathBuf) -> bool {
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
            let matches = fs::read_dir(&path)?
                .flat_map(|e| e.into_iter())
                .map(|e| e.path())
                .filter(is_json);
            let mut state = State::new(directory)?;

            for json_path in matches {
                let guild_id_str_opt =
                    json_path.file_stem().and_then(|o| o.to_str());

                if let Some(guild_id_str) = guild_id_str_opt {
                    let guild_id = GuildId::from(guild_id_str.parse::<u64>()?);
                    let topology =
                        serde_json::from_reader(File::open(json_path)?)?;
                    let guild_state =
                        GuildState::from_serde(Arc::clone(&plugin_manager),
                            topology);
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
    /// to be saved.
    pub fn guild_state(&mut self, id: GuildId,
            plugin_manager: Arc<PluginManager>) -> &GuildState {
        self.guild_states.entry(id)
            .or_insert_with(|| GuildState::new(plugin_manager))
    }

    /// Gets a [GuildStateGuard] to the [GuildState] with the given ID. This is
    /// intended to be used whenever any potential state changes need to be
    /// saved.
    pub fn guild_state_mut(&mut self, id: GuildId,
            plugin_manager: Arc<PluginManager>) -> GuildStateGuard<'_> {
        let path = Path::new(&self.directory);
        let file_path = path.join(format!("{}.json", id.as_u64()));
        let guild_state = self.guild_states.entry(id)
            .or_insert_with(|| GuildState::new(plugin_manager));

        GuildStateGuard {
            path: file_path,
            guild_state
        }
    }

    /// Gets the number of guilds for which a state is registered.
    pub fn guild_count(&self) -> usize {
        self.guild_states.len()
    }
}

impl TypeMapKey for State {
    type Value = State;
}
