use libloading::{Library, Symbol};

use rambot_api::{
    AudioSource,
    AudioSourceListResolver,
    AudioSourceResolver,
    EffectResolver,
    Plugin, AudioSourceList, AdapterResolver, PluginConfig
};

use serenity::prelude::TypeMapKey;

use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use crate::config::Config;

#[derive(Debug)]
pub enum LoadPluginsError {
    IoError(io::Error),
    LoadError(libloading::Error),
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

#[derive(Debug)]
pub enum ResolveError {
    NoPluginFound,
    PluginResolveError(String)
}

impl Display for ResolveError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::NoPluginFound =>
                write!(f, "No plugin matches the input."),
            ResolveError::PluginResolveError(msg) =>
                write!(f, "Plugin reported error during resolution: {}", msg)
        }
    }
}

pub enum AudioDescriptorList {
    Single(String),
    List(Box<dyn AudioSourceList + Send>)
}

trait AudioResolver {
    type Value;

    fn can_resolve(&self, descriptor: &str) -> bool;

    fn resolve(&self, descriptor: &str) -> Result<Self::Value, String>;
}

impl AudioResolver for Box<dyn AudioSourceResolver> {
    type Value = Box<dyn AudioSource + Send>;

    fn can_resolve(&self, descriptor: &str) -> bool {
        self.as_ref().can_resolve(descriptor)
    }

    fn resolve(&self, descriptor: &str) -> Result<Self::Value, String> {
        self.as_ref().resolve(descriptor)
    }
}

impl AudioResolver for Box<dyn AudioSourceListResolver> {
    type Value = Box<dyn AudioSourceList + Send>;

    fn can_resolve(&self, descriptor: &str) -> bool {
        self.as_ref().can_resolve(descriptor)
    }

    fn resolve(&self, descriptor: &str) -> Result<Self::Value, String> {
        self.as_ref().resolve(descriptor)
    }
}

trait ModifierResolver {
    type Value;

    fn name(&self) -> &str;

    fn unique(&self) -> bool;

    fn resolve(&self, key_values: &HashMap<String, String>,
        child: Self::Value) -> Result<Self::Value, String>;
}

impl ModifierResolver for Box<dyn EffectResolver> {
    type Value = Box<dyn AudioSource + Send>;

    fn name(&self) -> &str {
        self.as_ref().name()
    }

    fn unique(&self) -> bool {
        self.as_ref().unique()
    }

    fn resolve(&self, key_values: &HashMap<String, String>,
            child: Self::Value) -> Result<Self::Value, String> {
        self.as_ref().resolve(key_values, child)
    }
}

impl ModifierResolver for Box<dyn AdapterResolver> {
    type Value = Box<dyn AudioSourceList + Send>;

    fn name(&self) -> &str {
        self.as_ref().name()
    }

    fn unique(&self) -> bool {
        self.as_ref().unique()
    }

    fn resolve(&self, key_values: &HashMap<String, String>,
            child: Self::Value) -> Result<Self::Value, String> {
        self.as_ref().resolve(key_values, child)
    }
}

fn resolve_audio<V, R>(descriptor: &str, resolvers: &[R])
    -> Result<V, ResolveError>
where
    R: AudioResolver<Value = V>
{
    for resolver in resolvers.iter() {
        if resolver.can_resolve(descriptor) {
            return resolver.resolve(descriptor)
                .map_err(|msg| ResolveError::PluginResolveError(msg));
        }
    }

    Err(ResolveError::NoPluginFound)
}

fn is_modifier_unique<V, R>(name: &str, resolvers: &HashMap<String, R>) -> bool
where
    R: ModifierResolver<Value = V>
{
    resolvers.get(name)
        .map(|r| r.unique())
        .unwrap_or(false)
}

fn resolve_modifier<V, R>(name: &str, key_values: &HashMap<String, String>,
    child: V, resolvers: &HashMap<String, R>) -> Result<V, ResolveError>
where
    R: ModifierResolver<Value = V>
{
    if let Some(resolver) = resolvers.get(name) {
        resolver.resolve(key_values, child)
            .map_err(|msg| ResolveError::PluginResolveError(msg))
    }
    else {
        Err(ResolveError::NoPluginFound)
    }
}

