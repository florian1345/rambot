use serde::{Deserialize, Serialize};

use std::env;
use std::io;

mod audio;
mod resolver;

pub use audio::{AudioSource, AudioSourceList, Sample};
pub use resolver::{
    AdapterResolver,
    AudioSourceListResolver,
    AudioSourceResolver,
    EffectResolver,
    ResolveEffectError,
    ResolverRegistry
};

const DEFAULT_ALLOW_WEB_ACCESS: bool = true;

/// Configuration information that is potentially relevant to plugins, but not
/// the bot itself. It is passed to the plugins during initialization. It is
/// their responsibility to act according to this config.
#[derive(Clone, Deserialize, Serialize)]
pub struct PluginConfig {
    root_directory: String,
    allow_web_access: bool
}

impl PluginConfig {

    /// Creates a new, default plugin config. The root directory is initialized
    /// to the current working directory. Getting this may fail, hence this
    /// method may return an IO-[Error](io::Error).
    pub fn default() -> Result<PluginConfig, io::Error> {
        let root_directory = env::current_dir()?
            .as_os_str()
            .to_str()
            .unwrap()
            .to_owned();

        Ok(PluginConfig {
            root_directory,
            allow_web_access: DEFAULT_ALLOW_WEB_ACCESS 
        })
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
}

/// The main trait for Rambot plugins. This handles all initialization and
/// registration of resolvers by this plugin.
pub trait Plugin : Send + Sync {

    /// Initializes this plugin and handles registration of all resolvers that
    /// this plugin provides.
    ///
    /// # Arguments
    ///
    /// * `config`: The [PluginConfig] for all plugins. Currently, plugins
    /// themselves are responsible for respecting this config.
    /// * `registry`: The [ResolverRegistry] to use for registering resolvers
    /// provided by this plugin.
    ///
    /// # Errors
    ///
    /// In case initialization fails, an error message may be provided as a
    /// [String].
    fn load_plugin<'registry>(&mut self, config: &PluginConfig,
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
        pub extern "Rust" fn _create_plugin() -> *mut dyn $crate::Plugin {
            let plugin = $constructor();
            let boxed: Box<dyn $crate::Plugin> = Box::new(plugin);

            Box::into_raw(boxed)
        }
    }
}
