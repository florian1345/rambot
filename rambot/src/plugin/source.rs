use crate::plugin::Plugin;

use rambot_api::audio::{AudioSource, Sample};
use rambot_api::communication::{
    BotMessage,
    BotMessageData,
    ConversationId,
    PluginMessageData
};

use std::collections::VecDeque;
use std::time::Duration;

const RESOLUTION_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_BUFFER: u64 = 4096;

/// An enumeration of the errors that may occur when creating a plugin audio
/// source.
pub enum PluginAudioError {

    /// Indicates that the plugin raised an error with the provided message.
    ResolutionError(String),

    /// Indicates that the plugin did not respond to the resolution request
    /// within the timeout.
    Timeout
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
    pub fn resolve(plugin: &Plugin, name: &str, code: &str)
            -> Result<PluginAudioSource, PluginAudioError> {
        let mut plugin = Plugin::clone(plugin);
        let conversation = plugin.send_new(BotMessageData::SetupSource {
            name: name.to_owned(),
            code: code.to_owned()
        }).unwrap();

        match plugin.receive_for(conversation, RESOLUTION_TIMEOUT) {
            Some(PluginMessageData::SetupOk) =>
                Ok(PluginAudioSource {
                    plugin,
                    conversation,
                    buffer: VecDeque::new(),
                    processed: 0,
                    finished: false
                }),
            Some(PluginMessageData::SetupErr(msg)) =>
                Err(PluginAudioError::ResolutionError(msg)),
            Some(_) =>
                panic!("Plugin sent invalid message."), // should not happen
            None => Err(PluginAudioError::Timeout)
        }
    }
}

impl AudioSource for PluginAudioSource {
    fn next(&mut self) -> Option<Sample> {
        let dequeued = self.buffer.pop_front();

        if dequeued.is_some() {
            self.processed += 1;
            return dequeued;
        }

        if self.finished {
            return None;
        }

        let m = BotMessageData::SendUntil(self.processed + MAX_BUFFER);
        self.plugin.send(BotMessage::new(self.conversation.internal_id(), m))
            .unwrap();

        match self.plugin.receive_blocking(self.conversation) {
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
