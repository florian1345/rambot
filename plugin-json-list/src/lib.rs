use plugin_commons::{FileManager, OpenedFile};

use std::collections::VecDeque;
use std::io::{self, Read};

use rambot_api::{
    AudioDocumentation,
    AudioDocumentationBuilder,
    AudioSourceList,
    AudioSourceListResolver,
    Plugin,
    PluginConfig,
    PluginGuildConfig,
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

impl JsonAudioSourceListResolver {
    fn resolve_reader<R>(&self, reader: R)
        -> Result<Box<dyn AudioSourceList + Send>, String>
    where
        R: Read + Send + 'static
    {
        let audio_sources: Vec<String> = serde_json::from_reader(reader)
            .map_err(|e| format!("{}", e))?;

        Ok(Box::new(JsonAudioSourceList {
            audio_sources: VecDeque::from(audio_sources)
        }))
    }
}

impl AudioSourceListResolver for JsonAudioSourceListResolver {

    fn documentation(&self) -> AudioDocumentation {
        let web_descr = if self.file_manager.config().allow_web_access() {
            "Alternatively, a URL to a `.json` file on the internet can be \
                provided. "
        }
        else {
            ""
        };

        AudioDocumentationBuilder::new()
            .with_name("Json Playlist")
            .with_summary("Load JSON files as playlists.")
            .with_description(format!("Specify the path of a file with the \
                `.json` extension relative to the bot root directory. {}This \
                plugin will read the given file as a JSON array of strings \
                and provide the individual elements as pieces of a playlist.",
                web_descr))
            .build().unwrap()
    }

    fn can_resolve(&self, descriptor: &str,
            guild_config: PluginGuildConfig) -> bool {
        self.file_manager.is_file_with_extension(
            descriptor, &guild_config, ".json")
    }

    fn resolve(&self, descriptor: &str, guild_config: PluginGuildConfig)
            -> Result<Box<dyn AudioSourceList + Send>, String> {
        let file = self.file_manager.open_file_buf(descriptor, &guild_config)?;

        match file {
            OpenedFile::Local(reader) => self.resolve_reader(reader),
            OpenedFile::Web(reader) => self.resolve_reader(reader)
        }
    }
}

struct JsonListPlugin;

impl Plugin for JsonListPlugin {

    fn load_plugin<'registry>(&self, config: PluginConfig,
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
