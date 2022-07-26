use rambot_api::PluginConfig;

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// The file manager offers some commonly used functionality for plugins that
/// access the file system. Among other things, it helps with resolving paths
/// relative to the configured root directory.
#[derive(Clone)]
pub struct FileManager {
    config: PluginConfig
}

impl FileManager {

    /// Creates a new file manager from the plugin `config`.
    pub fn new(config: &PluginConfig) -> FileManager {
        FileManager {
            config: config.clone()
        }
    }

    /// Gets [PathBuf] pointing to the file with the given path relative to the
    /// root directory.
    pub fn resolve_file(&self, file: &str) -> PathBuf {
        Path::new(self.config.root_directory()).join(file)
    }

    /// Determines whether the given descriptor is the path of a file that has the
    /// given extension. This is a common operation among plugins that read files,
    /// as it is necessary for the implementation of various `can_resolve` methods.
    ///
    /// # Arguments
    ///
    /// * `descriptor`: The descriptor to check.
    /// * `extension`: The required extension (including the period) in lower case.
    ///
    /// # Returns
    ///
    /// True if and only if the descriptor represents a file with the given
    /// extension.
    pub fn is_file_with_extension(&self, descriptor: &str, extension: &str)
            -> bool {
        if descriptor.len() < extension.len() {
            return false;
        }

        let file_extension = descriptor[(descriptor.len() - extension.len())..]
            .to_lowercase();
        let path = self.resolve_file(descriptor);

        file_extension == extension && path.as_path().exists()
    }

    /// Utility function for opening a file and wrapping it in a [BufReader]. Any
    /// error is converted into a string to allow this function to be used inside
    /// various `resolve` methods.
    pub fn open_file_buf(&self, file: &str) -> Result<BufReader<File>, String> {
        let path = self.resolve_file(file);
        let file = File::open(path).map_err(|e| format!("{}", e))?;
        Ok(BufReader::new(file))
    }
}
