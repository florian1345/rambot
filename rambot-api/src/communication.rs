//! This module defines the data structures that the bot and a plugin exchange.
//! It is usually not necessary to use these if you write a plugin.

use crate::audio::Sample;

use serde::{Deserialize, Serialize};

/// The data of a message sent from the bot to a plugin.
#[derive(Deserialize, Serialize)]
pub enum BotMessageData {

    /// A request by the bot for the plugin to determine whether the given
    /// string is the code of a valid audio source (e.g. the path of an
    /// existing file). It is expected that the plugin responds with a
    /// [PluginMessageData::CanResolve] message.
    CanResolve(String),

    /// A request by the bot for the plugin to attempt to setup an audio source
    /// using the given code. It is expected that the plugin responds with a
    /// [PluginMessageData::SetupOk] message if the setup is complete and audio
    /// data can be sent and a [PluginMessageData::SetupErr] message otherwise.
    SetupSource(String),

    /// An indicator by the bot that the plugin can send samples with indices
    /// less than the given bound.
    SendUntil(u64),

    /// A request by the bot for the plugin to close the audio source. No
    /// response is expected, but all further audio data for the given index
    /// will be dropped.
    CloseSource
}

/// The data of a message sent from a plugin to the bot.
#[derive(Deserialize, Serialize)]
pub enum PluginMessageData {

    /// A response to a [BotMessageData::CanResolve] message that indicates
    /// whether the code represents a valid audio source for this plugin. In
    /// this case, the wrapped bool will be true, otherwise false.
    CanResolve(bool),

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

/// A general message, which contains an identification to associate it with
/// some other message, and some data.
#[derive(Deserialize, Serialize)]
pub struct Message<D> {
    id: u64,
    data: D
}
