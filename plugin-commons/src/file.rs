use rambot_api::PluginConfig;

use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};

use url::Url;

/// A [Read] implementation for a data stream from an HTTP get request.
pub struct WebRead {
    read: Box<dyn Read + Send + Sync + 'static>
}

impl WebRead {
    fn new(read: Box<dyn Read + Send + Sync + 'static>) -> WebRead {
        WebRead { read }
    }
}

impl Read for WebRead {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.read.read(buf)
    }
}

/// A file which could be resolved either locally or on the internet. This is
/// just the descriptor, no data from the file itself is queried. There is also
/// no guarantee that this file (still) exists or has any specific format.
pub enum ResolvedFile {

    /// A local file at the given path.
    Local(PathBuf),

    /// A remote file on the internet behind the given URL.
    Web(Url)
}

/// A local file that has been opened or a file on the internet that is in the
/// process of downloading.
pub enum OpenedFile {

    /// A locally opened [File].
    Local(BufReader<File>),

    /// A file on the internet that is being downloaded by a [WebRead].
    Web(BufReader<WebRead>)
}

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

    /// Gets a reference to the [PluginConfig] used by this file manager. This
    /// is a copy of the config provided in the constructor.
    pub fn config(&self) -> &PluginConfig {
        &self.config
    }

    /// Gets a [ResolvedFile] pointing to the file with the given path either
    /// locally relative to the root directory or, if the [PluginConfig]
    /// provided in the constructor permits it, on the internet.
    pub fn resolve_file(&self, file: &str) -> Option<ResolvedFile> {
        let path = Path::new(self.config.root_directory()).join(file);

        if path.as_path().exists() {
            return Some(ResolvedFile::Local(path));
        }

        if !self.config.allow_web_access() {
            return None;
        }

        match Url::parse(file) {
            Ok(url) => Some(ResolvedFile::Web(url)),
            Err(_) => None
        }
    }

    /// Determines whether the given descriptor is the path or, if the
    /// [PluginConfig] provided in the constructor permits it, the URL of a
    /// file that has the given extension. This is a common operation among
    /// plugins that read files, as it is necessary for the implementation of
    /// various `can_resolve` methods.
    ///
    /// # Arguments
    ///
    /// * `descriptor`: The descriptor to check.
    /// * `extension`: The required extension (including the period) in lower
    /// case.
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

        if file_extension != extension {
            return false;
        }

        self.resolve_file(descriptor).is_some()
    }

    /// Utility function for opening a file and wrapping it in a [BufReader]. Any
    /// error is converted into a string to allow this function to be used inside
    /// various `resolve` methods.
    pub fn open_file_buf(&self, file: &str) -> Result<OpenedFile, String> {
        match self.resolve_file(file) {
            Some(ResolvedFile::Local(path)) => {
                let file = File::open(path).map_err(|e| format!("{}", e))?;
                Ok(OpenedFile::Local(BufReader::new(file)))
            },
            Some(ResolvedFile::Web(url)) => {
                let response = ureq::request_url("GET", &url)
                    .call()
                    .map_err(|e| format!("{}", e))?;
                let web_read = WebRead::new(response.into_reader());
                Ok(OpenedFile::Web(BufReader::new(web_read)))
            },
            None => Err("File not found.".to_owned())
        }
    }
}
