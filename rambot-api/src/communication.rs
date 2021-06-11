//! This module defines the data structures that the bot and a plugin exchange
//! as well as basic data exchange functionality. It is usually not necessary
//! to use these if you write a plugin.

use crate::audio::Sample;
use crate::util::{MultiJoinHandle, ObservableQueue, TransactionalWrite};

use rand::Rng;

use serde::{Deserialize, Serialize};

use serde_cbor::Deserializer;
use serde_cbor::de::IoRead;

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::convert::TryFrom;
use std::fmt::{self, Display, Formatter};
use std::marker::PhantomData;
use std::net::TcpStream;
use std::num::ParseIntError;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;

/// An enumeration of the different categories of messages. These are used to
/// identify messages that belong together.
#[derive(Copy, Clone, Eq, Hash, PartialEq)]
pub enum MessageCategory {

    /// Messages dealing with the registration of audio sources.
    Registration,

    /// Messages dealing with the resolution of audio sources.
    Resolution,

    /// Messages dealing with the playback of audio sources.
    Audio
}

/// A trait for all message data types.
pub trait MessageData {

    /// Indicates whether messages of this data type can be the initial
    /// messages in a new conversation.
    const CAN_CREATE_CONVERSATIONS: bool;

    /// Gets the category of messages this message belongs to.
    fn category(&self) -> MessageCategory;
}

/// The reason for a [BotMessageData::RegisterErr] message.
#[derive(Deserialize, Serialize)]
pub enum RegisterErrReason {

    /// Indicates that there is already an audio source with the same name
    /// registered.
    DuplicateName
}

/// The data of a message sent from the bot to a plugin.
#[derive(Deserialize, Serialize)]
pub enum BotMessageData {

    /// An indication by the bot that the plugin may start with registration of
    /// audio sources as a response to this message.
    StartRegistration,

    /// A response by the bot that an audio source was succesfully registred.
    SourceOk,

    /// A response by the bot that an audio source could not be registered. The
    /// reason is provided.
    SourceErr(RegisterErrReason),

    /// A request by the bot for the plugin to determine whether the given
    /// string is the code of a valid audio source (e.g. the path of an
    /// existing file). It is expected that the plugin responds with a
    /// [PluginMessageData::CanResolve] message.
    CanResolve(String),

    /// A request by the bot for the plugin to attempt to setup an audio source
    /// using the given name and code. It is expected that the plugin responds
    /// with a [PluginMessageData::SetupOk] message if the setup is complete
    /// and audio data can be sent and a [PluginMessageData::SetupErr] message
    /// otherwise.
    SetupSource {

        /// The name of the audio source type.
        name: String,

        /// The code which was provided by the user.
        code: String
    },

    /// An indicator by the bot that the plugin can send samples with indices
    /// less than the given bound.
    SendUntil(u64),

    /// A request by the bot for the plugin to close the audio source. No
    /// response is expected, but all further audio data for the given index
    /// will be dropped.
    CloseSource
}

impl MessageData for BotMessageData {

    const CAN_CREATE_CONVERSATIONS: bool = true;

    fn category(&self) -> MessageCategory {
        match self {
            BotMessageData::StartRegistration
                | BotMessageData::SourceOk
                | BotMessageData::SourceErr(_) =>
                    MessageCategory::Registration,
            BotMessageData::CanResolve(_) => MessageCategory::Resolution,
            BotMessageData::SetupSource { .. }
                | BotMessageData::SendUntil(_)
                | BotMessageData::CloseSource => MessageCategory::Audio
        }
    }
}

/// The data of a message sent from a plugin to the bot.
#[derive(Deserialize, Serialize)]
pub enum PluginMessageData {

    /// A request by the plugin to register an audio source with the bot. It
    /// contains the source type's name.
    RegisterSource(String),

    /// Indicates that the plugin wants to finish the registration phase.
    RegistrationFinished,

    /// A response to a [BotMessageData::CanResolve] message that indicates
    /// whether the code represents a valid audio source for this plugin. In
    /// this case, the wrapped value will contain the name of all audio source
    /// types that can play the given code. Otherwise, it will be empty.
    Resolution(Vec<String>),

    /// A response to a [BotMessageData::SetupSource] message which indicates
    /// that audio data can now be sent.
    SetupOk,

    /// A response to a [BotMessageData::SetupSource] message which indicates
    /// that setting up the audio source has failed. An error message is
    /// provided.
    SetupErr(String),

    /// A message containing audio data from the source.
    AudioData(Vec<Sample>)
}

impl MessageData for PluginMessageData {

