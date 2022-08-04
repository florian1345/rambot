use libloading::{Library, Symbol};

use rambot_api::{
    AdapterResolver,
    AudioDocumentation,
    AudioSource,
    AudioSourceList,
    AudioSourceListResolver,
    AudioSourceResolver,
    EffectResolver,
    ModifierDocumentation,
    Plugin,
    PluginConfig,
    PluginGuildConfig,
    ResolverRegistry
};

use serenity::prelude::TypeMapKey;

use std::collections::HashMap;
use std::collections::hash_map::Keys;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use crate::config::Config;

/// An enumeration of the different errors that can occur when loading plugins.
#[derive(Debug)]
pub enum LoadPluginsError {

    /// An IO error that occurred while loading the plugin file.
    IoError(io::Error),

    /// A dynamic library loading error.
    LoadError(libloading::Error),

    /// An initialization error raised by the plugin itself. A message is
    /// provided.
    InitError(String)
}

impl From<io::Error> for LoadPluginsError {
    fn from(e: io::Error) -> LoadPluginsError {
        LoadPluginsError::IoError(e)
    }
}

impl From<libloading::Error> for LoadPluginsError {
    fn from(e: libloading::Error) -> LoadPluginsError {
        LoadPluginsError::LoadError(e)
    }
}

impl Display for LoadPluginsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            LoadPluginsError::IoError(e) =>
                write!(f, "error loading plugin file: {}", e),
            LoadPluginsError::LoadError(e) =>
                write!(f, "error loading plugin library: {}", e),
            LoadPluginsError::InitError(msg) =>
                write!(f, "plugin reported initialization error: {}", msg)
        }
    }
}

/// An enumeration of the errors that can occur when resolving an audio source,
/// audio source list, effect, or adapter by a [PluginManager].
#[derive(Debug)]
pub enum ResolveError {

    /// No plugin reported that it could resolve the given audio source/audio
    /// source list descriptor or effect/adapter name.
    NoPluginFound,

    /// A plugin that claimed to be able to resolve the given audio
    /// source/audio source list descriptor or effect/adapter name was found,
    /// however it reported an error during the actual resolution. An error
    /// message is provided.
    PluginResolveError(String)
}

impl Display for ResolveError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::NoPluginFound =>
                write!(f, "No plugin matches the input."),
            ResolveError::PluginResolveError(e) =>
                write!(f, "Plugin reported error during resolution: {}", e)
        }
    }
}

/// An abstract representation of a list of strings constituting audio source
/// descriptors.
pub enum AudioDescriptorList {

    /// A single audio descriptor, which is the string wrapped in this variant.
    Single(String),

    /// An [AudioSourceList] providing audio descriptors, which is wrapped in
    /// this variant.
    List(Box<dyn AudioSourceList + Send>)
}

trait AudioResolver {
    type Value;

    fn can_resolve(&self, descriptor: &str,
        plugin_guild_config: PluginGuildConfig) -> bool;

    fn resolve(&self, descriptor: &str, plugin_guild_config: PluginGuildConfig)
        -> Result<Self::Value, String>;
}

impl AudioResolver for Box<dyn AudioSourceResolver> {
    type Value = Box<dyn AudioSource + Send>;

    fn can_resolve(&self, descriptor: &str,
            plugin_guild_config: PluginGuildConfig) -> bool {
        self.as_ref().can_resolve(descriptor, plugin_guild_config)
    }

    fn resolve(&self, descriptor: &str, plugin_guild_config: PluginGuildConfig)
            -> Result<Self::Value, String> {
        self.as_ref().resolve(descriptor, plugin_guild_config)
    }
}

impl AudioResolver for Box<dyn AudioSourceListResolver> {
    type Value = Box<dyn AudioSourceList + Send>;

    fn can_resolve(&self, descriptor: &str,
            plugin_guild_config: PluginGuildConfig) -> bool {
        self.as_ref().can_resolve(descriptor, plugin_guild_config)
    }

