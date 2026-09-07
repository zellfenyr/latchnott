use std::fmt;
use std::path::Path;

use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegSetValueExW,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "Latchnott";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutostartError {
    message: String,
}

impl AutostartError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn win32(operation: &str, error: u32) -> Self {
        Self::new(format!("{operation} failed with Win32 error {error}"))
    }
}

impl fmt::Display for AutostartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AutostartError {}

pub struct Autostart;

impl Autostart {
    pub fn enable(executable: &Path) -> Result<(), AutostartError> {
        let command = startup_command(executable)?;
        let key = open_run_key(KEY_SET_VALUE)?;

        let value_name = wide_string(VALUE_NAME);
        let value_data = wide_string(&command);

        let result = unsafe {
            RegSetValueExW(
                key,
                value_name.as_ptr(),
                0,
                REG_SZ,
                value_data.as_ptr() as *const u8,
                (value_data.len() * std::mem::size_of::<u16>()) as u32,
            )
        };

        unsafe {
            RegCloseKey(key);
        }

        if result != 0 {
            return Err(AutostartError::win32("RegSetValueExW", result));
        }

        Ok(())
    }
}

fn open_run_key(access: u32) -> Result<HKEY, AutostartError> {
    let key_path = wide_string(RUN_KEY);
    let mut key: HKEY = std::ptr::null_mut();

    let result = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            key_path.as_ptr(),
            0,
            std::ptr::null_mut(),
            REG_OPTION_NON_VOLATILE,
            access,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        )
    };

    if result != 0 {
        return Err(AutostartError::win32("RegCreateKeyExW", result));
    }

    Ok(key)
}

fn startup_command(executable: &Path) -> Result<String, AutostartError> {
    let executable = executable
        .to_str()
        .ok_or_else(|| AutostartError::new("executable path is not valid UTF-8"))?;

    let quoted = quote_windows_argument(executable);

    if quoted.len() > 260 {
        return Err(AutostartError::new(
            "autostart command exceeds the Windows Run key command-line limit",
        ));
    }

    Ok(quoted)
}

fn quote_windows_argument(value: &str) -> String {
    let escaped = value.replace('"', r#"\""#);

    format!("\"{escaped}\"")
}

fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_executable_path() {
        assert_eq!(
            quote_windows_argument(r"C:\Program Files\Latchnott\latchnott.exe"),
            r#""C:\Program Files\Latchnott\latchnott.exe""#
        );
    }

    #[test]
    fn quotes_path_without_spaces() {
        assert_eq!(
            quote_windows_argument(r"C:\Latchnott\latchnott.exe"),
            r#""C:\Latchnott\latchnott.exe""#
        );
    }

    #[test]
    fn rejects_overly_long_startup_command() {
        let executable = Path::new(
            r"C:\this-is-a-very-long-path\this-is-a-very-long-path\this-is-a-very-long-path\this-is-a-very-long-path\this-is-a-very-long-path\this-is-a-very-long-path\this-is-a-very-long-path\this-is-a-very-long-path\this-is-a-very-long-path\this-is-a-very-long-path\this-is-a-very-long-path\this-is-a-very-long-path\latchnott.exe",
        );

        let result = startup_command(executable);

        assert!(result.is_err());
    }

    #[test]
    fn wide_string_is_null_terminated() {
        let value = wide_string(VALUE_NAME);

        assert_eq!(value.last(), Some(&0));
        assert!(!value.is_empty());
    }
}
