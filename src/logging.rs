use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const LOG_FILE_NAME: &str = "latchnott.log";
const ROTATED_LOG_FILE_NAME: &str = "latchnott.log.1";
const MAX_LOG_SIZE: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

struct LoggerState {
    file: Option<File>,
}

pub struct Logger {
    path: PathBuf,
    state: Mutex<LoggerState>,
}

impl Logger {
    pub fn new(data_dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(data_dir)?;

        let path = data_dir.join(LOG_FILE_NAME);

        let file = OpenOptions::new().create(true).append(true).open(&path)?;

        Ok(Self {
            path,
            state: Mutex::new(LoggerState { file: Some(file) }),
        })
    }

    pub fn info(&self, message: &str) {
        self.log(LogLevel::Info, message);
    }

    pub fn warn(&self, message: &str) {
        self.log(LogLevel::Warn, message);
    }

    pub fn error(&self, message: &str) {
        self.log(LogLevel::Error, message);
    }

    fn log(&self, level: LogLevel, message: &str) {
        let timestamp = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "unknown-time".to_owned());

        let line = format!("[{timestamp}] [{}] {message}\n", level.as_str());

        let Ok(mut state) = self.state.lock() else {
            return;
        };

        if let Err(error) = Self::rotate_if_needed(&self.path, &mut state, line.len() as u64) {
            if let Some(file) = state.file.as_mut() {
                let _ = writeln!(file, "[{timestamp}] [ERROR] failed to rotate log: {error}");
            } else if let Ok(file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
            {
                state.file = Some(file);

                if let Some(file) = state.file.as_mut() {
                    let _ = writeln!(file, "[{timestamp}] [ERROR] failed to rotate log: {error}");
                }
            }

            return;
        }

        let Some(file) = state.file.as_mut() else {
            return;
        };

        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();
    }

    fn rotate_if_needed(
        path: &Path,
        state: &mut LoggerState,
        incoming_size: u64,
    ) -> io::Result<()> {
        let Some(file) = state.file.as_mut() else {
            return Err(io::Error::other("log file is not open"));
        };

        let current_size = file.metadata()?.len();

        if current_size.saturating_add(incoming_size) <= MAX_LOG_SIZE {
            return Ok(());
        }

        file.flush()?;

        let file = state
            .file
            .take()
            .ok_or_else(|| io::Error::other("log file disappeared"))?;

        drop(file);

        let rotated_path = path.with_file_name(ROTATED_LOG_FILE_NAME);

        match fs::remove_file(&rotated_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                state.file = Some(OpenOptions::new().create(true).append(true).open(path)?);

                return Err(error);
            }
        }

        if let Err(error) = fs::rename(path, &rotated_path) {
            state.file = Some(OpenOptions::new().create(true).append(true).open(path)?);

            return Err(error);
        }

        state.file = Some(OpenOptions::new().create(true).append(true).open(path)?);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_data_dir() -> PathBuf {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after UNIX epoch")
            .as_nanos();

        std::env::temp_dir().join(format!(
            "latchnott-logging-test-{}-{timestamp}",
            std::process::id()
        ))
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_file(path.join(LOG_FILE_NAME));
        let _ = fs::remove_file(path.join(ROTATED_LOG_FILE_NAME));
        let _ = fs::remove_dir(path);
    }

    #[test]
    fn creates_log_file() {
        let data_dir = temporary_data_dir();

        let _logger = Logger::new(&data_dir).expect("logger should initialize");

        assert!(data_dir.join(LOG_FILE_NAME).is_file());

        cleanup(&data_dir);
    }

    #[test]
    fn writes_timestamped_log_line() {
        let data_dir = temporary_data_dir();

        let logger = Logger::new(&data_dir).expect("logger should initialize");
        logger.info("startup completed");

        let contents =
            fs::read_to_string(data_dir.join(LOG_FILE_NAME)).expect("log file should be readable");

        assert!(contents.contains("[INFO] startup completed"));
        assert!(contents.contains("T"));

        cleanup(&data_dir);
    }

    #[test]
    fn writes_warning_log_line() {
        let data_dir = temporary_data_dir();

        let logger = Logger::new(&data_dir).expect("logger should initialize");
        logger.warn("empty note input discarded");

        let contents =
            fs::read_to_string(data_dir.join(LOG_FILE_NAME)).expect("log file should be readable");

        assert!(contents.contains("[WARN] empty note input discarded"));

        cleanup(&data_dir);
    }

    #[test]
    fn rotates_log_when_size_limit_is_exceeded() {
        let data_dir = temporary_data_dir();

        let logger = Logger::new(&data_dir).expect("logger should initialize");

        {
            let mut file = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(data_dir.join(LOG_FILE_NAME))
                .expect("log file should be writable");

            let content = vec![b'x'; MAX_LOG_SIZE as usize];

            file.write_all(&content)
                .expect("test log data should be written");
        }

        logger.info("after rotation");

        let current_path = data_dir.join(LOG_FILE_NAME);
        let rotated_path = data_dir.join(ROTATED_LOG_FILE_NAME);

        assert!(rotated_path.is_file());
        assert!(current_path.is_file());

        let current = fs::read_to_string(current_path).expect("current log should be readable");

        assert!(current.contains("after rotation"));
        assert!(
            rotated_path
                .metadata()
                .expect("rotated log should have metadata")
                .len()
                >= MAX_LOG_SIZE
        );

        cleanup(&data_dir);
    }

    #[test]
    fn note_content_is_not_modified_or_injected_by_logger() {
        let data_dir = temporary_data_dir();

        let logger = Logger::new(&data_dir).expect("logger should initialize");
        logger.info("note saved successfully");

        let contents =
            fs::read_to_string(data_dir.join(LOG_FILE_NAME)).expect("log file should be readable");

        assert!(!contents.contains("Test note"));
        assert!(contents.contains("note saved successfully"));

        cleanup(&data_dir);
    }
}
