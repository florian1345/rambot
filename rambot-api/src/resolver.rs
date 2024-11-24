use crate::{AudioDocumentation, PluginGuildConfig};
use crate::audio::{AudioSource, AudioSourceList};
use crate::documentation::ModifierDocumentation;

use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};

/// A trait for resolvers which can create [AudioSource]s from string
/// descriptors. A plugin with the purpose of creating new ways of generating
/// audio to play with the bot usually registers at least one of these. As an
/// example, a plugin may register a resolver for WAV files. The resolver takes
/// as descriptors paths to WAV files and generates audio sources which decode
/// and stream those files.
///
/// The toy example provided below shows resolution of a sine wave with a
/// certain frequency.
///
/// ```
/// use rambot_api::{
///     AudioDocumentation,
///     AudioDocumentationBuilder,
///     AudioMetadata,
///     AudioMetadataBuilder,
///     AudioSource,
///     AudioSourceResolver,
///     PluginGuildConfig,
///     Sample
/// };
/// 
/// use regex::Regex;
/// 
/// use std::f32::consts;
/// use std::io;
/// 
/// // This is the audio source resolved by our resolver.
/// struct SineAudioSource {
///     state: f32,
///     step: f32
/// }
/// 
/// // We have to implement the AudioSource trait first.
/// impl AudioSource for SineAudioSource {
///     fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
///         // Simply fill the entire buffer with a sine wave.
///         for sample in buf.iter_mut() {
///             let value = self.state.sin();
///
///             *sample = Sample {
///                 left: value,
///                 right: value
///             };
/// 
///             self.state = (self.state + self.step) % consts::TAU;
///         }
/// 
///         // As we filled the entire buffer, we return its full length.
///         Ok(buf.len())
///     }
/// 
///     fn has_child(&self) -> bool {
///         // This method indicates whether this AudioSource is an effect or a
///         // "root" audio source. Effects are realized as wrappers of other
///         // audio sources, so they have a "child". Since the SineAudioSource
///         // is not an effect, we have no child.
///         false
///     }
///
///     fn take_child(&mut self) -> Box<dyn AudioSource + Send + Sync> {
///         // As discussed above, we have no child => panic.
///         panic!("cannot take child from sine audio source")
///     }
///
///     fn metadata(&self) -> AudioMetadata {
///         // Usually, you would try to obtain the metadata from a file,
///         // website, or wherever you take your audio from. Here, we just
///         // give a generic title.
///         AudioMetadataBuilder::new()
///             .with_title("Sine Wave")
///             .build()
///     }
/// }
/// 
/// fn sine_regex() -> Regex {
///     Regex::new(r"^sine:([0-9]+(?:\.[0-9]+)?)$").unwrap()
/// }
/// 
/// // This is the AudioSourceResolver that we will register with the bot in
/// // the initialization of our plugin.
/// struct SineAudioSourceResolver;
/// 
/// impl AudioSourceResolver for SineAudioSourceResolver {
///     fn documentation(&self) -> AudioDocumentation {
///         // Here we have to construct a documentation to be displayed to the
///         // user when they ask for it.
///         AudioDocumentationBuilder::new()
///             .with_name("sine")
///             .with_summary("Plays a sine wave at the given frequency. \
///                 Format: `sine:<frequency>`")
///             .build().unwrap()
///     }
/// 
///     fn can_resolve(&self, descriptor: &str, _: PluginGuildConfig) -> bool {
///         // In this function, we get a user-provided audio descriptor and
///         // have to determine whether this resolver can build an audio
///         // source from it.
///         sine_regex().is_match(descriptor)
///     }
/// 
///     fn resolve(&self, descriptor: &str, _: PluginGuildConfig)
///             -> Result<Box<dyn AudioSource + Send + Sync>, String> {
///         // Here we actually have to construct the audio source from the
///         // descriptor. We can rely on "can_resolve" to be true for the
///         // given descriptor, as the bot will not query this method
///         // otherwise. If for some reason resolution still fails, we can
///         // return an error message.
///         let frequency: f32 = sine_regex().captures(descriptor)
///             .ok_or_else(|| "Descriptor has invalid format.".to_owned())?
///             .get(1).unwrap().as_str().parse().unwrap();
///         let step = frequency / 48000.0 * consts::TAU;
/// 
///         Ok(Box::new(SineAudioSource {
///             state: 0.0,
///             step
///         }))
///     }
/// }
/// ```
pub trait AudioSourceResolver : Send + Sync {

