use std::fmt;
use std::ptr::null;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows_sys::Win32::System::Threading::CreateMutexW;

const MUTEX_NAME: &str = "Local\\Latchnott";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SingleInstanceError {
    message: String,
    already_running: bool,
}

impl SingleInstanceError {
    fn new(message: impl Into<String>, already_running: bool) -> Self {
        Self {
            message: message.into(),
            already_running,
        }
    }

    fn win32(operation: &str, error: u32) -> Self {
        Self::new(
            format!("{operation} failed with Win32 error {error}"),
            false,
        )
    }

    pub fn is_already_running(&self) -> bool {
        self.already_running
    }
}

impl fmt::Display for SingleInstanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SingleInstanceError {}

#[derive(Debug)]
pub struct SingleInstance {
    handle: HANDLE,
}

impl SingleInstance {
    pub fn acquire() -> Result<Self, SingleInstanceError> {
        Self::acquire_named(MUTEX_NAME)
    }

    fn acquire_named(name: &str) -> Result<Self, SingleInstanceError> {
        let name = wide_string(name);

        let handle = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };

        if handle.is_null() {
            let error = unsafe { GetLastError() };

            return Err(SingleInstanceError::win32("CreateMutexW", error));
        }

        let last_error = unsafe { GetLastError() };

        if last_error == ERROR_ALREADY_EXISTS {
            unsafe {
                CloseHandle(handle);
            }

            return Err(SingleInstanceError::new(
                "another Latchnott instance is already running",
                true,
            ));
        }

        Ok(Self { handle })
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquisition_reports_existing_instance() {
        let name = format!(
            "Local\\Latchnott-Test-{}-{}",
            std::process::id(),
            uuid::Uuid::now_v7()
        );

        let first = SingleInstance::acquire_named(&name).expect("first acquisition should succeed");

        let second = SingleInstance::acquire_named(&name)
            .expect_err("second acquisition should report existing instance");

        assert!(second.is_already_running());
        assert_eq!(
            second.to_string(),
            "another Latchnott instance is already running"
        );

        drop(first);

        let third =
            SingleInstance::acquire_named(&name).expect("acquisition after release should succeed");

        drop(third);
    }

    #[test]
    fn generated_mutex_name_is_null_terminated() {
        let value = wide_string(MUTEX_NAME);

        assert_eq!(value.last(), Some(&0));
        assert!(value.len() > 1);
    }
}
