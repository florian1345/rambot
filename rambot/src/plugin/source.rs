use crate::plugin::Plugin;

use rambot_api::audio::{AudioSource, Sample};
use rambot_api::communication::{
    BotMessage,
    BotMessageData,
    ConversationId,
    PluginMessageData
};

use std::collections::VecDeque;
use std::fmt::{self, Display, Formatter};
use std::time::Duration;

const RESOLUTION_TIMEOUT: Duration = Duration::from_secs(10);
const BUFFER_SIZE: u64 = 4096;

/// After this amount of samples a new "SendUntil" request will be sent to the
/// plugin to keep the buffer filled.
const REQUEST_EVERY: usize = BUFFER_SIZE as usize / 4;

/// An enumeration of the errors that may occur when creating a plugin audio
/// source.
pub enum PluginSourceError {

    /// Indicates that the plugin raised an error with the provided message.
    ResolutionError(String),

    /// Indicates that the plugin did not respond to the resolution request
    /// within the timeout.
    Timeout
}

impl Display for PluginSourceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            PluginSourceError::ResolutionError(msg) =>
                write!(f, "The plugin raised an error: {}", msg),
            PluginSourceError::Timeout =>
                write!(f, "The plugin timed out during resolution.")
        }
    }
}

/// An [AudioSource] abstraction which handles the reception of audio data from
/// a [Plugin] and maintains the conversation until it is finished.
pub struct PluginAudioSource {
    plugin: Plugin,
    conversation: ConversationId,
    buffer: VecDeque<Sample>,
    processed: u64,
    finished: bool
}

impl PluginAudioSource {

    /// Resolves an audio source using the given plugin.
    ///
    /// # Arguments
    ///
    /// * `plugin`: The [Plugin] from which to receive audio data.
    /// * `name`: The name of the audio source, which has been advertised by
    /// the plugin during the setup phase. It is either provided by the user or
    /// deduced by the plugin itself.
    /// * `code`: The actual code of the audio source, which defines what audio
    /// is played.
    ///
    /// # Errors
    ///
    /// If the plugin cannot resolve the given code, a
    /// [PluginAudioError::ResolutionError] is raised. If the plugin does not
    /// respond within the timeout, a [PluginAudioError::Timeout] is raised.
    pub fn resolve(mut plugin: Plugin, name: &str, code: &str)
            -> Result<PluginAudioSource, PluginSourceError> {
        let conversation = plugin.send_new(BotMessageData::SetupSource {
            name: name.to_owned(),
            code: code.to_owned()
        }).unwrap();

        match plugin.receive_for(conversation, RESOLUTION_TIMEOUT) {
            Some(PluginMessageData::SetupOk) => {
                let data = BotMessageData::SendUntil(BUFFER_SIZE);
                let msg = BotMessage::new(conversation.internal_id(), data);
                plugin.send(msg).unwrap();
                Ok(PluginAudioSource {
                    plugin,
                    conversation,
                    buffer: VecDeque::with_capacity(BUFFER_SIZE as usize),
                    processed: 0,
                    finished: false
                })
            },
            Some(PluginMessageData::SetupErr(msg)) =>
                Err(PluginSourceError::ResolutionError(msg)),
            Some(_) =>
                panic!("Plugin sent invalid message."), // should not happen
            None => Err(PluginSourceError::Timeout)
        }
    }
}

impl AudioSource for PluginAudioSource {
    fn next(&mut self) -> Option<Sample> {
        let dequeued = self.buffer.pop_front();

        if dequeued.is_some() {
            self.processed += 1;

            if self.buffer.len() % REQUEST_EVERY == 0 {
                let data =
                    BotMessageData::SendUntil(self.processed + BUFFER_SIZE);
                let msg =
                    BotMessage::new(self.conversation.internal_id(), data);
                self.plugin.send(msg).unwrap();
            }

            return dequeued;
        }

        if self.finished {
            return None;
        }

        match self.plugin.receive_blocking(self.conversation).unwrap() {
            PluginMessageData::AudioData(d) => {
                if d.len() == 0 {
                    self.finished = true;
                    return None;
                }

                for s in d {
                    self.buffer.push_back(s);
                }

                self.buffer.pop_front()
            },
            _ => panic!("Plugin sent invalid message."), // should not happen
        }
    }
}

impl Drop for PluginAudioSource {
    fn drop(&mut self) {
        self.plugin.send(BotMessage::new(self.conversation.internal_id(),
            BotMessageData::CloseSource)).unwrap();
    }
}
