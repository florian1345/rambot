use chrono::offset::Local;

use log::SetLoggerError;

use simplelog::{
    ColorChoice,
    CombinedLogger,
    Config,
    LevelFilter,
    TerminalMode,
    TermLogger,
    WriteLogger
};

use std::fmt::{self, Display, Formatter};
use std::fs::{self, File};
use std::io;
use std::path::Path;

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
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Info,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto
        ),
        WriteLogger::new(
            LevelFilter::Info,
            Config::default(),
            get_file()?
        )
    ])?;

    Ok(())
}