    /// Constructs an [AudioDocumentation] for this kind of audio source. This
    /// is displayed when executing the audio command.
    fn documentation(&self) -> AudioDocumentation;

    /// Indicates whether this resolver can construct an audio source from the
    /// given descriptor.
    ///
    /// # Arguments
    ///
    /// * `descriptor`: A textual descriptor of unspecified format.
    /// * `guild_config`: A [PluginGuildConfig] containing guild-specific
    ///   information that may be relevant to the resolution.
    ///
    /// # Returns
    ///
    /// True, if and only if this resolver can construct an audio source from
    /// the given descriptor.
    fn can_resolve(&self, descriptor: &str, guild_config: PluginGuildConfig)
        -> bool;

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
    /// * `guild_config`: A [PluginGuildConfig] containing guild-specific
    ///   information that may be relevant to the resolution.
    ///
    /// # Returns
    ///
    /// A boxed [AudioSource] playing the audio represented by the given
    /// descriptor.
    ///
    /// # Errors
    ///
    /// An error message provided as a [String] in case resolution fails.
    fn resolve(&self, descriptor: &str, guild_config: PluginGuildConfig)
        -> Result<Box<dyn AudioSource + Send + Sync>, String>;
}

/// An error that occurs when a plugin attempts to resolve an effect, i.e.
/// [EffectResolver::resolve] is called. In addition to the ordinary message
/// provided with plugin errors, this type also contains the child to which the
/// effect was supposed to be applied. This allows the plugin to return the
/// unmodified child and therefore the bot can restore the audio after a failed
/// resolution.
pub struct ResolveEffectError {
    message: String,
    child: Box<dyn AudioSource + Send + Sync>
}

impl ResolveEffectError {

    /// Creates a new resolve effect error from message and child.
    ///
    /// # Arguments
    ///
    /// * `message`: The error message to be displayed to the user.
    /// * `child`: The child audio source to which the effect should have been
    ///   applied.
    pub fn new<S>(message: S, child: Box<dyn AudioSource + Send + Sync>)
        -> ResolveEffectError
    where
        S: Into<String>
    {
        ResolveEffectError {
            message: message.into(),
            child
        }
    }

    /// Destructures this resolve effect error into its raw parts, i.e. the
    /// error message as the first and intended child as the second part.
    pub fn into_parts(self) -> (String, Box<dyn AudioSource + Send + Sync>) {
        (self.message, self.child)
    }
}

impl Display for ResolveEffectError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.message)
    }
}

