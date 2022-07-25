use rambot_api::{
    AdapterResolver,
    AudioSource,
    AudioSourceListResolver,
    AudioSourceResolver,
    EffectResolver,
    Plugin,
    Sample,
    PluginConfig
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
            -> Result<Box<dyn AudioSource + Send>, String> {
        let volume = key_values.get("volume")
            .map(|v| v.parse::<f32>()
                .map_err(|e| format!("Error parsing volume number: {}", e)))
            .unwrap_or_else(|| Err("Missing \"volume\" key.".to_owned()))?;

        Ok(Box::new(VolumeEffect {
            child: Some(child),
            volume
        }))
    }
}

struct VolumePlugin;

impl Plugin for VolumePlugin {
    fn load_plugin(&mut self, _config: &PluginConfig) -> Result<(), String> {
        Ok(())
    }

    fn audio_source_resolvers(&self) -> Vec<Box<dyn AudioSourceResolver>> {
        Vec::new()
    }

    fn effect_resolvers(&self) -> Vec<Box<dyn EffectResolver>> {
        vec![Box::new(VolumeEffectResolver)]
    }

    fn audio_source_list_resolvers(&self)
            -> Vec<Box<dyn AudioSourceListResolver>> {
        Vec::new()
    }

    fn adapter_resolvers(&self) -> Vec<Box<dyn AdapterResolver>> {
        Vec::new()
    }
}

fn make_volume_plugin() -> VolumePlugin {
    VolumePlugin
}

rambot_api::export_plugin!(make_volume_plugin);
