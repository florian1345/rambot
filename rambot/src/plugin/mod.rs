use crate::plugin::source::{PluginSourceError, PluginAudioSource};

use rambot_api::communication::{
    BotMessage,
    BotMessageData,
    ConversationId,
    MessageCategory,
    MessageData,
    PluginMessage,
    PluginMessageData
};

use serde_cbor::Deserializer;
use serde_cbor::de::IoRead;

use serenity::prelude::TypeMapKey;

use std::collections::{HashMap, VecDeque};
use std::fmt::{self, Display, Formatter};
use std::io;
use std::net::TcpStream;
use std::process::Child;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub mod load;
pub mod source;

const POLL_INTERVAL: Duration = Duration::from_millis(10);

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

struct Queues {
    queues: HashMap<MessageCategory, HashMap<u64, VecDeque<PluginMessageData>>>
}

impl Queues {
    fn new() -> Queues {
        Queues {
            queues: HashMap::new()
        }
    }

    fn queue_mut(&mut self, conversation: ConversationId)
            -> Option<&mut VecDeque<PluginMessageData>> {
        self.queues
            .get_mut(&conversation.category())
            .and_then(|m| m.get_mut(&conversation.internal_id()))
    }

    fn ensure_exists(&mut self, conversation: ConversationId) {
        self.queues
            .entry(conversation.category())
            .or_insert_with(|| HashMap::new())
            .entry(conversation.internal_id())
            .or_insert_with(|| VecDeque::new());
    }

    fn enqueue(&mut self, message: PluginMessage) -> bool {
        self.queue_mut(message.conversation_id())
            .map(|q| q.push_back(message.into_data()))
            .is_some()
    }

    fn dequeue(&mut self, conversation: ConversationId)
            -> Option<PluginMessageData> {
        self.queue_mut(conversation)
            .and_then(|q| q.pop_front())
    }
}

/// A simple abstraction of a plugin that sends and receives messages.
pub struct Plugin {
    stream: TcpStream,
    queues: Arc<Mutex<Queues>>,
    next_ids: Arc<Mutex<HashMap<MessageCategory, u64>>>
}

fn listen(queues: Arc<Mutex<Queues>>,
        deserializer: Deserializer<IoRead<TcpStream>>) {
    for msg_res in deserializer.into_iter::<PluginMessage>() {
        match msg_res {
            Ok(msg) =>
                if !queues.lock().unwrap().enqueue(msg) {
                    log::error!(
                        "Plugin sent message in non-existent conversation.");
                },
            Err(e) =>
                log::error!("Error deserializing plugin message: {}", e)
        }
    }
}

impl Plugin {

    /// Creates a new plugin that uses the given TCP stream for communication.
    pub fn new(stream: TcpStream) -> Result<Plugin, PluginError> {
        let queues = Arc::new(Mutex::new(Queues::new()));
        let queues_clone = Arc::clone(&queues);
        let stream_clone = stream.try_clone().unwrap();
        thread::spawn(||
            listen(
                queues_clone,
                Deserializer::new(IoRead::new(stream_clone))));
        Ok(Plugin {
            stream,
            queues,
            next_ids: Arc::new(Mutex::new(HashMap::new()))
        })
    }

    fn get_next_id(&mut self, category: MessageCategory) -> u64 {
        *self.next_ids.lock().unwrap().entry(category)
            .and_modify(|id| *id += 1)
            .or_insert(0)
    }

    /// Sends the given [BotMessage] to the plugin.
    pub fn send(&mut self, message: BotMessage)
            -> Result<(), serde_cbor::Error> {
        self.queues.lock().unwrap().ensure_exists(message.conversation_id());
        serde_cbor::to_writer(&mut self.stream, &message)
    }

    /// Sends the given [BotMessageData] as the first message of a new
    /// conversation to the plugin.
    pub fn send_new(&mut self, message_data: BotMessageData)
            -> Result<ConversationId, serde_cbor::Error> {
        let id = self.get_next_id(message_data.category());
        let message = BotMessage::new(id, message_data);
        let conversation_id = message.conversation_id();
        self.send(message).map(|_| conversation_id)
    }

    /// Returns the next available plugin message in the conversation with the
    /// given ID, if there currently is a cached one. This is non-blocking,
    /// i.e. if no message is currently queued, `None` will be returned.
    pub fn receive(&self, id: ConversationId) -> Option<PluginMessageData> {
        self.queues.lock().unwrap().dequeue(id)
    }

    /// Listens for messages in the conversation with the given ID until a
    /// message was received or the timeout was passed.
    pub fn receive_for(&self, id: ConversationId, timeout: Duration)
            -> Option<PluginMessageData> {
        let start = Instant::now();

        while (Instant::now() - start) < timeout {
            let msg = self.receive(id);

            if msg.is_some() {
                return msg;
            }

            thread::sleep(POLL_INTERVAL)
        }

        None
    }

    /// Blocks the thread until a new message is received.
    pub fn receive_blocking(&self, id: ConversationId) -> PluginMessageData {
        // TODO remove polling
        loop {
            if let Some(msg) = self.receive(id) {
                return msg;
            }

            thread::sleep(POLL_INTERVAL)
        }
    }
}

impl Clone for Plugin {
    fn clone(&self) -> Plugin {
        Plugin {
            stream: self.stream.try_clone().unwrap(),
            queues: Arc::clone(&self.queues),
            next_ids: Arc::clone(&self.next_ids)
        }
    }
}

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
