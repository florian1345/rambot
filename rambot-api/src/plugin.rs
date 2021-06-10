//! This module contains functionality to create a [PluginApp], i.e. an
//! application that runs plugins for Rambot.

use crate::audio::{AudioSource, Sample};
use crate::communication::{
    BotMessageData,
    Channel,
    ConversationId,
    Message,
    PluginMessageData
};

use rand::Rng;

use std::collections::HashMap;
use std::env;
use std::io;
use std::marker::PhantomData;
use std::net::TcpStream;
use std::thread;

/// An abstract representation of a type of audio source which can be
/// constructed or resolved.
pub trait AudioSourceProvider<S: AudioSource> {

    /// Indicates whether this provider is able to construct an audio source
    /// from the given code.
    fn can_resolve(&self, code: &str) -> bool;

    /// Constructs an audio source from the given code. In case an error
    /// occurs, a message wrapped in an `Err` variant is returned.
    fn resolve(&self, code: &str) -> Result<S, String>;
}

struct BoxedAudioSourceProvider<S, P>
where
    S: AudioSource + Send + 'static,
    P: AudioSourceProvider<S>
{
    provider: P,
    source_type: PhantomData<S>
}

impl<S, P> AudioSourceProvider<Box<dyn AudioSource + Send>>
for BoxedAudioSourceProvider<S, P>
where
    S: AudioSource + Send + 'static,
    P: AudioSourceProvider<S>
{
    fn can_resolve(&self, code: &str) -> bool {
        self.provider.can_resolve(code)
    }

    fn resolve(&self, code: &str) -> Result<Box<dyn AudioSource + Send>, String> {
        Ok(Box::new(self.provider.resolve(code)?))
    }
}

type DynAudioSourceProvider =
    Box<dyn AudioSourceProvider<Box<dyn AudioSource + Send>>>;

fn to_dyn<S, P>(provider: P) -> DynAudioSourceProvider
where
    S: AudioSource + Send + 'static,
    P: AudioSourceProvider<S> +  'static
{
    Box::new(BoxedAudioSourceProvider {
        provider,
        source_type: PhantomData
    })
}

type Bot = Channel<PluginMessageData, BotMessageData>;

fn read_samples(source: &mut dyn AudioSource, max_len: usize) -> Vec<Sample> {
    let mut result = Vec::new();

    while let Some(sample) = source.next() {
        result.push(sample);

        if result.len() >= max_len {
            break;
        }
    }

    result
}

const MAX_BATCH_SIZE: usize = 1024;

fn manage_source(mut source: Box<dyn AudioSource + Send>, mut bot: Bot,
        id: ConversationId) {
    loop {
        let mut sent = 0u64;
        let mut expected = 0u64;

        while let Some(msg) = bot.receive(id) {
            match msg {
                BotMessageData::SendUntil(next_expected) =>
                    expected = next_expected,
                BotMessageData::CloseSource => return,
                _ => {} // should not happen
            }
        }

        while sent < expected {
            let max_len = ((expected - sent) as usize).min(MAX_BATCH_SIZE);
            let samples = read_samples(source.as_mut(), max_len);
            let len = samples.len();
            let data = PluginMessageData::AudioData(samples);
            bot.send(Message::new(id.internal_id(), data)).unwrap();

            if len == 0 {
                return;
            }

            sent += len as u64;
        }
    }
}

/// An abstract representation of a plugin that can connect to the bot. As a
/// user, you do not have to interact with this struct beyond registering it
/// with a [PluginApp]. You can construct it with a [PluginBuilder].
pub struct Plugin {
    named_audio_source_providers: HashMap<String, DynAudioSourceProvider>,
    unnamed_audio_source_providers: HashMap<String, DynAudioSourceProvider>
}

impl Plugin {
    fn start_registration(&self, bot: &mut Bot, id: u64) {
        for name in self.named_audio_source_providers.keys() {
            let data = PluginMessageData::RegisterSource(name.clone());
            bot.send(Message::new(id, data)).unwrap();

            // TODO do something useful with result messages
        }

        let data = PluginMessageData::RegistrationFinished;
        bot.send(Message::new(id, data)).unwrap();
    }

    fn handle_can_resolve(&self, bot: &mut Bot, id: u64, code: &str) {
        let mut result = Vec::new();

        for (name, provider) in &self.named_audio_source_providers {
            if provider.can_resolve(code) {
                result.push(name.clone());
            }
        }

        let data = PluginMessageData::Resolution(result);
        bot.send(Message::new(id, data)).unwrap();
    }

