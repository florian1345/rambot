use rambot_api::{
    AudioSource,
    AudioSourceListResolver,
    AudioSourceResolver,
    EffectResolver,
    Plugin,
    Sample
};

use regex::Regex;

use std::io;

struct VolumeEffect {
    child: Option<Box<dyn AudioSource + Send>>,
    volume: f32
}

impl AudioSource for VolumeEffect {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error> {
        let count = self.child.as_mut().unwrap().read(buf)?;

        for i in 0..count {
            buf[i] *= self.volume;
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

lazy_static::lazy_static! {
    static ref REGEX: Regex = Regex::new(r"volume=([0-9]+(?:\.[0-9]+)?)")
        .unwrap();
}

struct VolumeEffectResolver;

impl EffectResolver for VolumeEffectResolver {
    fn can_resolve(&self, descriptor: &str) -> bool {
        REGEX.is_match(descriptor)
    }

    fn resolve(&self, descriptor: &str, child: Box<dyn AudioSource + Send>)
            -> Result<Box<dyn AudioSource + Send>, String> {
        if let Some(captures) = REGEX.captures(descriptor) {
            let volume = captures.get(1).unwrap().as_str().parse().unwrap();

            Ok(Box::new(VolumeEffect {
                child: Some(child),
                volume
            }))
        }
        else {
            Err("Descriptor does not match required syntax.".to_owned())
        }
    }
}

struct VolumePlugin;

impl Plugin for VolumePlugin {
    fn load_plugin(&self) -> Result<(), String> {
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
}

fn make_volume_plugin() -> VolumePlugin {
    VolumePlugin
}

rambot_api::export_plugin!(make_volume_plugin);
