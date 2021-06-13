//! This module contains functionality to create a [PluginApp], i.e. an
//! application that runs plugins for Rambot.

use crate::audio::{AudioSource, Sample};
use crate::communication::{
    BotMessageData,
    Channel,
    ConnectionIntent,
    ConversationId,
    Message,
    ParseTokenError,
    PluginMessageData,
    Token
};

use rand::Rng;

use std::collections::HashMap;
use std::env;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::marker::PhantomData;
use std::net::TcpStream;
use std::num::ParseIntError;
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
        let audio_source_providers = self.named_audio_source_providers.iter()
            .chain(self.unnamed_audio_source_providers.iter());

        for (name, provider) in audio_source_providers {
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

    async fn listen(&self, mut bot: Bot) {
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
}

/// An enumeration of the errors that may occur when connecting a plugin to the
/// bot.
#[derive(Debug)]
pub enum PluginLaunchError {

    /// Indicates that an IO error occurred while establishing a stream.
    IoError(io::Error),

    /// Indicates that an error occurred while sending the intent.
    CborError(serde_cbor::Error),

    /// Indicates that no port was provided in the CLI arguments.
    MissingPort,

    /// Indicates that no token was provided in the CLI arguments.
    MissingToken,

    /// Indicates that the port could not be parsed correctly.
    InvalidPort(ParseIntError),

    /// Indicates that the token could not be parsed correctly.
    InvalidToken(ParseTokenError)
}

impl From<io::Error> for PluginLaunchError {
    fn from(e: io::Error) -> PluginLaunchError {
        PluginLaunchError::IoError(e)
    }
}

impl From<serde_cbor::Error> for PluginLaunchError {
    fn from(e: serde_cbor::Error) -> PluginLaunchError {
        PluginLaunchError::CborError(e)
    }
}

impl From<ParseIntError> for PluginLaunchError {
    fn from(e: ParseIntError) -> PluginLaunchError {
        PluginLaunchError::InvalidPort(e)
    }
}

impl From<ParseTokenError> for PluginLaunchError {
    fn from(e: ParseTokenError) -> PluginLaunchError {
        PluginLaunchError::InvalidToken(e)
    }
}

impl Display for PluginLaunchError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            PluginLaunchError::IoError(e) =>
                write!(f, "Error while establishing connection: {}", e),
            PluginLaunchError::CborError(e) =>
                write!(f, "Error while sending intent: {}", e),
            PluginLaunchError::MissingPort =>
                write!(f, "Missing port in the CLI arguments."),
            PluginLaunchError::MissingToken =>
                write!(f, "Missing token in the CLI arguments."),
            PluginLaunchError::InvalidPort(e) =>
                write!(f, "Could not parse port: {}", e),
            PluginLaunchError::InvalidToken(e) =>
                write!(f, "Could not parse token: {}", e)
        }
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

    /// Registers an [AudioSourceProvider] with a name that can be specified by
    /// users to refer to this exact type of audio source. Returns this
    /// instance after the operation for chaining. This is a specialization of
    /// [PluginBuilder::with_audio_source], which should preferrably be used if
    /// the provider returns boxed audio sources.
    pub fn with_dyn_audio_source<N, P>(mut self, name: N, provider: P)
        -> PluginBuilder
    where
        N: Into<String>,
        P: AudioSourceProvider<Box<dyn AudioSource + Send>> + 'static
    {
        self.plugin.named_audio_source_providers
            .insert(name.into(), Box::new(provider));
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

    /// Registers an [AudioSourceProvider] without a name, i.e. it can only be
    /// resolved automatically. Returns this instance after the operation for
    /// chaining. This is a specialization of
    /// [PluginBuilder::with_unnamed_audio_source], which should preferrably be
    /// used if the provider returns boxed audio sources.
    pub fn with_unnamed_dyn_audio_source<P>(mut self, provider: P)
        -> PluginBuilder
    where
        P: AudioSourceProvider<Box<dyn AudioSource + Send>> + 'static
    {
        // TODO find a cleaner solution than "random_string"
        self.plugin.unnamed_audio_source_providers
            .insert(random_string(), Box::new(provider));
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

fn connect(port: u16, token: &Token) -> Result<Bot, PluginLaunchError> {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))?;
    serde_cbor::to_writer(&mut stream,
        &ConnectionIntent::RegisterPlugin(token.clone()))?;
    Ok(Bot::new(stream))
}

impl PluginApp {

    /// Launches the application, which spawns all registered plugins and
    /// attempts to connect them to a running instance of the Rambot. Panics if
    /// the CLI arguments have not been provided correctly (i.e.
    /// `<executable> <port>`).
    pub async fn launch(self) -> Result<(), PluginLaunchError> {
        let mut args = env::args().skip(1);
        let port = args.next()
            .ok_or(PluginLaunchError::MissingPort)?
            .parse()?;
        let token = args.next()
            .ok_or(PluginLaunchError::MissingToken)?
            .parse()?;
        let mut futures = Vec::new();

        for plugin in &self.plugins {
            let bot = connect(port, &token)?;
            futures.push(plugin.listen(bot));
        }

        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))?;
        serde_cbor::to_writer(&mut stream,
            &ConnectionIntent::CloseRegistration(token.clone()))?;

        for future in futures {
            future.await;
        }

        Ok(())
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
