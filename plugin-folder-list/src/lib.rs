use rambot_api::{
    AudioDocumentation,
    AudioDocumentationBuilder,
    AudioSourceList,
    AudioSourceListResolver,
    Plugin,
    PluginConfig,
    ResolverRegistry
};

use std::fs::{self, ReadDir};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

struct FolderList {
    path: PathBuf,
    read_dir: ReadDir
}

impl AudioSourceList for FolderList {
    fn next(&mut self) -> Result<Option<String>, io::Error> {
        if let Some(entry) = self.read_dir.next() {
            let entry = entry?;
            self.path.push(entry.file_name());
            let result = self.path.as_os_str().to_owned();
            self.path.pop();
            let result = result.into_string()
                .map_err(|_| io::Error::new(
                    ErrorKind::Other, "file name is not utf-8"))?;

            Ok(Some(result))
        }
        else {
            Ok(None)
        }
    }
}

struct FolderListResolver {
    root: String
}

impl AudioSourceListResolver for FolderListResolver {

    fn documentation(&self) -> AudioDocumentation {
        AudioDocumentationBuilder::new()
            .with_name("Folder Playlist")
            .with_summary("Load directories as playlists.")
            .with_description("Specify the path of a directory containing \
                audio files relative to the bot root directory. This plugin \
                will play all files in the directory as pieces of a playlist.")
            .build().unwrap()
    }

    fn can_resolve(&self, descriptor: &str) -> bool {
        match fs::metadata(descriptor) {
            Ok(meta) => meta.is_dir(),
            Err(_) => false
        }
    }

    fn resolve(&self, descriptor: &str)
            -> Result<Box<dyn AudioSourceList + Send>, String> {
        let path = Path::new(&self.root).join(descriptor);
        let read_dir = fs::read_dir(&path).map_err(|e| format!("{}", e))?;
        Ok(Box::new(FolderList {
            path,
            read_dir
        }))
    }
}

struct FolderListPlugin;

impl Plugin for FolderListPlugin {

    fn load_plugin<'registry>(&self, config: PluginConfig,
            registry: &mut ResolverRegistry<'registry>) -> Result<(), String> {
        registry.register_audio_source_list_resolver(FolderListResolver {
            root: config.root_directory().to_owned()
        });

        Ok(())
    }
}

fn make_folder_list_plugin() -> FolderListPlugin {
    FolderListPlugin
}

rambot_api::export_plugin!(make_folder_list_plugin);
