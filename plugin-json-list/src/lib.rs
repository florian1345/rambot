use plugin_commons::FileManager;

use std::collections::VecDeque;
use std::io;

use rambot_api::{
    AudioDocumentation,
    AudioDocumentationBuilder,
    AudioSourceList,
    AudioSourceListResolver,
    Plugin,
    PluginConfig,
    ResolverRegistry
};

struct JsonAudioSourceList {
    audio_sources: VecDeque<String>
}

impl AudioSourceList for JsonAudioSourceList {
    fn next(&mut self) -> Result<Option<String>, io::Error> {
        Ok(self.audio_sources.pop_front())
    }
}

struct JsonAudioSourceListResolver {
    file_manager: FileManager
}

impl AudioSourceListResolver for JsonAudioSourceListResolver {

    fn documentation(&self) -> AudioDocumentation {
        AudioDocumentationBuilder::new()
            .with_name("Json Playlist")
            .with_summary("Load JSON files as playlists.")
            .with_description("Specify the path of a file with the `.json` \
                extension relative to the bot root directory. This plugin \
                will read the given file as a JSON array of strings and \
                provide the individual elements as pieces of a playlist.")
            .build().unwrap()
    }

    fn can_resolve(&self, descriptor: &str) -> bool {
        self.file_manager.is_file_with_extension(descriptor, ".json")
    }

    fn resolve(&self, descriptor: &str)
            -> Result<Box<dyn AudioSourceList + Send>, String> {
        let reader = self.file_manager.open_file_buf(descriptor)?;
        let audio_sources: Vec<String> = serde_json::from_reader(reader)
            .map_err(|e| format!("{}", e))?;

        Ok(Box::new(JsonAudioSourceList {
            audio_sources: VecDeque::from(audio_sources)
        }))
    }
}

struct JsonListPlugin;

impl Plugin for JsonListPlugin {

    fn load_plugin<'registry>(&mut self, config: &PluginConfig,
            registry: &mut ResolverRegistry<'registry>) -> Result<(), String> {
        registry.register_audio_source_list_resolver(
            JsonAudioSourceListResolver {
                file_manager: FileManager::new(config)
            });

        Ok(())
    }
}

fn make_json_list_plugin() -> JsonListPlugin {
    JsonListPlugin
}

rambot_api::export_plugin!(make_json_list_plugin);
