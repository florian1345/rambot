use chrono::offset::Local;

use log::SetLoggerError;

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
use std::io;
use std::path::Path;
use poise::FrameworkContext;
use serenity::all::FullEvent;

use crate::command::{CommandError, CommandResult};
use crate::command_data::CommandData;
use crate::event::FrameworkEventHandler;

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
///
/// # Arguments
///
/// * `level_filter`: A verbosity level filter that represents the weakest log
///   level that is still logged, or [LevelFilter::Off] to disable logging
///   completely.
pub fn init(level_filter: LevelFilter) -> Result<(), LogInitError> {
    let config = ConfigBuilder::new()
        .add_filter_ignore_str("tracing::span")
        .add_filter_ignore_str("serenity")
        .add_filter_ignore_str("songbird")
        .build();

    CombinedLogger::init(vec![
        TermLogger::new(
            level_filter,
            config.clone(),
            TerminalMode::Mixed,
            ColorChoice::Auto
        ),
        WriteLogger::new(
            level_filter,
            config,
            get_file()?
        )
    ])?;

    Ok(())
}

/// An [EventHandler] that creates log entries for some important events.
pub struct LoggingEventHandler;

#[async_trait::async_trait]
impl EventHandler for LoggingEventHandler {

    async fn guild_create(&self, _ctx: Context, guild: Guild, _is_new: Option<bool>) {
        log::info!("Guild \"{}\" (ID {}) created.", guild.name, guild.id);
    }

    async fn ready(&self, _ctx: Context, data_about_bot: Ready) {
        log::info!("Started session {}.", data_about_bot.session_id);
        log::info!("Running version {}.", data_about_bot.version);
    }

    async fn resume(&self, _ctx: Context, _resumed_event: ResumedEvent) {
        log::info!("Resumed session.");
    }
}

impl FrameworkEventHandler for LoggingEventHandler {
    async fn handle_event(&self, _serenity_ctx: &Context, event: &FullEvent,
            _framework_ctx: FrameworkContext<'_, CommandData, CommandError>)
            -> CommandResult {
        match event {
            FullEvent::GuildCreate { guild, .. } => {
                log::info!("Guild \"{}\" (ID {}) created.", guild.name, guild.id);
            },
            FullEvent::Ready { data_about_bot } => {
                log::info!("Started session {}.", data_about_bot.session_id);
                log::info!("Using API version {}.", data_about_bot.version);
            },
            FullEvent::Resume { .. } => {
                log::info!("Resumed session.");
            },
            _ => { }
        }

        Ok(())
    }
}
