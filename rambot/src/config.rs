use serde::{Deserialize, Serialize};

use serenity::prelude::TypeMapKey;

use std::fmt::{self, Display, Formatter};
use std::fs::{self, File};
use std::io::{self, BufRead};
use std::path::Path;
use std::time::Duration;

const CONFIG_FILE_NAME: &str = "config.json";
const DEFAULT_PREFIX: &str = "!";
const DEFAULT_PLUGIN_PORT: u16 = 46085;
const DEFAULT_PLUGIN_DIRECTORY: &str = "plugins";
const DEFAULT_REGISTRATION_TIMEOUT_SECONDS: u64 = 10;
const DEFAULT_AUDIO_SOURCE_PREFIX: char = ':';

/// An enumeration of the different errors that can occur when loading the
/// configuration.
pub enum ConfigError {

    /// Indicates that the path of the config file is currently occupied by a
    /// directory of the same name.
    OccupiedByDirectory,

    /// Wraps an [IO error](std::io::Error) that occurred while loading or
    /// saving the file.
    IOError(io::Error),

    /// Wraps a [JSON error](serde_json::Error) that occurred during
    /// serialization or deserialization of the configuration file.
    JSONError(serde_json::Error)
}

impl From<io::Error> for ConfigError {
    fn from(e: io::Error) -> ConfigError {
        ConfigError::IOError(e)
    }
}

impl From<serde_json::Error> for ConfigError {
    fn from(e: serde_json::Error) -> ConfigError {
        ConfigError::JSONError(e)
    }
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::OccupiedByDirectory =>
                write!(f,
                    "The config file name ({}) is occupied by a directory.",
                    CONFIG_FILE_NAME),
            ConfigError::IOError(e) => write!(f, "{}", e),
            ConfigError::JSONError(e) =>
                write!(f, "Error while parsing the configuration file: {}", e)
        }
    }
}

/// The configuration data of the bot.
#[derive(Deserialize, Serialize)]
pub struct Config {
    prefix: String,
    token: String,
    plugin_port: u16,
    plugin_directory: String,
    registration_timeout_seconds: u64,
    audio_source_prefix: char
}

impl Config {

    /// Loads the config file or, if it is not present, creates a default
    /// config and stores it to the file.
    pub fn load() -> Result<Config, ConfigError> {
        let path = Path::new(CONFIG_FILE_NAME);

        if path.is_dir() {
            Err(ConfigError::OccupiedByDirectory)
        }
        else if path.is_file() {
            let json = fs::read_to_string(path)?;
            Ok(serde_json::from_str(&json)?)
        }
        else {
            log::info!("No config file was found. A new one will be created.");
            println!("Please specify the Discord API token below.");

            let stdin = io::stdin();
            let token = stdin.lock().lines().next().unwrap()?;
            let config = Config {
                prefix: DEFAULT_PREFIX.to_owned(),
                token,
                plugin_port: DEFAULT_PLUGIN_PORT,
                plugin_directory: DEFAULT_PLUGIN_DIRECTORY.to_owned(),
                registration_timeout_seconds:
                    DEFAULT_REGISTRATION_TIMEOUT_SECONDS,
                audio_source_prefix: DEFAULT_AUDIO_SOURCE_PREFIX
            };
            let file = File::create(path)?;
            serde_json::to_writer(file, &config)?;

            Ok(config)
        }
    }

    /// The prefix for commands to be recognized by the bot.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// The Discord API token that the bot uses to connect to Discord.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The port on which plugins can connect to the bot.
    pub fn plugin_port(&self) -> u16 {
        self.plugin_port
    }

    /// The path of the directory from which the bot shall load its plugins.
    pub fn plugin_directory(&self) -> &str {
        &self.plugin_directory
    }

    /// The [Duration] to wait for registration of plugins before aborting.
    pub fn registration_timeout(&self) -> Duration {
        Duration::from_secs(self.registration_timeout_seconds)
    }

    /// The character which prefixes an explicit audio source type definition
    /// by the user (e.g. `:ogg file.ogg` would be the code if this char is `:`
    /// and the user wants to specify the audio source type of the name `ogg`).
    pub fn audio_source_prefix(&self) -> char {
        self.audio_source_prefix
    }
}

impl TypeMapKey for Config {
    type Value = Config;
}
