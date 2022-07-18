use std::collections::VecDeque;
use std::io;

use rambot_api::{
    AdapterResolver,
    AudioSourceList,
    AudioSourceListResolver,
    AudioSourceResolver,
    EffectResolver,
    Plugin
};

struct JsonAudioSourceList {
    audio_sources: VecDeque<String>
}

impl AudioSourceList for JsonAudioSourceList {
    fn next(&mut self) -> Result<Option<String>, io::Error> {
        Ok(self.audio_sources.pop_front())
    }
}

struct JsonAudioSourceListResolver;

impl AudioSourceListResolver for JsonAudioSourceListResolver {

    fn can_resolve(&self, descriptor: &str) -> bool {
        plugin_commons::is_file_with_extension(descriptor, ".json")
    }

    fn resolve(&self, descriptor: &str)
            -> Result<Box<dyn AudioSourceList + Send>, String> {
        let reader = plugin_commons::open_file_buf(descriptor)?;
        let audio_sources: Vec<String> = serde_json::from_reader(reader)
            .map_err(|e| format!("{}", e))?;

        Ok(Box::new(JsonAudioSourceList {
            audio_sources: VecDeque::from(audio_sources)
        }))
    }
}

struct JsonListPlugin;

impl Plugin for JsonListPlugin {

    fn load_plugin(&self) -> Result<(), String> {
        Ok(())
    }

    fn audio_source_resolvers(&self) -> Vec<Box<dyn AudioSourceResolver>> {
        Vec::new()
    }

    fn effect_resolvers(&self) -> Vec<Box<dyn EffectResolver>> {
        Vec::new()
    }

    fn audio_source_list_resolvers(&self)
            -> Vec<Box<dyn AudioSourceListResolver>> {
        vec![Box::new(JsonAudioSourceListResolver)]
    }

    fn adapter_resolvers(&self) -> Vec<Box<dyn AdapterResolver>> {
        Vec::new()
    }
}

fn make_json_list_plugin() -> JsonListPlugin {
    JsonListPlugin
}

rambot_api::export_plugin!(make_json_list_plugin);