    fn resolve(&self, descriptor: &str, plugin_guild_config: PluginGuildConfig)
            -> Result<Self::Value, String> {
        self.as_ref().resolve(descriptor, plugin_guild_config)
    }
}

trait ModifierResolver {

    fn name(&self) -> &str;

    fn unique(&self) -> bool;

    fn documentation(&self) -> ModifierDocumentation;
}

impl ModifierResolver for Box<dyn EffectResolver> {

    fn name(&self) -> &str {
        self.as_ref().name()
    }

    fn unique(&self) -> bool {
        self.as_ref().unique()
    }

    fn documentation(&self) -> ModifierDocumentation {
        self.as_ref().documentation()
    }
}

impl ModifierResolver for Box<dyn AdapterResolver> {

    fn name(&self) -> &str {
        self.as_ref().name()
    }

    fn unique(&self) -> bool {
        self.as_ref().unique()
    }

    fn documentation(&self) -> ModifierDocumentation {
        self.as_ref().documentation()
    }
}

fn resolve_audio<V, R>(descriptor: &str,
    plugin_guild_config: &PluginGuildConfig, resolvers: &[R])
    -> Result<V, ResolveError>
where
    R: AudioResolver<Value = V>
{
    for resolver in resolvers.iter() {
        if resolver.can_resolve(descriptor, plugin_guild_config.clone()) {
            return resolver.resolve(descriptor, plugin_guild_config.clone())
                .map_err(ResolveError::PluginResolveError);
        }
    }

    Err(ResolveError::NoPluginFound)
}

fn is_modifier_unique<R>(name: &str, resolvers: &HashMap<String, R>) -> bool
where
    R: ModifierResolver
{
    resolvers.get(name)
        .map(|r| r.unique())
        .unwrap_or(false)
}

fn get_modifier_documentation<R>(name: &str, resolvers: &HashMap<String, R>)
    -> Option<ModifierDocumentation>
where
    R: ModifierResolver
{
    resolvers.get(name)
        .map(|r| r.documentation())
}

unsafe fn load_plugin(path: PathBuf, config: PluginConfig,
        registry: &mut ResolverRegistry, plugins: &mut Vec<Box<dyn Plugin>>,
        loaded_libraries: &mut Vec<Library>) -> Result<(), LoadPluginsError> {
    type CreatePlugin = unsafe fn() -> *mut Box<dyn Plugin>;

    let lib = Library::new(path)?;
    loaded_libraries.push(lib);
    let lib = loaded_libraries.last().unwrap();
    let create_plugin: Symbol<CreatePlugin> = lib.get(b"_create_plugin")?;
    let raw = create_plugin();
    plugins.push(*Box::from_raw(raw));
    let plugin = plugins.last().unwrap();

    if let Err(msg) = plugin.load_plugin(config, registry) {
        return Err(LoadPluginsError::InitError(msg));
    }

    Ok(())
}

/// Manages loading of plugins and resolution of functionality offered by those
/// plugins (i.e. audio sources, lists, effects, and adapters).
pub struct PluginManager {
    audio_source_resolvers: Vec<Box<dyn AudioSourceResolver>>,
    audio_source_list_resolvers: Vec<Box<dyn AudioSourceListResolver>>,
    effect_resolvers: HashMap<String, Box<dyn EffectResolver>>,
    adapter_resolvers: HashMap<String, Box<dyn AdapterResolver>>,
    plugins: Vec<Box<dyn Plugin>>,
    loaded_libraries: Vec<Library>
}

impl PluginManager {

    #[cfg(test)]
    pub(crate) fn mock() -> PluginManager {
        PluginManager {
            audio_source_resolvers: Vec::new(),
            audio_source_list_resolvers: Vec::new(),
            effect_resolvers: HashMap::new(),
            adapter_resolvers: HashMap::new(),
            plugins: Vec::new(),
            loaded_libraries: Vec::new()
        }
    }

