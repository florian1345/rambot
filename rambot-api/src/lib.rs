use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::env;
use std::io;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

/// A single stereo audio sample in 32-bit float PCM format.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {

    /// The current amplitude on the left channel. Usually on a scale from -1
    /// to 1.
    pub left: f32,

    /// The current amplitude on the right channel. Usually on a scale from -1
    /// to 1.
    pub right: f32
}

impl Sample {

    /// A sample which is zero on both channels.
    pub const ZERO: Sample = Sample {
        left: 0.0,
        right: 0.0
    };
}

impl AddAssign for Sample {
    fn add_assign(&mut self, rhs: Sample) {
        self.left += rhs.left;
        self.right += rhs.right;
    }
}

impl AddAssign<&Sample> for Sample {
    fn add_assign(&mut self, rhs: &Sample) {
        self.left += rhs.left;
        self.right += rhs.right;
    }
}

impl Add for Sample {
    type Output = Sample;

    fn add(mut self, rhs: Sample) -> Sample {
        self += rhs;
        self
    }
}

impl Add<&Sample> for Sample {
    type Output = Sample;

    fn add(mut self, rhs: &Sample) -> Sample {
        self += rhs;
        self
    }
}

impl Add for &Sample {
    type Output = Sample;

    fn add(self, rhs: &Sample) -> Sample {
        *self + rhs
    }
}

impl SubAssign for Sample {
    fn sub_assign(&mut self, rhs: Sample) {
        self.left -= rhs.left;
        self.right -= rhs.right;
    }
}

impl SubAssign<&Sample> for Sample {
    fn sub_assign(&mut self, rhs: &Sample) {
        self.left -= rhs.left;
        self.right -= rhs.right;
    }
}

impl Sub for Sample {
    type Output = Sample;

    fn sub(mut self, rhs: Sample) -> Sample {
        self -= rhs;
        self
    }
}

impl Sub<&Sample> for Sample {
    type Output = Sample;

    fn sub(mut self, rhs: &Sample) -> Sample {
        self -= rhs;
        self
    }
}

impl Sub for &Sample {
    type Output = Sample;

    fn sub(self, rhs: &Sample) -> Sample {
        *self - rhs
    }
}

impl MulAssign<f32> for Sample {
    fn mul_assign(&mut self, rhs: f32) {
        self.left *= rhs;
        self.right *= rhs;
    }
}

impl Mul<f32> for Sample {
    type Output = Sample;

    fn mul(mut self, rhs: f32) -> Sample {
        self *= rhs;
        self
    }
}

impl Mul<f32> for &Sample {
    type Output = Sample;

    fn mul(self, rhs: f32) -> Sample {
        *self * rhs
    }
}

impl DivAssign<f32> for Sample {
    fn div_assign(&mut self, rhs: f32) {
        self.left /= rhs;
        self.right /= rhs;
    }
}

impl Div<f32> for Sample {
    type Output = Sample;

    fn div(mut self, rhs: f32) -> Sample {
        self /= rhs;
        self
    }
}

impl Div<f32> for &Sample {
    type Output = Sample;

    fn div(self, rhs: f32) -> Sample {
        *self / rhs
    }
}

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

/// A trait for types which can read audio data in the form of [Sample]s. The
/// interface is similar to that of the IO [Read](std::io::Read) trait.
pub trait AudioSource {

    /// Reads samples from this source into the given buffer. If the audio
    /// source offers any more data, at least one new sample must be written.
    /// Otherwise, it is assumed that the audio has finished. The buffer does
    /// not need to be filled completely even if there is more audio to come.
    /// The return value indicates how much data was read.
    ///
    /// # Arguments
    ///
    /// * `buf`: A [Sample] buffer to fill with data, starting from index 0.
    /// May or may not be filled with junk.
    ///
    /// # Returns
    ///
    /// The number of samples which were entered into the buffer. That is, this
    /// audio source generated samples from index 0 to one less than this
    /// number (inclusively).
    ///
    /// # Errors
    ///
    /// Any IO-[Error](io::Error) that occurs during reading.
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error>;

    /// Indicates whether this audio source wraps around a child source. This
    /// must be `true` for any audio source constituting an effect, i.e. which
    /// was resolved by an [EffectResolver]. For example, a low-pass filter
    /// wraps around the root audio source which is filtered.
    fn has_child(&self) -> bool;

    /// Removes the child from this audio source and returns it. If
    /// [AudioSource::has_child] returns `true`, this must return a valid audio
    /// source, otherwise it may panic.
    ///
    /// Unless this method is called outside the framework, it is guaranteed
    /// that the audio source is dropped immediately afterwards. It is
    /// therefore not necessary to keep it in a usable state.
    fn take_child(&mut self) -> Box<dyn AudioSource + Send>;
}