    const CAN_CREATE_CONVERSATIONS: bool = false;

    fn category(&self) -> MessageCategory {
        match self {
            PluginMessageData::RegisterSource(_)
                | PluginMessageData::RegistrationFinished =>
                    MessageCategory::Registration,
            PluginMessageData::Resolution(_) => MessageCategory::Resolution,
            PluginMessageData::SetupOk
                | PluginMessageData::SetupErr(_)
                | PluginMessageData::AudioData(_) => MessageCategory::Audio
        }
    }
}

/// A unique identifier of a conversation. A conversation is defined as a
/// sequence of messages which relate to each other, for example a registration
/// request and an accepting response.
#[derive(Copy, Clone, Eq, Hash, PartialEq)]
pub struct ConversationId {
    category: MessageCategory,
    id: u64
}

impl ConversationId {

    /// Gets the category of messages posted in this conversation.
    pub fn category(&self) -> MessageCategory {
        self.category
    }

    /// Gets the category-internal ID of this conversation.
    pub fn internal_id(&self) -> u64 {
        self.id
    }
}

/// A general message, which contains an identification to associate it with
/// some other message, and some data.
#[derive(Deserialize, Serialize)]
pub struct Message<D: MessageData> {
    id: u64,
    data: D
}

impl<D: MessageData> Message<D> {

    /// Creates a new message with the given identifier and some data.
    pub fn new(id: u64, data: D) -> Message<D> {
        Message {
            id,
            data
        }
    }

    /// Gets the identifier of this message.
    pub fn conversation_id(&self) -> ConversationId {
        ConversationId {
            category: self.data().category(),
            id: self.id
        }
    }

    /// Gets a reference to the wrapped data.
    pub fn data(&self) -> &D {
        &self.data
    }

    /// Transfers ownership of the wrapped data to the caller.
    pub fn into_data(self) -> D {
        self.data
    }
}

/// A message sent by the bot to a plugin.
pub type BotMessage = Message<BotMessageData>;

/// A message sent by a plugin to the bot.
pub type PluginMessage = Message<PluginMessageData>;

/// An enumeration of the different errors that may occur when parsing
/// [Token]s.
#[derive(Debug)]
pub enum ParseTokenError {

    /// Indicates that the string had the wrong length.
    InvalidLength(usize),

    /// Indicates that one of the characters in the string was not a valid hex
    /// digit.
    InvalidDigit
}

impl From<ParseIntError> for ParseTokenError {
    fn from(_: ParseIntError) -> ParseTokenError {
        ParseTokenError::InvalidDigit
    }
}

/// The number of u64-values in a token
pub const TOKEN_SIZE: usize = 2;

/// A unique identifying token for plugin apps or unnamed audio sources.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Token {
    parts: [u64; TOKEN_SIZE]
}

impl Token {

    /// Creates a new, random token.
    pub fn new() -> Token {
        let mut rng = rand::thread_rng();
        let mut parts = [0; TOKEN_SIZE];

        for i in 0..TOKEN_SIZE {
            parts[i] = rng.gen();
        }

        Token {
            parts
        }
    }
}

impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for part in self.parts.iter() {
            write!(f, "{:016x}", part)?;
        }

        Ok(())
    }
}

impl FromStr for Token {
    type Err = ParseTokenError;

    fn from_str(s: &str) -> Result<Token, ParseTokenError> {
        if s.len() != 16 * TOKEN_SIZE {
            return Err(ParseTokenError::InvalidLength(s.len()));
        }

        let mut parts = [0; TOKEN_SIZE];

        for i in 0..TOKEN_SIZE {
            let start = i * 16;
            let end = start + 16;
            parts[i] = u64::from_str_radix(&s[start..end], 16)?;
        }

        Ok(Token {
            parts
        })
    }
}

impl Display for ParseTokenError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ParseTokenError::InvalidLength(l) =>
                write!(f, "Token has {} hex chars, but needs {}.", l,
                    TOKEN_SIZE * 2),
            ParseTokenError::InvalidDigit =>
                write!(f, "Token contains invalid digit.")
        }
    }
}

/// An initial message sent by a newly connected TCP client which indicates
/// what the client intended to communicate with this connection.
#[derive(Deserialize, Serialize)]
pub enum ConnectionIntent {

    /// Indicates that the client wants to use the connection for a new plugin.
    /// The client's token is provided for identification.
    RegisterPlugin(Token),

    /// Indicates that the client will no longer register new plugins. The
    /// client's token is provided for identification.
    CloseRegistration(Token)
}

impl TryFrom<&mut TcpStream> for ConnectionIntent {
    type Error = serde_cbor::Error;

