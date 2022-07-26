use rambot_api::{
    AudioSource,
    EffectResolver,
    Plugin,
    Sample,
    PluginConfig,
    ResolveEffectError,
    ResolverRegistry
};

use std::{io, collections::HashMap};

struct VolumeEffect {
    child: Option<Box<dyn AudioSource + Send>>,
    volume: f32
}

impl AudioSource for VolumeEffect {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        let count = self.child.as_mut().unwrap().read(buf)?;

        for sample in buf.iter_mut().take(count) {
            *sample *= self.volume;
        }

        Ok(count)
    }

    fn has_child(&self) -> bool {
        true
    }

    fn take_child(&mut self) -> Box<dyn AudioSource + Send> {
        self.child.take().unwrap()
    }
}

fn get_volume(key_values: &HashMap<String, String>) -> Result<f32, String> {
    key_values.get("volume")
        .map(|v| v.parse::<f32>()
            .map_err(|e| format!("Error parsing volume number: {}", e)))
        .unwrap_or_else(|| Err("Missing \"volume\" key.".to_owned()))
}

struct VolumeEffectResolver;

impl EffectResolver for VolumeEffectResolver {
    fn name(&self) -> &str {
        "volume"
    }

    fn unique(&self) -> bool {
        true
    }

    fn resolve(&self, key_values: &HashMap<String, String>,
            child: Box<dyn AudioSource + Send>)
            -> Result<Box<dyn AudioSource + Send>, ResolveEffectError> {
        let volume = match get_volume(key_values) {
            Ok(v) => v,
            Err(msg) => return Err(ResolveEffectError::new(msg, child))
        };

        Ok(Box::new(VolumeEffect {
            child: Some(child),
            volume
        }))
    }
}

struct VolumePlugin;

impl Plugin for VolumePlugin {
    fn load_plugin<'registry>(&mut self, _config: &PluginConfig,
            registry: &mut ResolverRegistry<'registry>) -> Result<(), String> {
        registry.register_effect_resolver(VolumeEffectResolver);
        Ok(())
    }
}

fn make_volume_plugin() -> VolumePlugin {
    VolumePlugin
}

rambot_api::export_plugin!(make_volume_plugin);
