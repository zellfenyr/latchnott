use std::fmt;
use std::ptr::null_mut;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetMessageW, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_HOTKEY, WM_QUIT,
};

const HOTKEY_ID: i32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyError {
    message: String,
}

impl HotkeyError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn win32(operation: &str, error: u32) -> Self {
        Self::new(format!("{operation} failed with Win32 error {error}"))
    }
}

impl fmt::Display for HotkeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for HotkeyError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HotkeyRegistration {
    modifiers: u32,
    virtual_key: u32,
}

pub struct GlobalHotkey {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl GlobalHotkey {
    pub fn start(
        shortcut: &str,
        callback: impl Fn() + Send + 'static,
    ) -> Result<Self, HotkeyError> {
        let registration = parse_shortcut(shortcut)?;

        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);

        let thread = thread::Builder::new()
            .name("latchnott-hotkey".to_owned())
            .spawn(move || {
                hotkey_thread(registration, callback, ready_sender);
            })
            .map_err(|error| HotkeyError::new(format!("failed to spawn hotkey thread: {error}")))?;

        let ready = ready_receiver.recv().map_err(|_| {
            HotkeyError::new("hotkey thread terminated before reporting startup result")
        })?;

        match ready {
            Ok(thread_id) => Ok(Self {
                thread_id,
                thread: Some(thread),
            }),
            Err(error) => {
                let _ = thread.join();
                Err(error)
            }
        }
    }
}

impl Drop for GlobalHotkey {
    fn drop(&mut self) {
        let posted = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };

        if posted == 0 {
            let _ = unsafe { GetLastError() };
        }

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn hotkey_thread(
    registration: HotkeyRegistration,
    callback: impl Fn() + Send + 'static,
    ready_sender: mpsc::SyncSender<Result<u32, HotkeyError>>,
) {
    // Calling PeekMessageW guarantees that this thread owns a message queue.
    // A zero return value only means that no message was available yet.
    let mut bootstrap_message = unsafe { std::mem::zeroed() };

    unsafe {
        PeekMessageW(&mut bootstrap_message, null_mut(), 0, 0, PM_NOREMOVE);
    }

    let thread_id = unsafe { GetCurrentThreadId() };

    let registered = unsafe {
        RegisterHotKey(
            null_mut(),
            HOTKEY_ID,
            registration.modifiers,
            registration.virtual_key,
        )
    };

    if registered == 0 {
        let error = unsafe { GetLastError() };

        let _ = ready_sender.send(Err(HotkeyError::win32("RegisterHotKey", error)));

        return;
    }

    if ready_sender.send(Ok(thread_id)).is_err() {
        unsafe {
            UnregisterHotKey(null_mut(), HOTKEY_ID);
        }

        return;
    }

    loop {
        let mut message = unsafe { std::mem::zeroed() };

        let result = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };

        if result == -1 {
            break;
        }

        if result == 0 {
            break;
        }

        if message.message == WM_HOTKEY && message.wParam == HOTKEY_ID as usize {
            callback();
        }
    }

    unsafe {
        UnregisterHotKey(null_mut(), HOTKEY_ID);
    }
}

fn parse_shortcut(shortcut: &str) -> Result<HotkeyRegistration, HotkeyError> {
    let mut modifiers = MOD_NOREPEAT;
    let mut key: Option<char> = None;

    for part in shortcut.split('+') {
        let token = part.trim();

        if token.is_empty() {
            return Err(HotkeyError::new(
                "hotkey shortcut contains an empty component",
            ));
        }

        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= MOD_CONTROL,
            "shift" => modifiers |= MOD_SHIFT,
            "alt" => modifiers |= MOD_ALT,
            "win" | "windows" | "super" => modifiers |= MOD_WIN,
            _ => {
                if key.is_some() {
                    return Err(HotkeyError::new(
                        "hotkey shortcut must contain exactly one key",
                    ));
                }

                let mut chars = token.chars();

                let character = chars
                    .next()
                    .ok_or_else(|| HotkeyError::new("hotkey shortcut key must not be empty"))?;

                if chars.next().is_some() {
                    return Err(HotkeyError::new(format!("unsupported hotkey key: {token}")));
                }

                if !character.is_ascii_alphanumeric() {
                    return Err(HotkeyError::new(format!("unsupported hotkey key: {token}")));
                }

                key = Some(character.to_ascii_uppercase());
            }
        }
    }

    let key = key.ok_or_else(|| HotkeyError::new("hotkey shortcut must contain a key"))?;

    Ok(HotkeyRegistration {
        modifiers,
        virtual_key: key as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_shortcut() {
        let registration =
            parse_shortcut("Ctrl+Shift+Alt+L").expect("default shortcut should parse");

        assert_eq!(
            registration,
            HotkeyRegistration {
                modifiers: MOD_NOREPEAT | MOD_CONTROL | MOD_SHIFT | MOD_ALT,
                virtual_key: b'L' as u32,
            }
        );
    }

    #[test]
    fn parses_case_insensitive_modifiers() {
        let registration = parse_shortcut("ctrl+SHIFT+alt+n").expect("shortcut should parse");

        assert_eq!(registration.virtual_key, b'N' as u32);

        assert_eq!(
            registration.modifiers,
            MOD_NOREPEAT | MOD_CONTROL | MOD_SHIFT | MOD_ALT
        );
    }

    #[test]
    fn parses_win_modifier() {
        let registration = parse_shortcut("Win+Shift+K").expect("shortcut should parse");

        assert_eq!(registration.modifiers, MOD_NOREPEAT | MOD_WIN | MOD_SHIFT);

        assert_eq!(registration.virtual_key, b'K' as u32);
    }

    #[test]
    fn rejects_shortcut_without_key() {
        let error = parse_shortcut("Ctrl+Shift+Alt").expect_err("shortcut without key should fail");

        assert_eq!(error.to_string(), "hotkey shortcut must contain a key");
    }

    #[test]
    fn rejects_shortcut_with_multiple_keys() {
        let error =
            parse_shortcut("Ctrl+A+B").expect_err("shortcut with multiple keys should fail");

        assert_eq!(
            error.to_string(),
            "hotkey shortcut must contain exactly one key"
        );
    }

    #[test]
    fn rejects_empty_component() {
        let error = parse_shortcut("Ctrl++L").expect_err("empty shortcut component should fail");

        assert_eq!(
            error.to_string(),
            "hotkey shortcut contains an empty component"
        );
    }

    #[test]
    fn rejects_unsupported_key() {
        let error =
            parse_shortcut("Ctrl+F13").expect_err("unsupported multi-character key should fail");

        assert_eq!(error.to_string(), "unsupported hotkey key: F13");
    }

    #[cfg(windows)]
    #[test]
    fn registers_and_unregisters_hotkey() {
        let hotkey = GlobalHotkey::start("Ctrl+Shift+Alt+L", || {})
            .expect("hotkey registration should succeed");

        assert_ne!(hotkey.thread_id, 0);

        drop(hotkey);
    }
}