    fn try_from(stream: &mut TcpStream) -> Result<ConnectionIntent, serde_cbor::Error> {
        let mut de = Deserializer::new(IoRead::new(stream));
        ConnectionIntent::deserialize(&mut de)
    }
}

/// Manages message queues of received messages.
struct Queues<D: MessageData> {
    queues: HashMap<MessageCategory, HashMap<u64, ObservableQueue<D>>>
}

impl<D: MessageData> Queues<D> {

    /// Creates new, empty queues.
    fn new() -> Queues<D> {
        Queues {
            queues: HashMap::new()
        }
    }

    fn queue_mut(&mut self, conversation: ConversationId)
            -> Option<&mut ObservableQueue<D>> {
        self.queues
            .get_mut(&conversation.category())
            .and_then(|m| m.get_mut(&conversation.internal_id()))
    }

    /// Ensures that a message queue for the given conversation exists. This
    /// method returns true if and only if a new queue was created.
    fn ensure_exists(&mut self, conversation: ConversationId) -> bool {
        let entry = self.queues
            .entry(conversation.category())
            .or_insert_with(|| HashMap::new())
            .entry(conversation.internal_id());
        let result = matches!(entry, Entry::Vacant(_));
        entry.or_insert_with(|| ObservableQueue::new());
        result
    }

    /// Enqueues a message to the respective queue for its conversation, if
    /// there is one. In this case, true is returned, but if there is no queue,
    /// false is returned.
    fn enqueue(&mut self, message: Message<D>) -> bool {
        self.queue_mut(message.conversation_id())
            .map(|q| q.enqueue(message.into_data()))
            .is_some()
    }

    /// Dequeues a message from the queue assigned to the given conversation.
    /// If there is no queue or there is one, but it is empty, this method
    /// returns none.
    fn dequeue(&mut self, conversation: ConversationId) -> Option<D> {
        self.queue_mut(conversation)
            .and_then(|q| q.dequeue())
    }

    fn observe(&mut self, conversation: ConversationId) -> Option<Receiver<D>> {
        self.queue_mut(conversation).map(|q| q.observe())
    }
}

/// A channel manages the communication of [Message]s over a [TcpStream].
pub struct Channel<S, R>
where
    S: MessageData + Serialize,
    for<'de> R: MessageData + Deserialize<'de> + Send + 'static
{
    stream: TransactionalWrite<TcpStream>,
    queues: Arc<Mutex<Queues<R>>>,
    next_ids: Arc<Mutex<HashMap<MessageCategory, u64>>>,
    new_conversations: Option<Arc<Mutex<ObservableQueue<ConversationId>>>>,
    listener: MultiJoinHandle<()>,
    send_type: PhantomData<S>
}

fn listen<R>(queues: Arc<Mutex<Queues<R>>>,
    deserializer: Deserializer<IoRead<TcpStream>>,
    new_conversations: Option<Arc<Mutex<ObservableQueue<ConversationId>>>>)
where
    for<'de> R: MessageData + Deserialize<'de>
{
    for msg_res in deserializer.into_iter::<Message<R>>() {
        match msg_res {
            Ok(msg) => {
                let mut queues = queues.lock().unwrap();

                if let Some(new_conversations) = &new_conversations {
                    let conversation = msg.conversation_id();

                    if queues.ensure_exists(conversation) {
                        new_conversations.lock().unwrap()
                            .enqueue(conversation);
                    }

                    queues.enqueue(msg);
                }
                else {
                    if !queues.enqueue(msg) {
                        log::error!(
                            "Received message in non-existent conversation.");
                    }
                }
            },
            Err(e) =>
                log::error!("Error deserializing message: {}", e)
        }
    }
}