/// Manages loading of plugins and resolution of functionality offered by those
/// plugins (i.e. audio sources, lists, effects, and adapters).
pub struct PluginManager {
    audio_source_resolvers: Vec<Box<dyn AudioSourceResolver>>,
    audio_source_list_resolvers: Vec<Box<dyn AudioSourceListResolver>>,
    effect_resolvers: HashMap<String, Box<dyn EffectResolver>>,
    adapter_resolvers: HashMap<String, Box<dyn AdapterResolver>>,
    loaded_libraries: Vec<Library>
}

impl PluginManager {

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
            loaded_libraries: Vec::new()
        };

        fs::create_dir_all(config.plugin_directory())?;

        for dir_entry in fs::read_dir(config.plugin_directory())? {
            let dir_entry = dir_entry?;
            let file_type = dir_entry.file_type()?;

            if file_type.is_file() {
                // TODO this is probably truly unsafe -- how to contain?

                unsafe {
                    plugin_manager.load_plugin(
                        dir_entry.path(), config.plugin_config())?;
                }
            }
        }

        log::info!("Loaded {} plugins with {} audio sources, {} lists, {} \
            effects, and {} adapters.", plugin_manager.loaded_libraries.len(),
            plugin_manager.audio_source_resolvers.len(),
            plugin_manager.audio_source_list_resolvers.len(),
            plugin_manager.effect_resolvers.len(),
            plugin_manager.adapter_resolvers.len());

        Ok(plugin_manager)
    }

    unsafe fn load_plugin(&mut self, path: PathBuf, config: &PluginConfig)
            -> Result<(), LoadPluginsError> {
        type CreatePlugin = unsafe fn() -> *mut dyn Plugin;
        
        let lib = Library::new(path)?;
        self.loaded_libraries.push(lib);
        let lib = self.loaded_libraries.last().unwrap();
        let create_plugin: Symbol<CreatePlugin> = lib.get(b"_create_plugin")?;
        let raw = create_plugin();
        let mut plugin: Box<dyn Plugin> = Box::from_raw(raw);

        if let Err(msg) = plugin.load_plugin(config) {
            return Err(LoadPluginsError::InitError(msg));
        }

        self.audio_source_resolvers.append(
            &mut plugin.audio_source_resolvers());
        self.audio_source_list_resolvers.append(
            &mut plugin.audio_source_list_resolvers());

        for effect_resolver in plugin.effect_resolvers() {
            let name = effect_resolver.name().to_owned();
            self.effect_resolvers.insert(name, effect_resolver);
        }

        for adapter_resolver in plugin.adapter_resolvers() {
            let name = adapter_resolver.name().to_owned();
            self.adapter_resolvers.insert(name, adapter_resolver);
        }

        Ok(())
    }

    pub fn resolve_audio_source(&self, descriptor: &str)
            -> Result<Box<dyn AudioSource + Send>, ResolveError> {
        resolve_audio(descriptor, &self.audio_source_resolvers)
    }

    pub fn resolve_audio_source_list(&self, descriptor: &str)
            -> Result<Box<dyn AudioSourceList + Send>, ResolveError> {
        resolve_audio(descriptor, &self.audio_source_list_resolvers)
    }

    pub fn resolve_audio_descriptor_list(&self, descriptor: &str)
            -> Result<AudioDescriptorList, ResolveError> {
        match self.resolve_audio_source_list(descriptor) {
            Ok(list) => Ok(AudioDescriptorList::List(list)),
            Err(ResolveError::PluginResolveError(e)) =>
                Err(ResolveError::PluginResolveError(e)),
            _ => Ok(AudioDescriptorList::Single(descriptor.to_owned()))
        }
    }

    pub fn is_effect_unique(&self, name: &str) -> bool {
        is_modifier_unique(name, &self.effect_resolvers)
    }

    pub fn resolve_effect(&self, name: &str,
            key_values: &HashMap<String, String>,
            child: Box<dyn AudioSource + Send>)
            -> Result<Box<dyn AudioSource + Send>, ResolveError> {
        resolve_modifier(name, key_values, child, &self.effect_resolvers)
    }

    pub fn is_adapter_unique(&self, name: &str) -> bool {
        is_modifier_unique(name, &self.adapter_resolvers)
    }

    pub fn resolve_adapter(&self, name: &str,
            key_values: &HashMap<String, String>,
            child: Box<dyn AudioSourceList + Send>)
            -> Result<Box<dyn AudioSourceList + Send>, ResolveError> {
        resolve_modifier(name, key_values, child, &self.adapter_resolvers)
    }
}

impl TypeMapKey for PluginManager {
    type Value = Arc<PluginManager>;
}
