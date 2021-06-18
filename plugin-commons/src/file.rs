//! This module contains functionality for plugins that load files.

use rambot_api::audio::AudioSource;
use rambot_api::plugin::{
    AudioSourceProvider,
    PluginAppBuilder,
    PluginBuilder,
    PluginLaunchError
};

use serde::{Deserialize, Serialize};

use std::collections::HashSet;
use std::fs::{self, File};
use std::marker::PhantomData;
use std::path::Path;

const CONFIG_FILE_NAME: &str = "config.json";

/// The configuration of a plugin which reads audio from files.
#[derive(Deserialize, Serialize)]
pub struct FilePluginConfig {
    audio_source_name: Option<String>,
    linked_file_extensions: HashSet<String>
}

impl FilePluginConfig {
    fn save(&self) {
        let file = if Path::new(CONFIG_FILE_NAME).exists() {
            File::open(CONFIG_FILE_NAME)
        }
        else {
            File::create(CONFIG_FILE_NAME)
        };

        if let Ok(file) = file {
            serde_json::to_writer(file, &self).unwrap();
        }
    }

    fn load(default: impl Fn() -> FilePluginConfig) -> FilePluginConfig {
        let path = Path::new(CONFIG_FILE_NAME);

        if path.is_dir() {
            return default();
        }
        else if path.is_file() {
            let config_res = fs::read_to_string(path)
                .and_then(|s| serde_json::from_str(&s).map_err(|e| e.into()));

            if let Ok(config) = config_res {
                return config;
            }
        }

        let config = default();
        config.save();
        config
    }
}

/// A builder for [FilePluginConfig]s.
pub struct FilePluginConfigBuilder {
    config: FilePluginConfig
}

impl FilePluginConfigBuilder {

    /// Creates a new file plugin config builder with an initial state of no
    /// audio source name and no linked file extensions.
    pub fn new() -> FilePluginConfigBuilder {
        FilePluginConfigBuilder {
            config: FilePluginConfig {
                audio_source_name: None,
                linked_file_extensions: HashSet::new()
            }
        }
    }

    /// Sets the name of the audio source type provided by this plugin. If this
    /// is not provided, the audio source will only be accessible by automatic
    /// resolution. Otherwise, the user can manually specify the audio source
    /// by its name.
    pub fn with_audio_source_name(mut self,
            audio_source_name: impl Into<String>) -> FilePluginConfigBuilder {
        self.config.audio_source_name = Some(audio_source_name.into());
        self
    }

    /// Registers a new file extension to be automatically resolved to the
    /// audio source provided by this plugin.
    pub fn with_linked_file_extensions(mut self, extension: impl Into<String>)
            -> FilePluginConfigBuilder {
        self.config.linked_file_extensions.insert(extension.into());
        self
    }

    /// Builds the constructed file plugin config.
    pub fn build(self) -> FilePluginConfig {
        self.config
    }
}

/// A trait for structs which can resolve file paths into an audio source of
/// type `S`.
pub trait FileAudioSourceResolver<S: AudioSource> {

    /// Constructs an audio source from the given file path. In case an error
    /// occurs, a message wrapped in an `Err` variant is returned.
    fn resolve(&self, path: &str) -> Result<S, String>;
}

struct FileAudioSourceProvider<S: AudioSource, R: FileAudioSourceResolver<S>> {
    resolver: R,
    linked_file_extensions: HashSet<String>,
    source_type: PhantomData<S>
}

impl<S, R> AudioSourceProvider<S> for FileAudioSourceProvider<S, R>
where
    S: AudioSource,
    R: FileAudioSourceResolver<S>
{
    fn can_resolve(&self, code: &str) -> bool {
        let path = Path::new(code);

        if !path.is_file() {
            return false;
        }

        if let Some(s) = path.extension().and_then(|s| s.to_str()) {
            self.linked_file_extensions.contains(&s.to_lowercase())
        }
        else {
            false
        }
    }

    fn resolve(&self, code: &str) -> Result<S, String> {
        self.resolver.resolve(code)
    }
}

fn prepare_file_plugin<C, S, R>(default_config: C, resolver: R)
    -> (FileAudioSourceProvider<S, R>, Option<String>)
where
    C: Fn() -> FilePluginConfig,
    S: AudioSource + Send + 'static,
    R: FileAudioSourceResolver<S> + 'static
{
    let config = FilePluginConfig::load(default_config);
    let audio_source_name = config.audio_source_name;
    let linked_file_extensions = config.linked_file_extensions;
    let provider = FileAudioSourceProvider {
        resolver,
        linked_file_extensions,
        source_type: PhantomData
    };
    (provider, audio_source_name)
}


/// Launches a plugin application for file resolution. This function is
/// intended for plugins which resolve a single [AudioSource] type. If your
/// plugin returns a trait object, you should use [run_dyn_file_plugin]
/// instead for performance reasons.
pub async fn run_file_plugin<C, S, R>(default_config: C, resolver: R)
    -> Result<(), PluginLaunchError>
where
    C: Fn() -> FilePluginConfig,
    S: AudioSource + Send + 'static,
    R: FileAudioSourceResolver<S> + 'static
{
    let (provider, audio_source_name) =
        prepare_file_plugin(default_config, resolver);
    let mut plugin_builder = PluginBuilder::new();

    if let Some(name) = audio_source_name {
        plugin_builder = plugin_builder.with_audio_source(name, provider);
    }
    else {
        plugin_builder = plugin_builder.with_unnamed_audio_source(provider);
    }

    PluginAppBuilder::new()
        .with_plugin(plugin_builder.build())
        .build().launch().await
}

/// Launches a plugin application for file resolution. This function is
/// intended for plugins which resolve a trait object/polymorphic
/// [AudioSource]. In that case, it will yield slightly better performance than
/// [run_file_plugin].
pub async fn run_dyn_file_plugin<C, R>(default_config: C, resolver: R)
    -> Result<(), PluginLaunchError>
where
    C: Fn() -> FilePluginConfig,
    R: FileAudioSourceResolver<Box<dyn AudioSource + Send>> + 'static
{
    let (provider, audio_source_name) =
        prepare_file_plugin(default_config, resolver);
    let mut plugin_builder = PluginBuilder::new();
    
    if let Some(name) = audio_source_name {
        plugin_builder = plugin_builder.with_dyn_audio_source(name, provider);
    }
    else {
        plugin_builder = plugin_builder
            .with_unnamed_dyn_audio_source(provider);
    }

    PluginAppBuilder::new()
        .with_plugin(plugin_builder.build())
        .build().launch().await
}
