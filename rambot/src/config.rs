use rambot_api::PluginConfig;

use serde::{Deserialize, Serialize};

use serenity::prelude::TypeMapKey;

use std::env;
use std::fmt::{self, Display, Formatter};
use std::fs::{self, File};
use std::io::{self, BufRead};
use std::path::Path;

const CONFIG_FILE_NAME: &str = "config.json";
const DEFAULT_PREFIX: &str = "!";
const DEFAULT_PLUGIN_DIRECTORY: &str = "plugins";
const DEFAULT_PLUGIN_CONFIG_DIRECTORY: &str = "plugins/config";
const DEFAULT_STATE_DIRECTORY: &str = "state";
const DEFAULT_ALLOW_WEB_ACCESS: bool = true;

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
    plugin_directory: String,
    plugin_config_directory: String,
    state_directory: String,
    root_directory: String,
    allow_web_access: bool
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
            let root_directory = env::current_dir()?
                .as_os_str()
                .to_str()
                .unwrap()
                .to_owned();
            let config = Config {
                prefix: DEFAULT_PREFIX.to_owned(),
                token,
                plugin_directory: DEFAULT_PLUGIN_DIRECTORY.to_owned(),
                plugin_config_directory:
                    DEFAULT_PLUGIN_CONFIG_DIRECTORY.to_owned(),
                state_directory: DEFAULT_STATE_DIRECTORY.to_owned(),
                root_directory,
                allow_web_access: DEFAULT_ALLOW_WEB_ACCESS
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

    /// The path of the directory from which the bot shall load its plugins.
    pub fn plugin_directory(&self) -> &str {
        &self.plugin_directory
    }

    /// The path of the directory in which plugins shall put their specific
    /// config files.
    pub fn plugin_config_directory(&self) -> &str {
        &self.plugin_config_directory
    }

    /// Gets the directory in which persistent state files are placed.
    pub fn state_directory(&self) -> &str {
        &self.state_directory
    }

    /// Gets the [PluginConfig] to pass to a plugin loaded from a file with the
    /// given name.
    ///
    /// # Arguments
    ///
    /// * `library`: The file name (without preceding directories, but with
    /// extension) of the library that contains the plugin for which to
    /// generate a config.
    pub fn generate_plugin_config(&self, library: &str) -> PluginConfig {
        let config_path =
            format!("{}/{}.config", &self.plugin_config_directory, library);

        PluginConfig::new(
            &self.root_directory, self.allow_web_access, config_path)
    }
}

impl TypeMapKey for Config {
    type Value = Config;
}
