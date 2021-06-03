use rambot_api::communication::{
    BotMessage,
    ConversationId,
    PluginMessage,
    PluginMessageData
};

use serde::Serialize;

use serde_cbor::{Deserializer, Serializer};
use serde_cbor::de::IoRead;
use serde_cbor::ser::IoWrite;

use std::collections::{HashMap, VecDeque};
use std::fmt::{self, Display, Formatter};
use std::io;
use std::net::TcpStream;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::thread;

/// An enumeration of all erros that ma occur when setting up a plugin.
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
pub struct Plugin {
    serializer: Serializer<IoWrite<TcpStream>>,
    queues: Arc<Mutex<HashMap<ConversationId, VecDeque<PluginMessageData>>>>
}

fn listen(queues: Arc<Mutex<HashMap<ConversationId, VecDeque<PluginMessageData>>>>,
        deserializer: Deserializer<IoRead<TcpStream>>) {
    for msg_res in deserializer.into_iter::<PluginMessage>() {
        match msg_res {
            Ok(msg) => {
                let mut lock = queues.lock().unwrap();

                if !lock.contains_key(&msg.conversation_id()) {
                    if !msg.is_initial() {
                        log::error!("Received non-initial message in fresh conversation.");
                        continue;
                    }

                    lock.insert(msg.conversation_id(), VecDeque::new());
                }

                if msg.is_initial() {
                    log::error!("Received initial message in running conversation.");
                    continue;
                }

                lock.get_mut(&msg.conversation_id()).unwrap().push_back(msg.into_data());
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
        let queues = Arc::new(Mutex::new(HashMap::new()));
        let queues_clone = Arc::clone(&queues);
        thread::spawn(||
            listen(
                queues_clone,
                Deserializer::new(IoRead::new(stream))));
        Ok(Plugin {
            serializer,
            queues
        })
    }

    /// Sends the given [BotMessage] to the plugin.
    pub fn send(&mut self, message: BotMessage)
            -> Result<(), serde_cbor::Error> {
        let mut queues = self.queues.lock().unwrap();

        if !queues.contains_key(&message.conversation_id()) {
            queues.insert(message.conversation_id(), VecDeque::new());
        }

        message.serialize(&mut self.serializer)
    }

    /// Returns the next available plugin message in the conversation with the
    /// given ID, if there currently is a cached one. This is non-blocking,
    /// i.e. if no message is currently queued, `None` will be returned.
    pub fn receive(&self, id: ConversationId) -> Option<PluginMessageData> {
        self.queues.lock().unwrap().get_mut(&id)
            .and_then(|q| q.pop_front())
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
