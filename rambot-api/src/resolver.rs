use crate::audio::{AudioSource, AudioSourceList};

use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};

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

/// An error that occurs when a plugin attempts to resolve an effect, i.e.
/// [EffectResolver::resolve] is called. In addition to the ordinary message
/// provided with plugin errors, this type also contains the child to which the
/// effect was supposed to be applied. This allows the plugin to return the
/// unmodified child and therefore the bot can restore the audio after a failed
/// resolution.
pub struct ResolveEffectError {
    message: String,
    child: Box<dyn AudioSource + Send>
}

impl ResolveEffectError {

    /// Creates a new resolve effect error from message and child.
    ///
    /// # Arguments
    ///
    /// * `message`: The error message to be displayed to the user.
    /// * `child`: The child audio source to which the effect should have been
    /// applied.
    pub fn new<S>(message: S, child: Box<dyn AudioSource + Send>)
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
    pub fn into_parts(self) -> (String, Box<dyn AudioSource + Send>) {
        (self.message, self.child)
    }
}

impl Display for ResolveEffectError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.message)
    }
}

struct ModifierParameterDocumentation {
    name: String,
    description: String
}

impl Display for ModifierParameterDocumentation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "`{}`: {}", &self.name, &self.description)
    }
}

/// Documentation of a modifier (effect or adapter) to be displayed to the user
/// of the bot. The short form can be accessed by
/// [ModifierDocumentation::short_summary] while a long markdown version is
/// available behind the implementation of the [Display] trait.
///
/// To construct instances of this type, use the
/// [ModifierDocumentationBuilder].
pub struct ModifierDocumentation {
    short_summary: String,
    long_summary: String,
    parameters: Vec<ModifierParameterDocumentation>
}

impl ModifierDocumentation {

    /// Gets a short summary of the functionality of the documented modifier.
    /// This is used for the overview page.
    pub fn short_summary(&self) -> &str {
        &self.short_summary
    }
}

impl Display for ModifierDocumentation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.long_summary)?;

        if self.parameters.is_empty() {
            return Ok(());
        }

        write!(f, "\n")?;

        for parameter in &self.parameters {
            write!(f, "\n- {}", parameter)?;
        }

        Ok(())
    }
}

/// A builder for [ModifierDocumentation]s. To construct a modifier
/// documentation, create a new builder using
/// [ModifierDocumentationBuilder::new], specify at least a short summary
/// using [ModifierDocumentationBuilder::with_short_summary], and then build
/// the final documentation using [ModifierDocumentation::build]. Further
/// information can be provided with other methods. You do not need to provide
/// the effect/adapter name, as that is taken from context.
///
/// A simple usage example is shown below.
///
/// ```
/// // Documentation for a volume effect
///
/// let doc = ModifierDocumentationBuilder::new()
///     .with_short_summary("Controls the volume of a layer.")
///     .with_long_summary(
///         "Controls the volume of a layer by multiplying all audio with a \
///         given factor.")
///     .with_parameter("volume", "The factor by which audio is multiplied.")
///     .build();
/// ```
pub struct ModifierDocumentationBuilder {
    short_summary: Option<String>,
    long_summary: Option<String>,
    parameters: Vec<ModifierParameterDocumentation>
}

impl ModifierDocumentationBuilder {

    /// Creates a new modifier documentation builder.
    pub fn new() -> ModifierDocumentationBuilder {
        ModifierDocumentationBuilder {
            short_summary: None,
            long_summary: None,
            parameters: Vec::new()
        }
    }

    /// Specify a short summary for this effect/adapter to be displayed in the
    /// overview. If no long summary has been specified, it will be assigned to
    /// the given short summary as well.
    ///
    /// # Arguments
    ///
    /// * `summary`: A short summary for this effect/adapter. Markdown is
    /// supported.
    ///
    /// # Returns
    ///
    /// This builder after the operation. Useful for chaining.
    pub fn with_short_summary<S>(mut self, summary: S)
        -> ModifierDocumentationBuilder
    where
        S: Into<String>
    {
        let summary = summary.into();

        self.long_summary.get_or_insert_with(|| summary.clone());
        self.short_summary = Some(summary);
        self
    }

    /// Specify a long summary for this effect/adapter to be displayed in the
    /// effect/adapter specific help page.
    ///
    /// # Arguments
    ///
    /// * `summary`: A long summary for this effect/adapter. Markdown is
    /// supported.
    ///
    /// # Returns
    ///
    /// This builder after the operation. Useful for chaining.
    pub fn with_long_summary<S>(mut self, summary: S)
        -> ModifierDocumentationBuilder
    where
        S: Into<String>
    {
        self.long_summary = Some(summary.into());
        self
    }

    /// Adds a parameter documentation for a new parameter to the constructed
    /// modifier documentation. To add multiple parameters, call this method
    /// multiple times. The parameters will be displayed top-to-bottom in the
    /// order this method is called.
    ///
    /// # Arguments
    ///
    /// * `name`: The name of the documented parameter.
    /// * `description`: A description to be displayed for the documented
    /// parameter.
    ///
    /// # Returns
    ///
    /// This builder after the operation. Useful for chaining.
    pub fn with_parameter<S1, S2>(mut self, name: S1, description: S2)
        -> ModifierDocumentationBuilder
    where
        S1: Into<String>,
        S2: Into<String>
    {
        let name = name.into();
        let description = description.into();

        self.parameters.push(ModifierParameterDocumentation {
            name,
            description
        });

        self
    }

    /// Builds the modifier documentation constructed from the data provided
    /// with previous method calls. At least
    /// [ModifierDocumentationBuilder::with_short_summary] is required to be
    /// called before this.
    ///
    /// # Returns
    ///
    /// `Some(_)` with a new [ModifierDocumentation] instance with the
    /// previously provided information. If no short summary has been
    /// specified, `None` is returned.
    pub fn build(self) -> Option<ModifierDocumentation> {
        self.short_summary
            .and_then(|short| self.long_summary.map(|long| (short, long)))
            .map(|(short_summary, long_summary)| ModifierDocumentation {
                short_summary,
                long_summary,
                parameters: self.parameters
            })
    }
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
    /// A [ResolveEffectError] containing an error message as well as the given
    /// `child` in case resolution fails.
    fn resolve(&self, key_values: &HashMap<String, String>,
        child: Box<dyn AudioSource + Send>)
        -> Result<Box<dyn AudioSource + Send>, ResolveEffectError>;
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