impl AudioSource for Box<dyn AudioSource + Send> {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        self.as_mut().read(buf)
    }

    fn has_child(&self) -> bool {
        self.as_ref().has_child()
    }

    fn take_child(&mut self) -> Box<dyn AudioSource + Send> {
        self.as_mut().take_child()
    }
}

/// A trait for types which can offer a list or enumeration of descriptors,
/// such as a playlist or loop functionality.
pub trait AudioSourceList {

    /// Gets the next descriptor in the list, or `None` if the list is
    /// finished. May return an IO-[Error](io::Error) if the operation fails.
    fn next(&mut self) -> Result<Option<String>, io::Error>;
}

/// A trait for resolvers which can create [AudioSource]s from string
/// descriptors. A plugin with the purpose of creating new ways of generating
/// audio to play with the bot usually registers at least one of these. As an
/// example, a plugin may register a resolver for WAV files. The resolver takes
/// as descriptors paths to WAV files and generates audio sources which decode
/// and stream those files.
pub trait AudioSourceResolver : Send + Sync {

    /// Indicates whether this resolver can construct an audio source from the
    /// given descriptor.
    ///
    /// # Arguments
    ///
    /// * `descriptor`: A textual descriptor of unspecified format.
    ///
    /// # Returns
    ///
    /// True, if and only if this resolver can construct an audio source from
    /// the given descriptor.
    fn can_resolve(&self, descriptor: &str) -> bool;

    /// Generates an [AudioSource] trait object from the given descriptor. If
    /// [AudioSourceResolver::can_resolve] returns `true`, this should probably
    /// work, however it may still return an error message should an unexpected
    /// problem occur.
    ///
    /// As an example, for a plugin that reads files of some type,
    /// [AudioSourceResolver::can_resolve] may be implemented by checking that
    /// a file exists and has the correct extension. Now it should probably
    /// work to load it, but the file format may still be corrupted, which
    /// would cause an error in this method.
    ///
    /// # Arguments
    ///
    /// * `descriptor`: A textual descriptor of unspecified format.
    ///
    /// # Returns
    ///
    /// A boxed [AudioSource] playing the audio represented by the given
    /// descriptor.
    ///
    /// # Errors
    ///
    /// An error message provided as a [String] in case resolution fails.
    fn resolve(&self, descriptor: &str)
        -> Result<Box<dyn AudioSource + Send>, String>;
}

/// A trait for resolvers which can create effects from key-value arguments.
/// Similarly to [AudioSourceResolver]s, these effects are realized as
/// [AudioSource]s, however they receive a child audio source whose output can
/// be modified, thus constituting an effect. As an example, a volume effect
/// could be realized by wrapping the child audio source and multiplying all
/// audio data it outputs by the volume number.
pub trait EffectResolver : Send + Sync {

    /// The name of the kind of effects resolved by this resolver.
    fn name(&self) -> &str;

    /// Indicates whether effects of this kind are unique, i.e. there may exist
    /// at most one per layer. When another effect of the same kind is added,
    /// the old one is removed. This makes sense for example for a volume
    /// effect, where adding volume effects can be seen more like an "update".
    fn unique(&self) -> bool;

    /// Generates an [AudioSource] trait object that yields audio constituting
    /// the effect defined by the given key-value pairs applied to the given
    /// child. This may return an error should the provided key-value map
    /// contain invalid inputs.
    ///
    /// # Arguments
    ///
    /// * `key_values`: A [HashMap] storing arguments for this effect. For each
    /// supplied argument, the parameter name maps to the string that was given
    /// as the argument value.
    /// * `child`: A boxed [AudioSource] to which the effect shall be applied,
    /// i.e. which should be wrapped in an effect audio source.
    ///
    /// # Returns
    ///
    /// A boxed [AudioSource] playing the same audio as `child` but with the
    /// effect applied to it. It must also offer `child` as a child in the
    /// context of [AudioSource::has_child] and [AudioSource::take_child].
    ///
    /// # Errors
    ///
    /// An error message provided as a [String] in case resolution fails.
    fn resolve(&self, key_values: &HashMap<String, String>,
        child: Box<dyn AudioSource + Send>)
        -> Result<Box<dyn AudioSource + Send>, String>;
}

/// A trait for resolvers which can create [AudioSourceList]s from string
/// descriptors. A plugin with the purpose of implementing new kinds of
/// playlists will usually register at least one of these.
pub trait AudioSourceListResolver : Send + Sync {

    /// Indicates whether this resolver can construct an audio source list from
    /// the given descriptor.
    ///
    /// # Arguments
    ///
    /// * `descriptor`: A textual descriptor of unspecified format.
    ///
    /// # Returns
    ///
    /// True, if and only if this resolver can construct an audio source list
    /// from the given descriptor.
    fn can_resolve(&self, descriptor: &str) -> bool;