    /// Loads plugins from the plugin directory specified in the given config
    /// and returns a manager for them.
    ///
    /// # Arguments
    ///
    /// * `config`: The [Config] which specifies the plugin directory as well
    /// as the [PluginConfig] provided to the individual plugins during
    /// initialization.
    ///
    /// # Errors
    ///
    /// Any [LoadPluginsError] according to their respective documentation.
    pub fn new(config: &Config) -> Result<PluginManager, LoadPluginsError> {
        let mut plugin_manager = PluginManager {
            audio_source_resolvers: Vec::new(),
            audio_source_list_resolvers: Vec::new(),
            effect_resolvers: HashMap::new(),
            adapter_resolvers: HashMap::new(),
            plugins: Vec::new(),
            loaded_libraries: Vec::new()
        };
        let mut resolver_registry = ResolverRegistry::new(
            |r| plugin_manager.audio_source_resolvers.push(r),
            |r| plugin_manager.audio_source_list_resolvers.push(r),
            |r| {
                let name = r.name().to_owned();
                plugin_manager.effect_resolvers.insert(name, r);
            },
            |r| {
                let name = r.name().to_owned();
                plugin_manager.adapter_resolvers.insert(name, r);
            }
        );

        fs::create_dir_all(config.plugin_directory())?;
        fs::create_dir_all(config.plugin_config_directory())?;

        for dir_entry in fs::read_dir(config.plugin_directory())? {
            let dir_entry = dir_entry?;
            let file_type = dir_entry.file_type()?;

            if file_type.is_file() {
                let plugin_config = config.generate_plugin_config(
                    dir_entry.file_name().to_str().unwrap());

                // TODO this is probably truly unsafe -- how to contain?

                unsafe {
                    load_plugin(
                        dir_entry.path(),
                        plugin_config,
                        &mut resolver_registry,
                        &mut plugin_manager.plugins,
                        &mut plugin_manager.loaded_libraries)?;
                }
            }
        }

        drop(resolver_registry);

        log::info!("Loaded {} plugins with {} audio sources, {} lists, {} \
            effects, and {} adapters.", plugin_manager.loaded_libraries.len(),
            plugin_manager.audio_source_resolvers.len(),
            plugin_manager.audio_source_list_resolvers.len(),
            plugin_manager.effect_resolvers.len(),
            plugin_manager.adapter_resolvers.len());

        Ok(plugin_manager)
    }

    /// Resolves an [AudioSource] given a textual descriptor by searching for a
    /// plugin-provided resolver that can process the descriptor.
    ///
    /// # Arguments
    ///
    /// * `descriptor`: A textual descriptor of the audio source to resolve.
    /// The accepted format(s) depends on the installed plugins.
    /// * `plugin_guild_config`: A reference to the [PluginGuildConfig] in
    /// which carries guild-specific information for the plugin(s).
    ///
    /// # Returns
    ///
    /// A new [AudioSource] trait object resolved by some plugin from the given
    /// descriptor.
    ///
    /// # Errors
    ///
    /// Any [ResolveError] according to their respective documentation.
    pub fn resolve_audio_source(&self, descriptor: &str,
            plugin_guild_config: &PluginGuildConfig)
            -> Result<Box<dyn AudioSource + Send>, ResolveError> {
        resolve_audio(descriptor, plugin_guild_config,
            &self.audio_source_resolvers)
    }

