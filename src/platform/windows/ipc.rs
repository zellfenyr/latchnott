use std::fmt;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE, OPEN_EXISTING, PIPE_ACCESS_INBOUND,
    ReadFile, WriteFile,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE,
    PIPE_TYPE_MESSAGE, PIPE_WAIT,
};

const PIPE_NAME: &str = r"\\.\pipe\Latchnott";
const MAX_COMMAND_SIZE: usize = 64;

const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_PIPE_BUSY: u32 = 231;
const ERROR_PIPE_CONNECTED: u32 = 535;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcCommand {
    ShowInput,
}

impl IpcCommand {
    fn as_bytes(self) -> &'static [u8] {
        b"ShowInput"
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, IpcError> {
        let command = trim_trailing_nul_and_whitespace(bytes);

        if command == b"ShowInput" {
            return Ok(Self::ShowInput);
        }

        Err(IpcError::new("unknown IPC command"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpcError {
    message: String,
}

impl IpcError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn win32(operation: &str, error: u32) -> Self {
        Self::new(format!("{operation} failed with Win32 error {error}"))
    }
}

impl fmt::Display for IpcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for IpcError {}

#[derive(Debug)]
pub struct IpcServer {
    pipe_name: String,
}

impl IpcServer {
    pub fn new() -> Self {
        Self {
            pipe_name: PIPE_NAME.to_owned(),
        }
    }

    pub fn run_until_stopped(
        &self,
        stop: Arc<AtomicBool>,
        ready_sender: mpsc::SyncSender<Result<(), IpcError>>,
        callback: impl Fn(IpcCommand) + Send + 'static,
    ) -> Result<(), IpcError> {
        let mut ready_reported = false;

        loop {
            if stop.load(Ordering::Acquire) {
                break;
            }

            let pipe = self.create_pipe()?;

            if !ready_reported {
                if ready_sender.send(Ok(())).is_err() {
                    unsafe {
                        CloseHandle(pipe);
                    }

                    return Ok(());
                }

                ready_reported = true;
            }

            let result = self.accept_command(pipe);

            unsafe {
                DisconnectNamedPipe(pipe);
                CloseHandle(pipe);
            }

            match result {
                Ok(command) => {
                    if !stop.load(Ordering::Acquire) {
                        callback(command);
                    }
                }
                Err(error) => {
                    if !stop.load(Ordering::Acquire) {
                        return Err(error);
                    }
                }
            }
        }

        if !ready_reported {
            let _ = ready_sender.send(Ok(()));
        }

        Ok(())
    }

    fn create_pipe(&self) -> Result<HANDLE, IpcError> {
        let pipe_name = wide_string(&self.pipe_name);

        let pipe = unsafe {
            CreateNamedPipeW(
                pipe_name.as_ptr(),
                PIPE_ACCESS_INBOUND,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                1,
                MAX_COMMAND_SIZE as u32,
                MAX_COMMAND_SIZE as u32,
                0,
                null(),
            )
        };

        if pipe == INVALID_HANDLE_VALUE {
            let error = unsafe { GetLastError() };

            return Err(IpcError::win32("CreateNamedPipeW", error));
        }

        Ok(pipe)
    }

    fn accept_command(&self, pipe: HANDLE) -> Result<IpcCommand, IpcError> {
        let connected = unsafe { ConnectNamedPipe(pipe, null_mut()) };

        if connected == 0 {
            let error = unsafe { GetLastError() };

            if error != ERROR_PIPE_CONNECTED {
                return Err(IpcError::win32("ConnectNamedPipe", error));
            }
        }

        let mut buffer = [0u8; MAX_COMMAND_SIZE];
        let mut bytes_read = 0u32;

        let success = unsafe {
            ReadFile(
                pipe,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut bytes_read,
                null_mut(),
            )
        };

        if success == 0 {
            let error = unsafe { GetLastError() };

            return Err(IpcError::win32("ReadFile", error));
        }

        IpcCommand::from_bytes(&buffer[..bytes_read as usize])
    }
}

impl Default for IpcServer {
    fn default() -> Self {
        Self::new()
    }
}

pub fn send(command: IpcCommand) -> Result<(), IpcError> {
    send_to_named_pipe(PIPE_NAME, command)
}

fn send_to_named_pipe(pipe_name: &str, command: IpcCommand) -> Result<(), IpcError> {
    let pipe_name = wide_string(pipe_name);

    for _ in 0..10 {
        let handle = unsafe {
            CreateFileW(
                pipe_name.as_ptr(),
                FILE_GENERIC_WRITE,
                0,
                null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                0 as HANDLE,
            )
        };

        if handle != INVALID_HANDLE_VALUE {
            let result = write_command(handle, command);

            unsafe {
                CloseHandle(handle);
            }

            return result;
        }

        let error = unsafe { GetLastError() };

        if error != ERROR_FILE_NOT_FOUND && error != ERROR_PIPE_BUSY {
            return Err(IpcError::win32("CreateFileW", error));
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    Err(IpcError::new(
        "timed out while waiting for Latchnott IPC server",
    ))
}

fn write_command(handle: HANDLE, command: IpcCommand) -> Result<(), IpcError> {
    let bytes = command.as_bytes();
    let mut bytes_written = 0u32;

    let success = unsafe {
        WriteFile(
            handle,
            bytes.as_ptr(),
            bytes.len() as u32,
            &mut bytes_written,
            null_mut(),
        )
    };

    if success == 0 {
        let error = unsafe { GetLastError() };

        return Err(IpcError::win32("WriteFile", error));
    }

    if bytes_written as usize != bytes.len() {
        return Err(IpcError::new("IPC command was only partially written"));
    }

    Ok(())
}

fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn trim_trailing_nul_and_whitespace(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .rposition(|byte| *byte != 0 && !byte.is_ascii_whitespace())
        .map(|index| index + 1)
        .unwrap_or(0);

    &bytes[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_show_input_command() {
        assert_eq!(IpcCommand::ShowInput.as_bytes(), b"ShowInput");
    }

    #[test]
    fn parses_show_input_command() {
        assert_eq!(
            IpcCommand::from_bytes(b"ShowInput").expect("valid command should parse"),
            IpcCommand::ShowInput
        );
    }

    #[test]
    fn parses_show_input_command_with_trailing_whitespace() {
        assert_eq!(
            IpcCommand::from_bytes(b"ShowInput\n").expect("valid command should parse"),
            IpcCommand::ShowInput
        );
    }

    #[test]
    fn rejects_unknown_command() {
        let error =
            IpcCommand::from_bytes(b"SomethingElse").expect_err("unknown command should fail");

        assert_eq!(error.to_string(), "unknown IPC command");
    }

    #[test]
    fn rejects_empty_command() {
        let error = IpcCommand::from_bytes(b"").expect_err("empty command should fail");

        assert_eq!(error.to_string(), "unknown IPC command");
    }

    #[cfg(windows)]
    #[test]
    fn server_receives_show_input_from_client() {
        let unique_pipe_name = format!(r"\\.\pipe\Latchnott-Test-{}", std::process::id());

        let server = IpcServer {
            pipe_name: unique_pipe_name.clone(),
        };

        let stop = Arc::new(AtomicBool::new(false));
        let stop_for_server = Arc::clone(&stop);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);

        let server_thread = std::thread::spawn(move || {
            server.run_until_stopped(stop_for_server, ready_sender, |command| {
                assert_eq!(command, IpcCommand::ShowInput);
            })
        });

        ready_receiver
            .recv()
            .expect("server should report readiness")
            .expect("server should start successfully");

        send_to_named_pipe(&unique_pipe_name, IpcCommand::ShowInput)
            .expect("client should send IPC command");

        stop.store(true, Ordering::Release);

        let result = server_thread.join().expect("server thread should finish");

        result.expect("server should shut down successfully");
    }
}
