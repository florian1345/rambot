use libloading::{Library, Symbol};

use rambot_api::{
    AudioSource,
    AudioSourceListResolver,
    AudioSourceResolver,
    EffectResolver,
    Plugin, AudioSourceList
};

use serenity::prelude::TypeMapKey;

use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

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

pub enum Audio {
    Single(Box<dyn AudioSource + Send>),
    List(Box<dyn AudioSourceList + Send>)
}

pub struct PluginManager {
    audio_source_resolvers: Vec<Box<dyn AudioSourceResolver>>,
    audio_source_list_resolvers: Vec<Box<dyn AudioSourceListResolver>>,
    effect_resolvers: HashMap<String, Box<dyn EffectResolver>>,
    loaded_libraries: Vec<Library>
}

impl PluginManager {

    pub fn new(directory: &str) -> Result<PluginManager, LoadPluginsError> {
        let mut plugin_manager = PluginManager {
            audio_source_resolvers: Vec::new(),
            audio_source_list_resolvers: Vec::new(),
            effect_resolvers: HashMap::new(),
            loaded_libraries: Vec::new()
        };

        for dir_entry in fs::read_dir(directory)? {
            let dir_entry = dir_entry?;
            let file_type = dir_entry.file_type()?;

            if file_type.is_file() {
                // TODO this is probably truly unsafe -- how to contain?

                unsafe {
                    plugin_manager.load_plugin(dir_entry.path())?;
                }
            }
        }

        log::info!("Loaded {} plugins with {} audio sources, {} lists, and {} \
            effects.", plugin_manager.loaded_libraries.len(),
            plugin_manager.audio_source_resolvers.len(),
            plugin_manager.audio_source_list_resolvers.len(),
            plugin_manager.effect_resolvers.len());

        Ok(plugin_manager)
    }

    unsafe fn load_plugin(&mut self, path: PathBuf)
            -> Result<(), LoadPluginsError> {
        type CreatePlugin = unsafe fn() -> *mut dyn Plugin;
        
        let lib = Library::new(path)?;
        self.loaded_libraries.push(lib);
        let lib = self.loaded_libraries.last().unwrap();
        let create_plugin: Symbol<CreatePlugin> = lib.get(b"_create_plugin")?;
        let raw = create_plugin();
        let plugin: Box<dyn Plugin> = Box::from_raw(raw);

        if let Err(msg) = plugin.load_plugin() {
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

        Ok(())
    }

    pub fn resolve_audio_source(&self, descriptor: &str)
            -> Result<Box<dyn AudioSource + Send>, ResolveError> {
        for resolver in self.audio_source_resolvers.iter() {
            if resolver.can_resolve(descriptor) {
                return resolver.resolve(descriptor)
                    .map_err(|msg| ResolveError::PluginResolveError(msg));
            }
        }

        Err(ResolveError::NoPluginFound)
    }

    pub fn resolve_audio_source_list(&self, descriptor: &str)
            -> Result<Box<dyn AudioSourceList + Send>, ResolveError> {
        for resolver in self.audio_source_list_resolvers.iter() {
            if resolver.can_resolve(descriptor) {
                return resolver.resolve(descriptor)
                    .map_err(|msg| ResolveError::PluginResolveError(msg));
            }
        }
    
        Err(ResolveError::NoPluginFound)
    }

    pub fn resolve_audio(&self, descriptor: &str)
            -> Result<Audio, ResolveError> {
        match self.resolve_audio_source(descriptor) {
            Ok(source) => Ok(Audio::Single(source)),
            Err(ResolveError::PluginResolveError(e)) =>
                Err(ResolveError::PluginResolveError(e)),
            _ => {
                self.resolve_audio_source_list(descriptor)
                    .map(|list| Audio::List(list))
            }
        }
    }

    pub fn is_effect_unique(&self, name: &str) -> bool {
        self.effect_resolvers.get(name)
            .map(|r| r.unique())
            .unwrap_or(false)
    }

    pub fn resolve_effect(&self, name: &str,
            key_values: &HashMap<String, String>,
            child: Box<dyn AudioSource + Send>)
            -> Result<Box<dyn AudioSource + Send>, ResolveError> {
        if let Some(resolver) = self.effect_resolvers.get(name) {
            resolver.resolve(key_values, child)
                .map_err(|msg| ResolveError::PluginResolveError(msg))
        }
        else {
            Err(ResolveError::NoPluginFound)
        }
    }
}

impl TypeMapKey for PluginManager {
    type Value = Arc<PluginManager>;
}
