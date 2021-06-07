//! This module defines the data structures that the bot and a plugin exchange.
//! It is usually not necessary to use these if you write a plugin.

use crate::audio::Sample;

use serde::{Deserialize, Serialize};

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
