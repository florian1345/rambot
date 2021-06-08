//! This module contains functionality to create a [PluginApp], i.e. an
//! application that runs plugins for Rambot.

use crate::audio::AudioSource;
use crate::communication::{
    BotMessageData,
    Channel,
    Message,
    PluginMessageData
};

use std::collections::HashMap;
use std::io;
use std::marker::PhantomData;
use std::net::TcpStream;

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
    S: AudioSource + 'static,
    P: AudioSourceProvider<S>
{
    provider: P,
    source_type: PhantomData<S>
}

impl<S, P> AudioSourceProvider<Box<dyn AudioSource>>
for BoxedAudioSourceProvider<S, P>
where
    S: AudioSource + 'static,
    P: AudioSourceProvider<S>
{
    fn can_resolve(&self, code: &str) -> bool {
        self.provider.can_resolve(code)
    }

    fn resolve(&self, code: &str) -> Result<Box<dyn AudioSource>, String> {
        Ok(Box::new(self.provider.resolve(code)?))
    }
}

type DynAudioSourceProvider =
    Box<dyn AudioSourceProvider<Box<dyn AudioSource>>>;

fn to_dyn<S, P>(provider: P) -> DynAudioSourceProvider
where
    S: AudioSource + 'static,
    P: AudioSourceProvider<S> +  'static
{
    Box::new(BoxedAudioSourceProvider {
        provider,
        source_type: PhantomData
    })
}

type Bot = Channel<PluginMessageData, BotMessageData>;

/// An abstract representation of a plugin that can connect to the bot. As a
/// user, you do not have to interact with this struct beyond registering it
/// with a [PluginApp]. You can construct it with a [PluginBuilder].
pub struct Plugin {
    named_audio_source_providers: HashMap<String, DynAudioSourceProvider>,
    unnamed_audio_source_providers: Vec<DynAudioSourceProvider>
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

    fn listen(&self, mut bot: Bot) {
        loop {
            let msg = bot.receive_new_blocking();
            let id = msg.conversation_id().internal_id();

            match msg.data() {
                BotMessageData::StartRegistration =>
                    self.start_registration(&mut bot, id),
                BotMessageData::CanResolve(_) => {},
                BotMessageData::SetupSource { .. } => {},
                _ => {} // should not happen
            }
        }
    }

    async fn launch(self) -> io::Result<()> {
        let stream = TcpStream::connect("127.0.0.1:46085")?;
        let bot = Bot::new(stream);
        self.listen(bot);
        Ok(())
    }
}

/// A builder which can construct [Plugin]s.
pub struct PluginBuilder {
    plugin: Plugin
}

impl PluginBuilder {

    /// Creates a plugin builder for a new plugin.
    pub fn new() -> PluginBuilder {
        PluginBuilder {
            plugin: Plugin {
                named_audio_source_providers: HashMap::new(),
                unnamed_audio_source_providers: Vec::new()
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
        S: AudioSource + 'static,
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
        S: AudioSource + 'static,
        P: AudioSourceProvider<S> + 'static
    {
        self.plugin.unnamed_audio_source_providers.push(to_dyn(provider));
        self
    }

    /// Builds the plugin with the previously registered information.
    pub fn build(self) -> Plugin {
        self.plugin
    }
}

/// Represents an application which may contain some (or one) [Plugin](s).
pub struct PluginApp {
    plugins: Vec<Plugin>
}

impl PluginApp {

    /// Launches the application, which spawns all registered plugins and
    /// attempts to connect them to a running instance of the Rambot.
    pub async fn launch(self) -> Vec<io::Error> {
        let mut futures = Vec::new();

        for plugin in self.plugins {
            futures.push(plugin.launch());
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
