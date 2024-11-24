//! This crate defines the API against which plugins for the Rambot are
//! programmed. Any plugin must implement the [Plugin] trait. The most
//! important method there is [Plugin::load_plugin], where a plugin can
//! register all functionality it provides with the bot.
//!
//! ```ignore
//! use rambot_api::{Plugin, PluginConfig, ResolverRegistry};
//!
//! struct MyPlugin;
//!
//! impl Plugin for MyPlugin {
//!     fn load_plugin(&self, config: PluginConfig,
//!             registry: &mut ResolverRegistry<'_>) -> Result<(), String> {
//!         // Here we do all registration by calling the appropriate methods
//!         // on "registry". We can register an arbitrary amount of
//!         // functionality. The parameter `config` provides some extra
//!         // configuration data assigned to this plugin by the bot.
//! 
//!         registry.register_audio_source_resolver(...);
//!         registry.register_audio_source_list_resolver(...);
//!         registry.register_effect_resolver(...);
//!         registry.register_adapter_resolver(...);
//! 
//!         // If registration was successful, return Ok(()), otherwise return
//!         // Err(...) with an error message, which will be logged by the bot
//!         // and cause startup to fail.
//! 
//!         Ok(())
//!     }
//! 
//!     fn unload_plugin(&self) {
//!         // This function is run when the bot's plugin manager is dropped.
//!         // Here you can do any cleanup required by the plugin, such as
//!         // closing any IO resources.
//!     }
//! }
//! ```
//! 
//! There are currently four different kinds of functionality a plugin can
//! provide for the bot. Check out their respective documentation for more
//! information and examples.
//!
//! * [AudioSourceResolver]s are the most essential feature, where a plugin
//!   provides a way to play some new kind of audio. An example would be
//!   playback of a certain type of audio file, such as MP3.
//! * [AudioSourceListResolver]s offer a way to resolve playlists. An example
//!   would be playback of all audio files in a directory.
//! * [EffectResolver]s transform one audio stream into another which depends
//!   on the former, applying some kind of audio effect. An example would be
//!   changing the volume of audio.
//! * [AdapterResolver]s transform one playlist into another which depends on
//!   the former, changing the order and/or content. An example would be
//!   shuffling a playlist.

mod audio;
mod documentation;
mod resolver;
mod time;

pub use audio::{
    AudioMetadata,
    AudioMetadataBuilder,
    AudioSource,
    AudioSourceList,
    Sample,
    SeekError
};
pub use documentation::{
    AudioDocumentation,
    AudioDocumentationBuilder,
    ModifierDocumentation,
    ModifierDocumentationBuilder
};
pub use resolver::{
    AdapterResolver,
    AudioSourceListResolver,
    AudioSourceResolver,
    EffectResolver,
    ResolveEffectError,
    ResolverRegistry
};
pub use time::{
    ParseSampleDurationError,
    SampleDuration,
    SampleDurationError,
    SampleDurationResult,
    SAMPLES_PER_HOUR,
    SAMPLES_PER_MILLISECOND,
    SAMPLES_PER_MINUTE,
    SAMPLES_PER_SECOND
};

/// Configuration information that is potentially relevant to a specific
/// plugin, but not the bot itself. It is passed to the plugin during
/// initialization. It is the plugin's responsibility to act according to this
/// config.
#[derive(Clone, Debug)]
pub struct PluginConfig {
    root_directory: String,
    allow_web_access: bool,
    config_path: String
}

impl PluginConfig {

    /// Creates a new plugin config from the given information.
    ///
    /// # Arguments
    ///
    /// * `root_directory`: The path to take as the root for file system
    ///   accesses, such as searching for audio files to play.
    /// * `allow_web_access`: Indicates whether plugins are allowed to access
    ///   the internet.
    /// * `config_path`: The path of the config file that the plugin receiving
    ///   this config should use, if it needs one.
    pub fn new<S1, S2>(root_directory: S1, allow_web_access: bool,
        config_path: S2) -> PluginConfig
    where
        S1: Into<String>,
        S2: Into<String>
    {
        PluginConfig {
            root_directory: root_directory.into(),
            allow_web_access,
            config_path: config_path.into() 
        }
    }

