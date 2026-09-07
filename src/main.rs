mod application;
mod configuration;
mod domain;
mod gui;
mod logging;
mod platform;
mod storage;

use std::path::PathBuf;

#[cfg(windows)]
use crate::platform::windows::ipc::{self, IpcCommand};
#[cfg(windows)]
use crate::platform::windows::single_instance::SingleInstance;

fn main() {
    if let Err(error) = run() {
        eprintln!("Latchnott startup error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let configuration = configuration::load_default().map_err(|error| error.to_string())?;

    let data_dir = configuration.data_dir();

    let logger = logging::Logger::new(data_dir)
        .map_err(|error| format!("failed to initialize logging: {error}"))?;

    logger.info("starting Latchnott");

    #[cfg(windows)]
    {
        match SingleInstance::acquire() {
            Ok(_instance) => {
                logger.info("single-instance ownership acquired");

                let storage_path = notes_storage_path(data_dir);

                let result = gui::run(storage_path, configuration.shortcut(), logger);

                if let Err(error) = &result {
                    eprintln!("Latchnott runtime error: {error}");
                }

                return result;
            }
            Err(error) if error.is_already_running() => {
                logger.info("existing Latchnott instance detected");

                ipc::send(IpcCommand::ShowInput).map_err(|error| {
                    format!("failed to notify running Latchnott instance: {error}")
                })?;

                return Ok(());
            }
            Err(error) => {
                return Err(format!("failed to acquire single-instance mutex: {error}"));
            }
        }
    }

    #[cfg(not(windows))]
    {
        let storage_path = notes_storage_path(data_dir);

        gui::run(storage_path, configuration.shortcut(), logger)
    }
}

fn notes_storage_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("notes.json")
}