/// A trait for resolvers which can create effects from key-value arguments.
/// Similarly to [AudioSourceResolver]s, these effects are realized as
/// [AudioSource]s, however they receive a child audio source whose output can
/// be modified, thus constituting an effect. As an example, a volume effect
/// could be realized by wrapping the child audio source and multiplying all
/// audio data it outputs by the volume number.
///
/// The example below is a slightly simplified and documented version of the
/// actual `plugin-volume` crate.
///
/// ```
/// use rambot_api::{
///     AudioMetadata,
///     AudioSource,
///     EffectResolver,
///     ModifierDocumentation,
///     ModifierDocumentationBuilder,
///     PluginGuildConfig,
///     ResolveEffectError,
///     Sample
/// };
/// 
/// use std::collections::HashMap;
/// use std::io;
/// 
/// // This is the effect that actually applies the volume. It wraps a child
/// // effect and implements AudioSource itself, providing the altered audio.
/// struct VolumeEffect {
///     // Because we need to be able to remove the child from the effect, we
///     // wrap it in an Option so we can move out of it later.
///     child: Option<Box<dyn AudioSource + Send + Sync>>,
///     volume: f32
/// }
/// 
/// impl AudioSource for VolumeEffect {
///     fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
///         // Let the child read into the buffer and multiply all samples by
///         // our volume factor. As the child cannot be taken before this, we
///         // can rely on it still being present.
///         let count = self.child.as_mut().unwrap().read(buf)?;
///     
///         for sample in buf.iter_mut().take(count) {
///             *sample *= self.volume;
///         }
///     
///         Ok(count)
///     }
///     
///     fn has_child(&self) -> bool {
///         // This method indicates whether this AudioSource is an effect or a
///         // "root" audio source. Since the VolumeEffect s an effect, we do
///         // have a child.
///         true
///     }
///     
///     fn take_child(&mut self) -> Box<dyn AudioSource + Send + Sync> {
///         // Remove and return the child. We can rely on this being the last
///         // call made to the effect, so it does not matter that the child
///         // will no longer be present after.
///         self.child.take().unwrap()
///     }
/// 
///     fn metadata(&self) -> AudioMetadata {
///         // As an effect, you usually want to just forward your child's
///         // metadata.
///         self.child.as_ref().unwrap().metadata()
///     }
/// }
/// 
/// // Helper method that extracts the "volume" parameter from key-values.
/// fn get_volume(key_values: &HashMap<String, String>)
///         -> Result<f32, String> {
///     // Naming the parameter the same as the effect (see
///     // VolumeEffectResolver::name) allows the abbreviation volume=[...].
///     key_values.get("volume")
///         .map(|v| v.parse::<f32>().map_err(|e| format!("{}", e)))
///         .unwrap_or_else(|| Err("Missing \"volume\" key.".to_owned()))
/// }
/// 
/// // This is the EffectResolver that we will register with the bot in the
/// // initialization of our plugin.
/// struct VolumeEffectResolver;
/// 
/// impl EffectResolver for VolumeEffectResolver {
///     fn name(&self) -> &str {
///         // Plugin-provided effects are specified by the user in the format
///         // name(key=value,...). This format is parsed already by the bot.
///         // Hence, we need to specify what the name-part is.
///         "volume"
///     }
/// 
///     fn unique(&self) -> bool {
///         // Returning true here instructs the bot to remove any old volume
///         // effect when a new one is added. This is more intuitive for
///         // controlling the volume, as it allows setting the volume rather
///         // than multiplying it.
///         true
///     }
///     
///     fn documentation(&self) -> ModifierDocumentation {
///         // Here we have to construct a documentation to be displayed to the
///         // user when they ask for it.
///         ModifierDocumentationBuilder::new()
///             .with_short_summary("Controls the volume.")
///             .with_parameter("volume", "The volume.")
///             .build().unwrap()
///     }
///     
///     fn resolve(&self, key_values: &HashMap<String, String>,
///             child: Box<dyn AudioSource + Send + Sync>, _: PluginGuildConfig)
///             -> Result<Box<dyn AudioSource + Send + Sync>, ResolveEffectError> {
///         // Here we actually have to construct the effect from the
///         // key-value-pairs parsed by the bot. We get the child to which we
///         // are supposed to apply the effect.
///         let volume = match get_volume(key_values) {
///             Ok(v) => v,
///             // If resolution fails, we have to give the original child back
///             // so it can be restored.
///             Err(msg) => return Err(ResolveEffectError::new(msg, child))
///         };
///     
///         Ok(Box::new(VolumeEffect {
///             child: Some(child),
///             volume
///         }))
///     }
/// }
/// ```
pub trait EffectResolver : Send + Sync {

    /// The name of the kind of effects resolved by this resolver.
    fn name(&self) -> &str;

    /// Indicates whether effects of this kind are unique, i.e. there may exist
    /// at most one per layer. When another effect of the same kind is added,
    /// the old one is removed. This makes sense for example for a volume
    /// effect, where adding volume effects can be seen more like an "update".
    fn unique(&self) -> bool;

    /// Constructs a [ModifierDocumentation] for this kind of effect. This is
    /// displayed when executing the effect help command.
    fn documentation(&self) -> ModifierDocumentation;

    /// Generates an [AudioSource] trait object that yields audio constituting
    /// the effect defined by the given key-value pairs applied to the given
    /// child. This may return an error should the provided key-value map
    /// contain invalid inputs.
    ///
    /// # Arguments
    ///
    /// * `key_values`: A [HashMap] storing arguments for this effect. For each
    ///   supplied argument, the parameter name maps to the string that was
    ///   given as the argument value.
    /// * `child`: A boxed [AudioSource] to which the effect shall be applied,
    ///   i.e. which should be wrapped in an effect audio source.
    /// * `guild_config`: A [PluginGuildConfig] containing guild-specific
    ///   information that may be relevant to the resolution.
    ///
    /// # Returns
    ///
    /// A boxed [AudioSource] playing the same audio as `child` but with the
    /// effect applied to it. It must also offer `child` as a child in the
    /// context of [AudioSource::has_child] and [AudioSource::take_child].
    ///
    /// # Errors
    ///
    /// A [ResolveEffectError] containing an error message as well as the given
    /// `child` in case resolution fails.
    fn resolve(&self, key_values: &HashMap<String, String>,
        child: Box<dyn AudioSource + Send + Sync>, guild_config: PluginGuildConfig)
        -> Result<Box<dyn AudioSource + Send + Sync>, ResolveEffectError>;
}

