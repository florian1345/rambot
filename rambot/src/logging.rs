use chrono::offset::Local;

use log::SetLoggerError;

use serde_json::Value;

use serenity::client::{EventHandler, Context};
use serenity::model::event::ResumedEvent;
use serenity::model::guild::Guild;
use serenity::model::prelude::Ready;

use simplelog::{
    ColorChoice,
    CombinedLogger,
    ConfigBuilder,
    LevelFilter,
    TerminalMode,
    TermLogger,
    WriteLogger
};

use std::fmt::{self, Display, Formatter};
use std::fs::{self, File};
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;

const LOG_DIR: &str = "logs";

/// An enumeration of the different errors that may occur while setting up the
/// logger.
pub enum LogInitError {

    /// Indicates that the `logs` directory is occupied by a file of the same
    /// name.
    OccupiedByFile,

    /// A wrapper for an IO-error that was raised during creation of the `logs`
    /// directory or the log file.
    IOError(io::Error),

    /// A wrapper for a [SetLoggerError].
    SetLoggerError(SetLoggerError)
}

impl From<io::Error> for LogInitError {
    fn from(e: io::Error) -> LogInitError {
        LogInitError::IOError(e)
    }
}

impl From<SetLoggerError> for LogInitError {
    fn from(e: SetLoggerError) -> LogInitError {
        LogInitError::SetLoggerError(e)
    }
}

impl Display for LogInitError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            LogInitError::OccupiedByFile =>
                write!(f,
                    "The log directory ({}) is occupied by a file of the same \
                    name.", LOG_DIR),
            LogInitError::IOError(e) => write!(f, "{}", e),
            LogInitError::SetLoggerError(e) => write!(f, "{}", e)
        }
    }
}

fn get_file() -> Result<File, LogInitError> {
    let dir = Path::new(LOG_DIR);

    if dir.is_file() {
        return Err(LogInitError::OccupiedByFile);
    }
    else if !dir.exists() {
        fs::create_dir(dir)?;
    }

    let now = Local::now();
    let base_path = format!("{}/{}", LOG_DIR, now.format("%Y-%m-%d-%H-%M-%S"));
    let naive_path = format!("{}.log", &base_path);
    let mut file = None;

    if Path::new(&naive_path).exists() {
        for i in 0u64.. {
            let path = format!("{}-{}.log", &base_path, i);

            if !Path::new(&path).exists() {
                file = Some(File::create(&path)?);
                break;
            }
        }
    }
    else {
        file = Some(File::create(&naive_path)?);
    }

    Ok(file.unwrap())
}

/// Initializes a logger that writes to the terminal as well as to a log file
/// named after the current time in the `logs` directory.
pub fn init() -> Result<(), LogInitError> {
    let config = ConfigBuilder::new()
        .add_filter_ignore_str("tracing::span")
        .add_filter_ignore_str("serenity")
        .add_filter_ignore_str("songbird")
        .build();

    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Info,
            config.clone(),
            TerminalMode::Mixed,
            ColorChoice::Auto
        ),
        WriteLogger::new(
            LevelFilter::Info,
            config,
            get_file()?
        )
    ])?;

    Ok(())
}

/// An [EventHandler] that creates log entries for some important events.
pub struct LoggingEventHandler;

impl EventHandler for LoggingEventHandler {

    fn guild_create<'life0, 'async_trait>(&'life0 self, _ctx: Context,
        guild: Guild, _is_new: bool)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait
    {
        log::info!("Guild \"{}\" (ID {}) created.", guild.name, guild.id);

        Box::pin(async { })
    }

    fn ready<'life0, 'async_trait>(&'life0 self, _ctx: Context,
        data_about_bot: Ready)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait
    {
        log::info!("Started session {}.", data_about_bot.session_id);
        log::info!("Running version {}.", data_about_bot.version);

        Box::pin(async { })
    }

    fn resume<'life0, 'async_trait>(&'life0 self, _ctx: Context,
        _resumed_event: ResumedEvent)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait
    {
        log::info!("Resumed session.");

        Box::pin(async { })
    }

    fn unknown<'life0, 'async_trait>(&'life0 self, _ctx: Context,
        name: String, _raw: Value)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait
    {
        log::warn!("Unknown event of name \"{}\".", name);

        Box::pin(async { })
    }
}
