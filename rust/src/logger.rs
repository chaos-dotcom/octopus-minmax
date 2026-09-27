//! Logging in the exact shape the Python implementation produces.
//!
//! `src/logger.py` configures the `octobot` logger with two handlers:
//!   * a rotating file handler at `logs/octobot.log` with the detailed format
//!     `%(asctime)s - %(name)s - %(levelname)s - %(module)s.%(funcName)s - %(message)s`
//!   * a console (stderr) handler at INFO with `%(asctime)s - %(levelname)s - %(message)s`
//!
//! `%(asctime)s` uses `datefmt="%Y-%m-%d %H:%M:%S"` in local time.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

impl Level {
    fn name(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warning => "WARNING",
            Level::Error => "ERROR",
            Level::Critical => "CRITICAL",
        }
    }
}

const MAX_BYTES: u64 = 10 * 1024 * 1024;
const BACKUP_COUNT: u32 = 5;

struct Logger {
    file: Option<File>,
    path: PathBuf,
    written: u64,
}

static LOGGER: OnceLock<Mutex<Logger>> = OnceLock::new();

fn logger() -> &'static Mutex<Logger> {
    LOGGER.get_or_init(|| {
        let dir = PathBuf::from("logs");
        if !dir.exists() {
            let _ = fs::create_dir_all(&dir);
        }
        let path = dir.join("octobot.log");
        let file = OpenOptions::new().create(true).append(true).open(&path).ok();
        let written = file.as_ref().and_then(|handle| handle.metadata().ok()).map(|meta| meta.len()).unwrap_or(0);
        Mutex::new(Logger { file, path, written })
    })
}

/// `logging.RotatingFileHandler.shouldRollover()`
fn rotate_if_needed(state: &mut Logger, incoming: usize) {
    if state.written + (incoming as u64) < MAX_BYTES {
        return;
    }
    state.file = None;
    for index in (1..=BACKUP_COUNT).rev() {
        let source = if index == 1 {
            state.path.clone()
        } else {
            PathBuf::from(format!("{}.{}", state.path.display(), index - 1))
        };
        let target = PathBuf::from(format!("{}.{}", state.path.display(), index));
        if source.exists() {
            let _ = fs::remove_file(&target);
            let _ = fs::rename(&source, &target);
        }
    }
    state.file = OpenOptions::new().create(true).append(true).open(&state.path).ok();
    state.written = 0;
}

/// Create `logs/` and open the log file, like `setup_logging()` does at import time.
pub fn initialise() {
    let _ = logger();
}

fn timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Emit one record.  `name` is the Python logger name (`octobot.query_service`),
/// `location` is `%(module)s.%(funcName)s` (`query_service._get_token`).
pub fn log(level: Level, name: &str, location: &str, message: &str) {
    let stamp = timestamp();
    let detailed = format!("{} - {} - {} - {} - {}\n", stamp, name, level.name(), location, message);
    let simple = format!("{} - {} - {}\n", stamp, level.name(), message);

    {
        let mut state = logger().lock().unwrap();
        if level >= Level::Debug {
            rotate_if_needed(&mut state, detailed.len());
            if let Some(file) = state.file.as_mut() {
                if file.write_all(detailed.as_bytes()).is_ok() {
                    state.written += detailed.len() as u64;
                }
            }
        }
    }
    if level >= Level::Info {
        let _ = std::io::stderr().write_all(simple.as_bytes());
        let _ = std::io::stderr().flush();
    }
}

#[macro_export]
macro_rules! logd {
    ($name:expr, $location:expr, $($arg:tt)*) => {
        $crate::logger::log($crate::logger::Level::Debug, $name, $location, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! logi {
    ($name:expr, $location:expr, $($arg:tt)*) => {
        $crate::logger::log($crate::logger::Level::Info, $name, $location, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! logw {
    ($name:expr, $location:expr, $($arg:tt)*) => {
        $crate::logger::log($crate::logger::Level::Warning, $name, $location, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! loge {
    ($name:expr, $location:expr, $($arg:tt)*) => {
        $crate::logger::log($crate::logger::Level::Error, $name, $location, &format!($($arg)*))
    };
}