    /// The path of the directory to use as a root for file system operations,
    /// such as opening audio files. All paths should be interpreted relative
    /// to this root and no files outside this directory should be read, for
    /// security reasons.
    pub fn root_directory(&self) -> &str {
        &self.root_directory
    }

    /// Indicates whether plugins should access the internet. Currently, it is
    /// the plugins' responsibility to enforce this constraint.
    pub fn allow_web_access(&self) -> bool {
        self.allow_web_access
    }

    /// The path of the config file that the plugin receiving this config
    /// should use, if it needs one. The file is not guaranteed to exist,
    /// however its parent directory has been created by the bot if it did not
    /// exist.
    pub fn config_path(&self) -> &str {
        &self.config_path
    }
}

/// Guild-specific configuration provided to a plugin's resolvers.
#[derive(Clone, Debug, Default)]
pub struct PluginGuildConfig {
    root_directory: Option<String>
}

impl PluginGuildConfig {

    /// Creates a new plugin guild config from the given data.
    ///
    /// # Arguments
    ///
    /// * `root_directory`: The guild-specific root directory or `None` if the
    ///   global root directory should be used.
    pub fn new<S>(root_directory: Option<S>) -> PluginGuildConfig
    where
        S: Into<String>
    {
        PluginGuildConfig {
            root_directory: root_directory.map(|s| s.into())
        }
    }

    /// Gets the guild-specific root directory to use for file system accesses.
    /// If present, this overrides the global root directory, which should be
    /// used if this method returns `None`.
    pub fn root_directory(&self) -> Option<&String> {
        self.root_directory.as_ref()
    }
}

/// The main trait for Rambot plugins. This handles all initialization and
/// registration of resolvers by this plugin.
pub trait Plugin : Send + Sync {

    /// Initializes this plugin and handles registration of all resolvers that
    /// this plugin provides.
    ///
    /// # Arguments
    ///
    /// * `config`: The [PluginConfig] for this plugins. Currently, plugins
    ///   themselves are responsible for respecting this config.
    /// * `registry`: The [ResolverRegistry] to use for registering resolvers
    ///   provided by this plugin.
    ///
    /// # Errors
    ///
    /// In case initialization fails, an error message may be provided as a
    /// [String].
    fn load_plugin(&self, config: PluginConfig,
        registry: &mut ResolverRegistry<'_>) -> Result<(), String>;

    /// This function is called when the plugin is unloaded, i.e. the bot's
    /// plugin manager is dropped. Any cleanup of the plugin's operation should
    /// be done here. By default, it does nothing.
    fn unload_plugin(&self) { }
}

/// Exports this plugin by creating a common entry point for dynamically loaded
/// libraries that returns a pointer to a [Plugin] trait object. As an
/// argument, this macro requires the path to a function which can be called
/// without arguments and returns an instance of any type implementing
/// [Plugin].
///
/// # Example
///
/// ```ignore
/// use rambot_api::{export_plugin, Plugin};
///
/// struct MyPlugin { [...] }
///
/// impl Plugin for MyPlugin { [...] }
///
/// fn make_my_plugin() -> MyPlugin {
///     MyPlugin { [...] }
/// }
///
/// export_plugin!(make_my_plugin);
/// ```
#[macro_export]
macro_rules! export_plugin {
    ($constructor:path) => {
        #[no_mangle]
        pub extern "Rust" fn _create_plugin() -> *mut Box<dyn $crate::Plugin> {
            let plugin = $constructor();
            let boxed: Box<Box<dyn $crate::Plugin>> =
                Box::new(Box::new(plugin));

            Box::into_raw(boxed)
        }
    }
}
