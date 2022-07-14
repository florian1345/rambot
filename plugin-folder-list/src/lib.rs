use rambot_api::{
    AudioSourceList,
    AudioSourceListResolver,
    AudioSourceResolver,
    EffectResolver,
    Plugin
};

use std::fs::{self, ReadDir};
use std::io::{self, ErrorKind};
use std::path::PathBuf;

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

struct FolderListResolver;

impl AudioSourceListResolver for FolderListResolver {
    fn can_resolve(&self, descriptor: &str) -> bool {
        match fs::metadata(descriptor) {
            Ok(meta) => meta.is_dir(),
            Err(_) => false
        }
    }

    fn resolve(&self, descriptor: &str)
            -> Result<Box<dyn AudioSourceList + Send>, String> {
        let path = PathBuf::from(descriptor);
        let read_dir = fs::read_dir(&path).map_err(|e| format!("{}", e))?;
        Ok(Box::new(FolderList {
            path,
            read_dir
        }))
    }
}

struct FolderListPlugin;

impl Plugin for FolderListPlugin {

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
        vec![Box::new(FolderListResolver)]
    }
}

fn make_folder_list_plugin() -> FolderListPlugin {
    FolderListPlugin
}

rambot_api::export_plugin!(make_folder_list_plugin);
