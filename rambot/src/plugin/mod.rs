use crate::plugin::source::{PluginSourceError, PluginAudioSource};

use rambot_api::communication::{
    BotMessageData,
    Channel,
    PluginMessageData
};

use serenity::prelude::TypeMapKey;

use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::process::Child;
use std::str::FromStr;

pub mod load;
pub mod source;

/// An enumeration of all errors that may occur when setting up a plugin.
pub enum PluginError {

    /// Indicates that something went wrong regarding the data stream.
    IOError(io::Error)
}

impl From<io::Error> for PluginError {
    fn from(e: io::Error) -> PluginError {
        PluginError::IOError(e)
    }
}

impl Display for PluginError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            PluginError::IOError(e) => write!(f, "{}", e)
        }
    }
}

/// A simple abstraction of a plugin that sends and receives messages.
pub type Plugin = Channel<BotMessageData, PluginMessageData>;

/// An enumeration of the errors that may occur while resolving a general audio
/// source and its associated plugin.
pub enum PluginResolutionError {

    /// Indicates that some error occurred during the creation of the audio
    /// source.
    Source(PluginSourceError),

    /// Indicates that no plugin was able to resolve the audio source's code
    /// (only possible if no audio source name has been explicitly provided).
    NoMatch,

    /// Indicates that multiple possible audio source types have been found for
    /// the given code (only possible if no audio source has been explicitly
    /// provided).
    MultipleMatches(Vec<String>),

    /// Indicates that an explicit name of an audio source that does not exist
    /// has been provided.
    UnknownSourceName
}

impl From<PluginSourceError> for PluginResolutionError {
    fn from(e: PluginSourceError) -> PluginResolutionError {
        PluginResolutionError::Source(e)
    }
}

impl Display for PluginResolutionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            PluginResolutionError::Source(e) => write!(f, "{}", e),
            PluginResolutionError::NoMatch =>
                write!(f, "No matching plugin was found."),
            PluginResolutionError::MultipleMatches(v) =>
                write!(f, "Multiple possible audio source types found: {}",
                    v.join(", ")),
            PluginResolutionError::UnknownSourceName =>
                write!(f, "No audio source of that type is registered.")
        }
    }
}

struct PlayCommand {
    name: Option<String>,
    code: String
}

impl FromStr for PlayCommand {
    type Err = ();

    fn from_str(s: &str) -> Result<PlayCommand, ()> {
        let mut chars = s.chars().peekable();
        let mut name = None;

        // TODO make configurable
        if Some(':') == chars.peek().cloned() {
            chars.next();
            let mut name_content = String::new();

            while let Some(c) = chars.next() {
                if c == ' ' {
                    break;
                }

                name_content.push(c);
            }

            name = Some(name_content);
        }

        Ok(PlayCommand {
            name,
            code: chars.collect()
        })
    }
}

/// Holds all the plugins with their child processes, so they can be killed
/// once they are no longer needed.
pub struct PluginManager {
    plugins: Vec<Plugin>,
    sources: HashMap<String, usize>,
    children: Vec<Child>
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        for child in &mut self.children {
            if let Err(e) = child.kill() {
                log::error!("Error while killing plugin process: {}", e);
            }
        }
    }
}

impl PluginManager {

    /// Creates a new plugin manager which currently holds no plugins.
    pub fn new() -> PluginManager {
        PluginManager {
            plugins: Vec::new(),
            children: Vec::new(),
            sources: HashMap::new()
        }
    }

    /// Registers a plugin to be managed by this manager. Returns the ID of the
    /// plugin for identification.
    pub fn register_plugin(&mut self, plugin: Plugin) -> usize {
        let result = self.plugins.len();
        self.plugins.push(plugin);
        result
    }

    /// Registers a child process to be managed by this manager.
    pub fn register_child(&mut self, child: Child) {
        self.children.push(child)
    }

    /// Registers an audio source with the given name for the plugin with the
    /// specified ID. Returns true if and only if registration was successful,
    /// i.e. no other source with the same name was registered before. Note
    /// that it is not checked whether the ID is valid. If it is not, future
    /// operations with this manager may fail.
    pub fn register_source(&mut self, id: usize, name: String) -> bool {
        if self.sources.contains_key(&name) {
            false
        }
        else {
            self.sources.insert(name, id);
            true
        }
    }

    /// Attempts to resolve an audio source from its (complete) command.
    pub fn resolve_source(&self, command: &str)
            -> Result<PluginAudioSource, PluginResolutionError> {
        let command = PlayCommand::from_str(command).unwrap();

        if let Some(name) = &command.name {
            if let Some(&source) = self.sources.get(name) {
                let plugin = self.plugins[source].clone();
                Ok(PluginAudioSource::resolve(plugin, name, &command.code)?)
            }
            else {
                Err(PluginResolutionError::UnknownSourceName)
            }
        }
        else {
            let mut matching_sources = Vec::new();
            let mut matching_plugin = None;

            for plugin in &self.plugins {
                let mut plugin = plugin.clone();
                let message = BotMessageData::CanResolve(command.code.clone());
                let conversation_id = plugin.send_new(message).unwrap();

                match plugin.receive_blocking(conversation_id) {
                    PluginMessageData::Resolution(mut name) => {
                        if !name.is_empty() {
                            matching_plugin = Some(plugin);
                            matching_sources.append(&mut name);
                        }
                    },
                    _ => {} // should not happen
                }
            }

            if matching_sources.len() == 0 {
                Err(PluginResolutionError::NoMatch)
            }
            else if matching_sources.len() > 1 {
                Err(PluginResolutionError::MultipleMatches(matching_sources))
            }
            else {
                let plugin = matching_plugin.unwrap();
                let name = &matching_sources[0];
                Ok(PluginAudioSource::resolve(plugin, name, &command.code)?)
            }
        }
    }
}

impl TypeMapKey for PluginManager {
    type Value = PluginManager;
}