    /// Resolves an [AudioSourceList] given a textual descriptor by searching
    /// for a plugin-provided resolver that can process the descriptor.
    ///
    /// # Arguments
    ///
    /// * `descriptor`: A textual descriptor of the audio source list to
    /// resolve. The accepted format(s) depends on the installed plugins.
    /// * `plugin_guild_config`: A reference to the [PluginGuildConfig] in
    /// which carries guild-specific information for the plugin(s).
    ///
    /// # Returns
    ///
    /// A new [AudioSourceList] trait object resolved by some plugin from the
    /// given descriptor.
    ///
    /// # Errors
    ///
    /// Any [ResolveError] according to their respective documentation.
    pub fn resolve_audio_source_list(&self, descriptor: &str,
            plugin_guild_config: &PluginGuildConfig)
            -> Result<Box<dyn AudioSourceList + Send>, ResolveError> {
        resolve_audio(descriptor, plugin_guild_config,
            &self.audio_source_list_resolvers)
    }

    /// Resolves an [AudioDescriptorList] given a textual descriptor. This is
    /// achieved by searching for a audio source list plugin that can resolve
    /// it as a list and, if none is available, returning the descriptor itself
    /// as a [AudioDescriptorList::Single].
    ///
    /// # Arguments
    ///
    /// * `descriptor`: A textual descriptor of the audio descriptor list to
    /// resolve.
    /// * `plugin_guild_config`: A reference to the [PluginGuildConfig] in
    /// which carries guild-specific information for the plugin(s).
    ///
    /// # Errors
    ///
    /// * [ResolveError::PluginResolveError] if a plugin claims to be able to
    /// resolve the descriptor as an audio source list, but fails to do so when
    /// queried.
    pub fn resolve_audio_descriptor_list(&self, descriptor: &str,
            plugin_guild_config: &PluginGuildConfig)
            -> Result<AudioDescriptorList, ResolveError> {
        match self.resolve_audio_source_list(descriptor, plugin_guild_config) {
            Ok(list) => Ok(AudioDescriptorList::List(list)),
            Err(ResolveError::PluginResolveError(e)) =>
                Err(ResolveError::PluginResolveError(e)),
            _ => Ok(AudioDescriptorList::Single(descriptor.to_owned()))
        }
    }

