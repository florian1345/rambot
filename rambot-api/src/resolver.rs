use crate::audio::{AudioSource, AudioSourceList};

use std::collections::HashMap;

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
    /// [AudioSourceResolver] trait object and handles its registration.
    /// * `register_audio_source_list_resolver`: A function that receives an
    /// [AudioSourceListResolver] trait object and handles its registration.
    /// * `register_effect_resolver`: A function that receives an
    /// [EffectResolver] trait object and handles its registration.
    /// * `register_adapter_resolver`: A function that receives an
    /// [AdapterResolver] trait object and handles its registration.
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
