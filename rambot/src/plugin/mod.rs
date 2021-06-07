use rambot_api::communication::{
    BotMessage,
    BotMessageData,
    ConversationId,
    MessageCategory,
    MessageData,
    PluginMessage,
    PluginMessageData
};

use serde::Serialize;

use serde_cbor::{Deserializer, Serializer};
use serde_cbor::de::IoRead;
use serde_cbor::ser::IoWrite;

use serenity::prelude::TypeMapKey;

use std::collections::{HashMap, VecDeque};
use std::fmt::{self, Display, Formatter};
use std::io;
use std::net::TcpStream;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::thread;

pub mod load;

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
    serializer: Serializer<IoWrite<TcpStream>>,
    queues: Arc<Mutex<Queues>>,
    next_ids: HashMap<MessageCategory, u64>
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
        let serializer = Serializer::new(IoWrite::new(stream.try_clone()?));
        let queues = Arc::new(Mutex::new(Queues::new()));
        let queues_clone = Arc::clone(&queues);
        thread::spawn(||
            listen(
                queues_clone,
                Deserializer::new(IoRead::new(stream))));
        Ok(Plugin {
            serializer,
            queues,
            next_ids: HashMap::new()
        })
    }

    fn get_next_id(&mut self, category: MessageCategory) -> u64 {
        *self.next_ids.entry(category)
            .and_modify(|id| *id += 1)
            .or_insert(0)
    }

    /// Sends the given [BotMessage] to the plugin.
    pub fn send(&mut self, message: BotMessage)
            -> Result<(), serde_cbor::Error> {
        self.queues.lock().unwrap().ensure_exists(message.conversation_id());
        message.serialize(&mut self.serializer)
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
}

/// Holds all the plugins with their child processes, so they can be killed
/// once they are no longer needed.
pub struct PluginManager {
    plugins: Vec<Arc<Mutex<Plugin>>>,
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
            children: Vec::new()
        }
    }

    /// Registers a plugin to be managed by this manager.
    pub fn register_plugin(&mut self, plugin: Plugin) {
        self.plugins.push(Arc::new(Mutex::new(plugin)));
    }

    /// Registers a child process to be managed by this manager.
    pub fn register_child(&mut self, child: Child) {
        self.children.push(child)
    }
}

impl TypeMapKey for PluginManager {
    type Value = PluginManager;
}