impl<S, R> Channel<S, R>
where
    S: MessageData + Serialize,
    for<'de> R: MessageData + Deserialize<'de> + Send + 'static
{

    /// Creates a new plugin that uses the given TCP stream for communication.
    pub fn new(stream: TcpStream) -> Channel<S, R> {
        let queues = Arc::new(Mutex::new(Queues::new()));
        let queues_clone = Arc::clone(&queues);
        let stream_clone = stream.try_clone().unwrap();
        let new_conversations = if R::CAN_CREATE_CONVERSATIONS {
            Some(Arc::new(Mutex::new(ObservableQueue::new())))
        }
        else {
            None
        };
        let new_conversations_clone =
            new_conversations.as_ref().map(Arc::clone);
        let listener = MultiJoinHandle::new(thread::spawn(||
            listen(
                queues_clone,
                Deserializer::new(IoRead::new(stream_clone)),
                new_conversations_clone)));
        Channel {
            stream: TransactionalWrite::new(stream),
            queues,
            next_ids: Arc::new(Mutex::new(HashMap::new())),
            new_conversations,
            listener,
            send_type: PhantomData
        }
    }

    fn get_next_id(&mut self, category: MessageCategory) -> u64 {
        *self.next_ids.lock().unwrap().entry(category)
            .and_modify(|id| *id += 1)
            .or_insert(0)
    }

    /// Sends the given [Message] through this channel.
    pub fn send(&mut self, message: Message<S>)
            -> Result<(), serde_cbor::Error> {
        self.queues.lock().unwrap().ensure_exists(message.conversation_id());
        let mut transaction = self.stream.open_transaction();
        serde_cbor::to_writer(&mut transaction, &message)?;
        transaction.commit()?;
        Ok(())
    }

    /// Sends the given message data as the first message of a new conversation
    /// through this channel.
    pub fn send_new(&mut self, message_data: S)
            -> Result<ConversationId, serde_cbor::Error> {
        let id = self.get_next_id(message_data.category());
        let message = Message::new(id, message_data);
        let conversation_id = message.conversation_id();
        self.send(message).map(|_| conversation_id)
    }

    /// Returns the next available message in the conversation with the given
    /// ID, if there currently is a cached one. This is non-blocking, i.e. if
    /// no message is currently queued, `None` will be returned.
    pub fn receive(&self, id: ConversationId) -> Option<R> {
        self.queues.lock().unwrap().dequeue(id)
    }

    /// Listens for messages in the conversation with the given ID until a
    /// message was received or the timeout was passed.
    pub fn receive_for(&self, id: ConversationId, timeout: Duration)
            -> Option<R> {
        let receiver = self.queues.lock().unwrap().observe(id);
        receiver.and_then(|r| r.recv_timeout(timeout).ok())
    }

    /// Blocks the thread until a new message is received. This returns `None`
    /// only if there is no conversation with the given ID.
    pub fn receive_blocking(&self, id: ConversationId) -> Option<R> {
        let receiver = self.queues.lock().unwrap().observe(id);
        receiver.map(|r| r.recv().unwrap())
    }

    /// Returns the first message in the next new conversation, if there is
    /// currently a cached one. This is non-blocking, i.e. if there is no new
    /// conversation, `None` will be returned. It is assumed that no messages
    /// of this conversation have been received manually using
    /// [Channel::receive], [Channel::receive_for], or
    /// [Channel::receive_blocking].
    pub fn receive_new(&self) -> Option<Message<R>> {
        let id = {
            let mut new_conversations =
                self.new_conversations.as_ref().unwrap().lock().unwrap();
            new_conversations.dequeue()
        };
        id.and_then(|id|
            Some(Message::new(id.internal_id(), self.receive(id)?)))
    }

    /// Blocks the thread until a message in a new conversation is received.
    pub fn receive_new_blocking(&self) -> Message<R> {
        let receiver = self.new_conversations.as_ref().unwrap().lock().unwrap()
            .observe();
        let id = receiver.recv().unwrap();
        let data = self.queues.lock().unwrap().dequeue(id).unwrap();
        Message::new(id.internal_id(), data)
    }

    /// Waits for the listener thread to terminate, which indicates that the
    /// stream has ended and the bot has been closed.
    pub fn await_listener(&self) {
        self.listener.join()
    }

    /// Indicates whether this channel has ended, i.e. there will be no more
    /// new messages. This does not imply that all bufferes have been emptied,
    /// just that they will not be filled any more.
    pub fn has_ended(&self) -> bool {
        self.listener.has_terminated()
    }
}

impl<S, R> Clone for Channel<S, R>
where
    S: MessageData + Serialize,
    for<'de> R: MessageData + Deserialize<'de> + Send + 'static
{
    fn clone(&self) -> Channel<S, R> {
        Channel {
            stream: self.stream.clone(),
            queues: Arc::clone(&self.queues),
            next_ids: Arc::clone(&self.next_ids),
            new_conversations: self.new_conversations.as_ref().map(Arc::clone),
            listener: self.listener.clone(),
            send_type: PhantomData
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn token_equality() {
        let token_1 = Token::new();
        let token_2 = Token::new();

        assert!(&token_1 != &token_2);
        assert!(&token_1 == &token_1.clone())
    }

    #[test]
    fn token_parsing_consistency() {
        let original = Token::new();
        let string = original.to_string();
        let parsed = Token::from_str(&string).unwrap();

        assert_eq!(original, parsed);
    }
}