/// A trait for resolvers which can create [AudioSourceList]s from string
/// descriptors. A plugin with the purpose of implementing new kinds of
/// playlists will usually register at least one of these.
///
/// The example below demonstrates an audio source list resolver that takes a
/// comma-separated list of descriptors and returns them as a list.
/// 
/// ```
/// use rambot_api::{
///     AudioDocumentation,
///     AudioDocumentationBuilder,
///     AudioSourceList,
///     AudioSourceListResolver,
///     PluginGuildConfig
/// };
/// 
/// use std::io;
/// use std::vec::IntoIter;
/// 
/// // The audio source list type that is resolved by our resolver.
/// struct CommaSeparatedList {
///     entries: IntoIter<String>
/// }
/// 
/// impl AudioSourceList for CommaSeparatedList {
///     fn next(&mut self) -> Result<Option<String>, io::Error> {
///         // Return the next item in the list. As the interface is almost
///         // identical to iterators, we just need to wrap it in Ok(...). For
///         // plugins where this is fallible, an error can be returned.
///         Ok(self.entries.next())
///     }
/// }
/// 
/// // Helper function that splits a descriptor into parts.
/// fn resolve_list(descriptor: &str) -> Option<Vec<String>> {
///     let vec = descriptor.split(',')
///         .map(|s| s.to_owned())
///         .collect::<Vec<_>>();
/// 
///     // We do not want this plugin to match every descriptor, so we only
///     // consider those composed of at least two parts.
///     if vec.len() > 1 {
///         Some(vec)
///     }
///     else {
///         None
///     }
/// }
/// 
/// // This is the AudioSourceListResolver that we will register with the bot
/// // in the initialization of our plugin.
/// struct CommaSeparatedListResolver;
/// 
/// impl AudioSourceListResolver for CommaSeparatedListResolver {
///     fn documentation(&self) -> AudioDocumentation {
///         // Here we have to construct a documentation to be displayed to the
///         // user when they ask for it. As the separation between lists and
///         // ordinary audio-sources is opaque to the user, this will be
///         // displayed together with all other lists and audio sources.
///         AudioDocumentationBuilder::new()
///             .with_name("Comma Separated Playlist")
///             .with_summary("Given a comma-separated list of descriptors, \
///                 yields them individually as parts of a playlist.")
///             .build().unwrap()
///     }
///     
///     fn can_resolve(&self, descriptor: &str, _: PluginGuildConfig) -> bool {
///         // As with AudioSourceResolvers, we get a user-provided audio
///         // descriptor and have to determine whether this resolver can build
///         // an audio source list from it.
///         resolve_list(descriptor).is_some()
///     }
///     
///     fn resolve(&self, descriptor: &str, _: PluginGuildConfig)
///             -> Result<Box<dyn AudioSourceList + Send + Sync>, String> {
///         // As with AudioSourceResolvers, here we actually have to construct
///         // the audio source list from the descriptor. We can rely on
///         // "can_resolve" to be true for the given descriptor, as the bot
///         // will not query this method otherwise. For plugins where this
///         // operation is fallible anyway, we can return an error message to
///         // be displayed to the user.
///         Ok(Box::new(CommaSeparatedList {
///             entries: resolve_list(descriptor).unwrap().into_iter()
///         }))
///     }
/// }
/// ```
pub trait AudioSourceListResolver : Send + Sync {

    /// Constructs an [AudioDocumentation] for this kind of audio source list.
    /// This is displayed when executing the audio command.
    fn documentation(&self) -> AudioDocumentation;

    /// Indicates whether this resolver can construct an audio source list from
    /// the given descriptor.
    ///
    /// # Arguments
    ///
    /// * `descriptor`: A textual descriptor of unspecified format.
    /// * `guild_config`: A [PluginGuildConfig] containing guild-specific
    ///   information that may be relevant to the resolution.
    ///
    /// # Returns
    ///
    /// True, if and only if this resolver can construct an audio source list
    /// from the given descriptor.
    fn can_resolve(&self, descriptor: &str, guild_config: PluginGuildConfig)
        -> bool;

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
    /// * `guild_config`: A [PluginGuildConfig] containing guild-specific
    ///   information that may be relevant to the resolution.
    ///
    /// # Returns
    ///
    /// A boxed [AudioSourceList] providing the playlist represented by the
    /// given `descriptor`.
    ///
    /// # Errors
    ///
    /// An error message provided as a [String] in case resolution fails.
    fn resolve(&self, descriptor: &str, guild_config: PluginGuildConfig)
        -> Result<Box<dyn AudioSourceList + Send + Sync>, String>;
}