    /// Gets an iterator over the [AudioDocumentation]s for all audio sources
    /// and audio source lists provided by plugins.
    pub fn get_audio_documentations(&self)
            -> impl Iterator<Item = AudioDocumentation> + '_ {
        self.audio_source_resolvers.iter()
            .map(|r| r.documentation())
            .chain(self.audio_source_list_resolvers.iter()
                .map(|r| r.documentation()))
    }

    /// Gets an iterator over the names of all effects that have been
    /// registered by plugins.
    pub fn effect_names(&self) -> Keys<'_, String, Box<dyn EffectResolver>> {
        self.effect_resolvers.keys()
    }

    /// Indicates whether the effect with the given name is unique, that is,
    /// only one can exist per layer. If no effect with the given name exists,
    /// `false` is returned.
    pub fn is_effect_unique(&self, name: &str) -> bool {
        is_modifier_unique(name, &self.effect_resolvers)
    }

    /// Resolves an effect given the name and parameters as key-values by
    /// querying a plugin-provided resolver for the given name.
    ///
    /// # Arguments
    ///
    /// * `name`: The name of the effect type to resolve. This is the key by
    /// which the resolver is looked up.
    /// * `key_values`: A [HashMap] that stores key-value pairs provided as
    /// arguments for the effect.
    /// * `child`: The [AudioSource] to which to apply the resolved effect.
    /// * `plugin_guild_config`: A reference to the [PluginGuildConfig] in
    /// which carries guild-specific information for the plugin(s).
    ///
    /// # Returns
    ///
    /// A new [AudioSource] trait object that represents the resolved effect
    /// applied to the child.
    ///
    /// # Errors
    ///
    /// Any [ResolveError] according to their respective documentation.
    pub fn resolve_effect(&self, name: &str,
            key_values: &HashMap<String, String>,
            child: Box<dyn AudioSource + Send>,
            plugin_guild_config: &PluginGuildConfig)
            -> Result<Box<dyn AudioSource + Send>,
                (ResolveError, Box<dyn AudioSource + Send>)> {
        if let Some(resolver) = self.effect_resolvers.get(name) {
            resolver.resolve(key_values, child, plugin_guild_config.clone())
                .map_err(|e| {
                    let (msg, child) = e.into_parts();
                    (ResolveError::PluginResolveError(msg), child)
                })
        }
        else {
            Err((ResolveError::NoPluginFound, child))
        }
    }

    /// Gets the documentation for the effect with the given name. The
    /// documentation is provided by plugins themselves.
    ///
    /// # Arguments
    ///
    /// * `name`: The name of the effect of which to get the documentation.
    ///
    /// # Returns
    ///
    /// `Some(_)` with the [ModifierDocumentation] for the effect with the
    /// given name and `None` if no such effect exists.
    pub fn get_effect_documentation(&self, name: &str)
            -> Option<ModifierDocumentation> {
        get_modifier_documentation(name, &self.effect_resolvers)
    }

    /// Gets an iterator over the names of all adapters that have been
    /// registered by plugins.
    pub fn adapter_names(&self) -> Keys<'_, String, Box<dyn AdapterResolver>> {
        self.adapter_resolvers.keys()
    }

    /// Indicates whether the adapter with the given name is unique, that is,
    /// only one can exist per layer. If no adapter with the given name exists,
    /// `false` is returned.
    pub fn is_adapter_unique(&self, name: &str) -> bool {
        is_modifier_unique(name, &self.adapter_resolvers)
    }

    /// Resolves an adapter given the name and parameters as key-values by
    /// querying a plugin-provided resolver for the given name.
    ///
    /// # Arguments
    ///
    /// * `name`: The name of the adapter type to resolve. This is the key by
    /// which the resolver is looked up.
    /// * `key_values`: A [HashMap] that stores key-value pairs provided as
    /// arguments for the adapter.
    /// * `child`: The [AudioSource] to which to apply the resolved adapter.
    /// * `plugin_guild_config`: A reference to the [PluginGuildConfig] in
    /// which carries guild-specific information for the plugin(s).
    ///
    /// # Returns
    ///
    /// A new [AudioSource] trait object that represents the resolved adapter
    /// applied to the child.
    ///
    /// # Errors
    ///
    /// Any [ResolveError] according to their respective documentation.
    pub fn resolve_adapter(&self, name: &str,
            key_values: &HashMap<String, String>,
            child: Box<dyn AudioSourceList + Send>,
            plugin_guild_config: &PluginGuildConfig)
            -> Result<Box<dyn AudioSourceList + Send>, ResolveError> {
        if let Some(resolver) = self.adapter_resolvers.get(name) {
            resolver.resolve(key_values, child, plugin_guild_config.clone())
                .map_err(ResolveError::PluginResolveError)
        }
        else {
            Err(ResolveError::NoPluginFound)
        }
    }

    /// Gets the documentation for the adapter with the given name. The
    /// documentation is provided by plugins themselves.
    ///
    /// # Arguments
    ///
    /// * `name`: The name of the adapter of which to get the documentation.
    ///
    /// # Returns
    ///
    /// `Some(_)` with the [ModifierDocumentation] for the adapter with the
    /// given name and `None` if no such adapter exists.
    pub fn get_adapter_documentation(&self, name: &str)
            -> Option<ModifierDocumentation> {
        get_modifier_documentation(name, &self.adapter_resolvers)
    }

    fn unload(&mut self) {
        let count = self.plugins.len();

        for plugin in self.plugins.drain(..) {
            plugin.unload_plugin();
        }

        for library in self.loaded_libraries.drain(..) {
            drop(library);
        }

        log::debug!("Unloaded {} plugins.", count);
    }
}

impl TypeMapKey for PluginManager {
    type Value = Arc<PluginManager>;
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        if !self.plugins.is_empty() || !self.loaded_libraries.is_empty() {
            self.unload()
        }
    }
}
