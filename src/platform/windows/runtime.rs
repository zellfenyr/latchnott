use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};

use crate::logging::Logger;

use super::hotkey::{GlobalHotkey, HotkeyError};
use super::ipc::{IpcCommand, IpcError, IpcServer};

#[derive(Debug)]
pub enum PlatformRuntimeError {
    Hotkey(HotkeyError),
    Ipc(IpcError),
    Thread(String),
}

impl fmt::Display for PlatformRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hotkey(error) => write!(formatter, "{error}"),
            Self::Ipc(error) => write!(formatter, "{error}"),
            Self::Thread(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for PlatformRuntimeError {}

impl From<HotkeyError> for PlatformRuntimeError {
    fn from(error: HotkeyError) -> Self {
        Self::Hotkey(error)
    }
}

impl From<IpcError> for PlatformRuntimeError {
    fn from(error: IpcError) -> Self {
        Self::Ipc(error)
    }
}

pub struct PlatformRuntime {
    hotkey: Option<GlobalHotkey>,
    stop: Arc<AtomicBool>,
    ipc_thread: Option<JoinHandle<()>>,
}

impl PlatformRuntime {
    pub fn start(
        shortcut: &str,
        logger: Arc<Logger>,
        on_show_input: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, PlatformRuntimeError> {
        let callback = Arc::new(on_show_input);

        let hotkey_callback = Arc::clone(&callback);

        let hotkey = GlobalHotkey::start(shortcut, move || {
            hotkey_callback();
        })?;

        logger.info("global hotkey registered");

        let stop = Arc::new(AtomicBool::new(false));
        let stop_for_thread = Arc::clone(&stop);
        let ipc_callback = Arc::clone(&callback);
        let logger_for_thread = Arc::clone(&logger);

        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);

        let ipc_thread = thread::Builder::new()
            .name("latchnott-ipc".to_owned())
            .spawn(move || {
                let server = IpcServer::new();

                logger_for_thread.info("IPC server thread started");

                if let Err(error) =
                    server.run_until_stopped(stop_for_thread, ready_sender, move |command| {
                        match command {
                            IpcCommand::ShowInput => {
                                ipc_callback();
                            }
                        }
                    })
                {
                    logger_for_thread.error(&format!("IPC server stopped with error: {error}"));
                }

                logger_for_thread.info("IPC server thread stopped");
            })
            .map_err(|error| {
                PlatformRuntimeError::Thread(format!("failed to spawn IPC thread: {error}"))
            })?;

        let ready = ready_receiver.recv().map_err(|_| {
            PlatformRuntimeError::Thread(
                "IPC thread terminated before reporting readiness".to_owned(),
            )
        })?;

        if let Err(error) = ready {
            stop.store(true, Ordering::Release);

            let _ = super::ipc::send(IpcCommand::ShowInput);
            let _ = ipc_thread.join();

            return Err(PlatformRuntimeError::Ipc(error));
        }

        logger.info("IPC server initialized");

        Ok(Self {
            hotkey: Some(hotkey),
            stop,
            ipc_thread: Some(ipc_thread),
        })
    }
}

impl Drop for PlatformRuntime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);

        let _ = super::ipc::send(IpcCommand::ShowInput);

        if let Some(thread) = self.ipc_thread.take() {
            let _ = thread.join();
        }

        drop(self.hotkey.take());
    }
}