/// A trait for resolvers which can create adapters from key-value arguments.
/// Adapters are essentially effects for [AudioSourceList]s. Similarly to
/// effects, they are realized as [AudioSourceList]s wrapping other audio
/// source lists and altering their output. As an example, a shuffle effect
/// could be realized by wrapping the child audio source list, collecting all
/// its content, shuffling it, and then iterating over it.
///
/// The example provided below is a slightly simplified and more documented
/// version of the adapter and resolver in the `plugin-loop` crate.
/// 
/// ```
/// use rambot_api::{
///     AdapterResolver,
///     AudioSourceList,
///     ModifierDocumentation,
///     ModifierDocumentationBuilder,
///     PluginGuildConfig
/// };
/// 
/// use std::collections::HashMap;
/// use std::io;
/// 
/// // The adapter type that is resolved by our resolver. It wraps a child
/// // audio source list and implements AudioSourceList itself, returning the
/// // modified playlist.
/// struct LoopAudioSourceList {
///     child: Box<dyn AudioSourceList + Send + Sync>,
///     buf: Vec<String>,
///     idx: usize
/// }
/// 
/// impl AudioSourceList for LoopAudioSourceList {
///     fn next(&mut self) -> Result<Option<String>, io::Error> {
///         // Here we must return the next item in the looped playlist.
///         if let Some(s) = self.child.next()? {
///             // We are still in the first pass over the list => memorize any
///             // new descriptors.
///             self.buf.push(s.clone());
///             Ok(Some(s))
///         }
///         else if self.buf.is_empty() {
///             // The child audio source was empty => we are empty as well
///             Ok(None)
///         }
///         else {
///             // The first pass is over, we must now iterate over the
///             // memorized descriptors.
///             let result = self.buf[self.idx].clone();
///             self.idx = (self.idx + 1) % self.buf.len();
///             Ok(Some(result))
///         }
///     }
/// }
/// 
/// struct LoopAdapterResolver;
/// 
/// impl AdapterResolver for LoopAdapterResolver {
///     fn name(&self) -> &str {
///         // Plugin-provided adapters are specified by the user in the format
///         // name(key=value,...). This format is parsed already by the bot.
///         // Hence, we need to specify what the name-part is.
///         "loop"
///     }
///     
///     fn unique(&self) -> bool {
///         // Returning true here instructs the bot to remove any old loop
///         // adapter when a new one is added. We do this as looping an
///         // already looped and therefore infinite list makes no sense.
///         true
///     }
///     
///     fn documentation(&self) -> ModifierDocumentation {
///         // Here we have to construct a documentation to be displayed to the
///         // user when they ask for it.
///         ModifierDocumentationBuilder::new()
///             .with_short_summary("Loops a playlist indefinitely.")
///             .build().unwrap()
///     }
///     
///     fn resolve(&self, _key_values: &HashMap<String, String>,
///             child: Box<dyn AudioSourceList + Send + Sync>, _: PluginGuildConfig)
///             -> Result<Box<dyn AudioSourceList + Send + Sync>, String> {
///         // Here we actually have to construct the effect from the
///         // key-value-pairs parsed by the bot. We get the child to which we
///         // are supposed to apply the effect. As looping does not depend on
///         // any parameters, we can ignore the key-values. To view an example
///         // of their use, check out the documentation of the EffectResolver.
///         // Effects also use this format of a key-value map with parameters.
///         Ok(Box::new(LoopAudioSourceList {
///             child,
///             buf: Vec::new(),
///             idx: 0
///         }))
///     }
/// }
/// ```
pub trait AdapterResolver : Send + Sync {

    /// The name of the kind of adapters resolved by this resolver.
    fn name(&self) -> &str;

    /// Indicates whether adapters of this kind are unique, i.e. there may
    /// exist at most one per layer. When another adapter of the same kind is
    /// added, the old one is removed. This makes sense for example for a loop
    /// adapter, because looping an already infinite (because looped) audio
    /// source list is redundant.
    fn unique(&self) -> bool;

    /// Constructs a [ModifierDocumentation] for this kind of adapter. This is
    /// displayed when executing the adapter help command.
    fn documentation(&self) -> ModifierDocumentation;