    fn setup_source(&self, bot: &mut Bot, id: ConversationId, name: &str, code: &str) {
        let provider =
            self.named_audio_source_providers.get(name)
                .or_else(|| self.unnamed_audio_source_providers.get(name));

        if let Some(provider) = provider {
            match provider.resolve(code) {
                Ok(s) => {
                    let data = PluginMessageData::SetupOk;
                    bot.send(Message::new(id.internal_id(), data)).unwrap();
                    let bot = bot.clone();
                    thread::spawn(move || manage_source(s, bot, id));
                },
                Err(msg) => {
                    let data = PluginMessageData::SetupErr(msg);
                    bot.send(Message::new(id.internal_id(), data)).unwrap();
                }
            }
        }
        else {
            let data =
                PluginMessageData::SetupErr(
                    format!("Unknown audio source type: {}.", name));
            bot.send(Message::new(id.internal_id(), data)).unwrap();
        }
    }

    fn listen(&self, mut bot: Bot) {
        loop {
            let msg = bot.receive_new_blocking();
            let conv_id = msg.conversation_id();
            let int_id = conv_id.internal_id();

            match msg.data() {
                BotMessageData::StartRegistration =>
                    self.start_registration(&mut bot, int_id),
                BotMessageData::CanResolve(code) =>
                    self.handle_can_resolve(&mut bot, int_id, code),
                BotMessageData::SetupSource { name, code } =>
                    self.setup_source(&mut bot, conv_id, name, code),
                _ => {} // should not happen
            }
        }
    }

    async fn launch(self, port: u16) -> io::Result<()> {
        let stream = TcpStream::connect(format!("127.0.0.1:{}", port))?;
        let bot = Bot::new(stream);
        self.listen(bot);
        Ok(())
    }
}

/// A builder which can construct [Plugin]s.
pub struct PluginBuilder {
    plugin: Plugin
}

fn random_string() -> String {
    let mut rng = rand::thread_rng();
    let mut string = String::new();

    for _ in 0..4 {
        let rnum: u64 = rng.gen();
        string.push_str(&format!("{:x}", rnum));
    }

    string
}

impl PluginBuilder {

    /// Creates a plugin builder for a new plugin.
    pub fn new() -> PluginBuilder {
        PluginBuilder {
            plugin: Plugin {
                named_audio_source_providers: HashMap::new(),
                unnamed_audio_source_providers: HashMap::new()
            }
        }
    }

    /// Registers an [AudioSourceProvider] with a name that can be specified by
    /// users to refer to this exact type of audio source. Returns this
    /// instance after the operation for chaining.
    pub fn with_audio_source<N, S, P>(mut self, name: N, provider: P)
        -> PluginBuilder
    where
        N: Into<String>,
        S: AudioSource + Send + 'static,
        P: AudioSourceProvider<S> + 'static
    {
        self.plugin.named_audio_source_providers
            .insert(name.into(), to_dyn(provider));
        self
    }

    /// Registers an [AudioSourceProvider] without a name, i.e. it can only be
    /// resolved automatically. Returns this instance after the operation for
    /// chaining.
    pub fn with_unnamed_audio_source<S, P>(mut self, provider: P)
        -> PluginBuilder
    where
        S: AudioSource + Send + 'static,
        P: AudioSourceProvider<S> + 'static
    {
        // TODO find a cleaner solution than "random_string"
        self.plugin.unnamed_audio_source_providers
            .insert(random_string(), to_dyn(provider));
        self
    }

    /// Builds the plugin with the previously registered information.
    pub fn build(self) -> Plugin {
        self.plugin
    }
}

/// Represents an application which may contain some (or one) [Plugin]s.
pub struct PluginApp {
    plugins: Vec<Plugin>
}

impl PluginApp {

    /// Launches the application, which spawns all registered plugins and
    /// attempts to connect them to a running instance of the Rambot. Panics if
    /// the CLI arguments have not been provided correctly (i.e.
    /// `<executable> <port>`).
    pub async fn launch(self) -> Vec<io::Error> {
        let port = env::args().skip(1).next()
            .expect("Missing port as CLI argument.")
            .parse()
            .expect("Port has invalid format.");
        let mut futures = Vec::new();

        for plugin in self.plugins {
            futures.push(plugin.launch(port));
        }

        let mut result = Vec::new();

        for future in futures {
            match future.await {
                Ok(_) => {},
                Err(e) => result.push(e)
            }
        }

        result
    }
}

/// A builder for [PluginApp]s.
pub struct PluginAppBuilder {
    app: PluginApp
}

impl PluginAppBuilder {

    /// Creates a builder for a new [PluginApp], initially without any plugins.
    pub fn new() -> PluginAppBuilder {
        PluginAppBuilder {
            app: PluginApp {
                plugins: Vec::new()
            }
        }
    }

    /// Registers a plugin with the constructed app. Returns this instance
    /// after the operation for chaining.
    pub fn with_plugin(mut self, plugin: Plugin) -> PluginAppBuilder {
        self.app.plugins.push(plugin);
        self
    }

    /// Builds the plugin app with the previously registered plugins.
    pub fn build(self) -> PluginApp {
        self.app
    }
}
