use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use slint::{ComponentHandle, SharedString};

use crate::application::CreateNote;
use crate::platform::windows::runtime::PlatformRuntime;
use crate::storage::JsonNoteRepository;

slint::include_modules!();

pub fn run(storage_path: PathBuf, shortcut: &str) -> Result<(), String> {
    let window =
        InputWindow::new().map_err(|error| format!("failed to create input window: {error}"))?;

    let tray =
        LatchnottTray::new().map_err(|error| format!("failed to create system tray: {error}"))?;

    let repository = Rc::new(RefCell::new(
        JsonNoteRepository::new(storage_path)
            .map_err(|error| format!("failed to initialize note storage: {}", error.message()))?,
    ));

    let repository_for_save = Rc::clone(&repository);
    let window_for_save = window.as_weak();

    window.on_save_note(move |text: SharedString| {
        let mut repository = repository_for_save.borrow_mut();
        let mut create_note = CreateNote::new(&mut *repository);

        match create_note.execute(text.to_string()) {
            Ok(Some(_note)) => {
                if let Some(window) = window_for_save.upgrade() {
                    window.set_text(SharedString::default());
                    window.set_error_message(SharedString::default());

                    if let Err(error) = window.hide() {
                        eprintln!("Latchnott GUI error while hiding input window: {error}");
                    }
                }
            }
            Ok(None) => {}
            Err(error) => {
                eprintln!("Latchnott storage error: {}", error.message());

                if let Some(window) = window_for_save.upgrade() {
                    window.set_error_message("Failed to save note. The note was not saved.".into());
                }
            }
        }
    });

    let window_for_show_input = window.as_weak();

    let show_input = move || {
        let window = window_for_show_input.clone();

        if let Err(error) = slint::invoke_from_event_loop(move || {
            if let Some(window) = window.upgrade() {
                window.set_text(SharedString::default());
                window.set_error_message(SharedString::default());

                if let Err(error) = window.show() {
                    eprintln!("Latchnott GUI error while showing input window: {error}");
                    return;
                }

                window.invoke_focus_input();
            }
        }) {
            eprintln!("Latchnott GUI event-loop error: {error}");
        }
    };

    let window_for_tray = window.as_weak();

    tray.on_new_note(move || {
        if let Some(window) = window_for_tray.upgrade() {
            window.set_text(SharedString::default());
            window.set_error_message(SharedString::default());

            if let Err(error) = window.show() {
                eprintln!("Latchnott GUI error while showing input window: {error}");
                return;
            }

            window.invoke_focus_input();
        }
    });

    tray.on_quit(|| {
        if let Err(error) = slint::quit_event_loop() {
            eprintln!("Latchnott GUI error while quitting: {error}");
        }
    });

    let _platform_runtime = PlatformRuntime::start(shortcut, show_input)
        .map_err(|error| format!("failed to initialize Windows runtime: {error}"))?;

    tray.show()
        .map_err(|error| format!("failed to show system tray: {error}"))?;

    slint::run_event_loop().map_err(|error| format!("GUI event loop failed: {error}"))?;

    Ok(())
}