    /// Generates an [AudioSourceList] trait object that yields audio source
    /// descriptors constituting the output of the adapter defined by the given
    /// key-value pairs applied to the given child. This may return an error
    /// should the provided key-value map contain invalid inputs.
    ///
    /// # Arguments
    ///
    /// * `key_values`: A [HashMap] storing arguments for this adapter. For
    ///   each supplied argument, the parameter name maps to the string that was
    ///   given as the argument value.
    /// * `child`: A boxed [AudioSourceList] to which the adapter shall be
    ///   applied, i.e. which should be wrapped in an adapter audio source list.
    /// * `guild_config`: A [PluginGuildConfig] containing guild-specific
    ///   information that may be relevant to the resolution.
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
        child: Box<dyn AudioSourceList + Send + Sync>,
        guild_config: PluginGuildConfig)
        -> Result<Box<dyn AudioSourceList + Send + Sync>, String>;
}

/// An interface given to plugins that they can use to register all kinds of
/// resolvers. It abstracts from the concrete handling of registration by the
/// bot.
///
/// For plugin developers, the relevant methods are
/// [ResolverRegistry::register_audio_source_resolver],
/// [ResolverRegistry::register_audio_source_list_resolver],
/// [ResolverRegistry::register_effect_resolver], and
/// [ResolverRegistry::register_adapter_resolver].
pub struct ResolverRegistry<'registry> {
    register_audio_source_resolver:
        Box<dyn FnMut(Box<dyn AudioSourceResolver>) + 'registry>,
    register_audio_source_list_resolver:
        Box<dyn FnMut(Box<dyn AudioSourceListResolver>) + 'registry>,
    register_effect_resolver:
        Box<dyn FnMut(Box<dyn EffectResolver>) + 'registry>,
    register_adapter_resolver:
        Box<dyn FnMut(Box<dyn AdapterResolver>) + 'registry>
}

impl<'registry> ResolverRegistry<'registry> {

    /// Creates a new resolver registry with a specified implementation over
    /// which the constructed registry abstracts.
    ///
    /// # Arguments
    ///
    /// * `register_audio_source_resolver`: A function that receives an
    ///   [AudioSourceResolver] trait object and handles its registration.
    /// * `register_audio_source_list_resolver`: A function that receives an
    ///   [AudioSourceListResolver] trait object and handles its registration.
    /// * `register_effect_resolver`: A function that receives an
    ///   [EffectResolver] trait object and handles its registration.
    /// * `register_adapter_resolver`: A function that receives an
    ///   [AdapterResolver] trait object and handles its registration.
    pub fn new<RegAS, RegASL, RegEf, RegAd>(
        register_audio_source_resolver: RegAS,
        register_audio_source_list_resolver: RegASL,
        register_effect_resolver: RegEf, register_adapter_resolver: RegAd)
        -> ResolverRegistry<'registry>
    where
        RegAS: FnMut(Box<dyn AudioSourceResolver>) + 'registry,
        RegASL: FnMut(Box<dyn AudioSourceListResolver>) + 'registry,
        RegEf: FnMut(Box<dyn EffectResolver>) + 'registry,
        RegAd: FnMut(Box<dyn AdapterResolver>) + 'registry
    {
        ResolverRegistry {
            register_audio_source_resolver:
                Box::new(register_audio_source_resolver),
            register_audio_source_list_resolver:
                Box::new(register_audio_source_list_resolver),
            register_effect_resolver: Box::new(register_effect_resolver),
            register_adapter_resolver: Box::new(register_adapter_resolver)
        }
    }

    /// Registers the given [AudioSourceResolver] with the bot.
    pub fn register_audio_source_resolver<R>(&mut self, resolver: R)
    where
        R: AudioSourceResolver + 'static
    {
        self.register_audio_source_resolver.as_mut()(Box::new(resolver))
    }

    /// Registers the given [AudioSourceListResolver] with the bot.
    pub fn register_audio_source_list_resolver<R>(&mut self, resolver: R)
    where
        R: AudioSourceListResolver + 'static
    {
        self.register_audio_source_list_resolver.as_mut()(Box::new(resolver))
    }

    /// Registers the given [EffectResolver] with the bot.
    pub fn register_effect_resolver<R>(&mut self, resolver: R)
    where
        R: EffectResolver + 'static
    {
        self.register_effect_resolver.as_mut()(Box::new(resolver))
    }

    /// Registers the given [AdapterResolver] with the bot.
    pub fn register_adapter_resolver<R>(&mut self, resolver: R)
    where
        R: AdapterResolver + 'static
    {
        self.register_adapter_resolver.as_mut()(Box::new(resolver))
    }
}