    /// Generates an [AudioSourceList] trait object from the given descriptor.
    /// If [AudioSourceListResolver::can_resolve] returns `true`, this should
    /// probably work, however it may still return an error message should an
    /// unexpected problem occur.
    ///
    /// As an example, for a plugin that reads files of some type,
    /// [AudioSourceListResolver::can_resolve] may be implemented by checking
    /// that a file exists and has the correct extension. Now it should
    /// probably work to load it, but the file format may still be corrupted,
    /// which would cause an error in this method.
    ///
    /// # Arguments
    ///
    /// * `descriptor`: A textual descriptor of unspecified format.
    ///
    /// # Returns
    ///
    /// A boxed [AudioSourceList] providing the playlist represented by the
    /// given `descriptor`.
    ///
    /// # Errors
    ///
    /// An error message provided as a [String] in case resolution fails.
    fn resolve(&self, descriptor: &str)
        -> Result<Box<dyn AudioSourceList + Send>, String>;
}

/// A trait for resolvers which can create adapters from key-value arguments.
/// Adapters are essentially effects for [AudioSourceList]s. Similarly to
/// effects, they are realized as [AudioSourceList]s wrapping other audio
/// source lists and altering their output. As an example, a shuffle effect
/// could be realized by wrapping the child audio source list, collecting all
/// its content, shuffling it, and then iterating over it.
pub trait AdapterResolver : Send + Sync {

    /// The name of the kind of adapters resolved by this resolver.
    fn name(&self) -> &str;

    /// Indicates whether adapters of this kind are unique, i.e. there may
    /// exist at most one per layer. When another adapter of the same kind is
    /// added, the old one is removed. This makes sense for example for a loop
    /// adapter, because looping an already infinite (because looped) audio
    /// source list is redundant.
    fn unique(&self) -> bool;

    /// Generates an [AudioSourceList] trait object that yields audio source
    /// descriptors constituting the output of the adapter defined by the given
    /// key-value pairs applied to the given child. This may return an error
    /// should the provided key-value map contain invalid inputs.
    ///
    /// # Arguments
    ///
    /// * `key_values`: A [HashMap] storing arguments for this adapter. For
    /// each supplied argument, the parameter name maps to the string that was
    /// given as the argument value.
    /// * `child`: A boxed [AudioSourceList] to which the adapter shall be
    /// applied, i.e. which should be wrapped in an adapter audio source list.
    ///
    /// # Returns
    ///
    /// A boxed [AudioSourceList] which provides the output of the adapter
    /// applied to `child`.
    ///
    /// # Errors
    ///
    /// An error message provided as a [String] in case resolution fails.
    fn resolve(&self, key_values: &HashMap<String, String>,
        child: Box<dyn AudioSourceList + Send>)
        -> Result<Box<dyn AudioSourceList + Send>, String>;
}

/// The main trait for Rambot plugins. This handles all initialization and
/// registration of resolvers by this plugin.
pub trait Plugin : Send + Sync {

    /// Initializes this plugin and allows it to view the config valid for all
    /// plugins.
    ///
    /// # Arguments
    ///
    /// * `config`: The [PluginConfig] for all plugins. Currently, plugins
    /// themselves are responsible for respecting this config.
    ///
    /// # Errors
    ///
    /// In case initialization fails, an error message may be provided as a
    /// [String].
    fn load_plugin(&mut self, config: &PluginConfig) -> Result<(), String>;

    /// Gets a list of all [AudioSourceResolver] to be registered by this
    /// plugin.
    ///
    /// # Returns
    ///
    /// A new vector of [AudioSourceResolver] trait objects for all audio
    /// source resolvers this plugin wishes to register.
    fn audio_source_resolvers(&self) -> Vec<Box<dyn AudioSourceResolver>>;

    /// Gets a list of all [EffectResolver] to be registered by this plugin.
    ///
    /// # Returns
    ///
    /// A new vector of [EffectResolver] trait objects for all effect resolvers
    /// this plugin wishes to register.
    fn effect_resolvers(&self) -> Vec<Box<dyn EffectResolver>>;

    /// Gets a list of all [AudioSourceListResolver] to be registered by this
    /// plugin.
    ///
    /// # Returns
    ///
    /// A new vector of [AudioSourceListResolver] trait objects for all audio
    /// source list resolvers this plugin wishes to register.
    fn audio_source_list_resolvers(&self)
        -> Vec<Box<dyn AudioSourceListResolver>>;

    /// Gets a list of all [AdapterResolver] to be registered by this plugin.
    ///
    /// # Returns
    ///
    /// A new vector of [AdapterResolver] trait objects for all adapter
    /// resolvers this plugin wishes to register.
    fn adapter_resolvers(&self) -> Vec<Box<dyn AdapterResolver>>;
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
