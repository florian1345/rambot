mod audio;
mod documentation;
mod resolver;

pub use audio::{AudioSource, AudioSourceList, Sample};
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

/// Configuration information that is potentially relevant to a specific
/// plugin, but not the bot itself. It is passed to the plugin during
/// initialization. It is the plugin's responsibility to act according to this
/// config.
#[derive(Clone)]
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
    /// accesses, such as searching for audio files to play.
    /// * `allow_web_access`: Indicates whether plugins are allowed to access
    /// the internet.
    /// * `config_path`: The path of the config file that the plugin receiving
    /// this config should use, if it needs one.
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

/// The main trait for Rambot plugins. This handles all initialization and
/// registration of resolvers by this plugin.
pub trait Plugin : Send + Sync {

    /// Initializes this plugin and handles registration of all resolvers that
    /// this plugin provides.
    ///
    /// # Arguments
    ///
    /// * `config`: The [PluginConfig] for this plugins. Currently, plugins
    /// themselves are responsible for respecting this config.
    /// * `registry`: The [ResolverRegistry] to use for registering resolvers
    /// provided by this plugin.
    ///
    /// # Errors
    ///
    /// In case initialization fails, an error message may be provided as a
    /// [String].
    fn load_plugin<'registry>(&self, config: PluginConfig,
        registry: &mut ResolverRegistry<'registry>) -> Result<(), String>;
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
