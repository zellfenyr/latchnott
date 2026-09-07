use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, PhysicalPosition, SharedString, WindowPosition};

use crate::application::CreateNote;
use crate::logging::Logger;
use crate::platform::windows::runtime::PlatformRuntime;
use crate::storage::JsonNoteRepository;

slint::include_modules!();

pub fn run(
    storage_path: PathBuf,
    shortcut: &str,
    font_family: Option<&str>,
    logger: Logger,
) -> Result<(), String> {
    let logger = Arc::new(logger);

    logger.info("creating input window");

    let window =
        InputWindow::new().map_err(|error| format!("failed to create input window: {error}"))?;

    window.set_font_family(SharedString::from(font_family.unwrap_or("")));

    let tray =
        LatchnottTray::new().map_err(|error| format!("failed to create system tray: {error}"))?;

    let repository = Rc::new(RefCell::new(
        JsonNoteRepository::new(storage_path)
            .map_err(|error| format!("failed to initialize note storage: {}", error.message()))?,
    ));

    logger.info("note storage initialized");

    let repository_for_save = Rc::clone(&repository);
    let window_for_save = window.as_weak();
    let logger_for_save = Arc::clone(&logger);

    window.on_save_note(move |text: SharedString| {
        let mut repository = repository_for_save.borrow_mut();
        let mut create_note = CreateNote::new(&mut *repository);

        match create_note.execute(text.to_string()) {
            Ok(Some(_note)) => {
                logger_for_save.info("note saved successfully");

                if let Some(window) = window_for_save.upgrade() {
                    window.set_text(SharedString::default());
                    window.set_error_message(SharedString::default());

                    if let Err(error) = window.hide() {
                        logger_for_save
                            .error(&format!("failed to hide input window after save: {error}"));
                    }
                }
            }
            Ok(None) => {
                logger_for_save.info("empty note input discarded");

                if let Some(window) = window_for_save.upgrade() {
                    window.set_text(SharedString::default());
                    window.set_error_message(SharedString::default());

                    if let Err(error) = window.hide() {
                        logger_for_save.error(&format!(
                            "failed to hide input window after empty input: {error}"
                        ));
                    }
                }
            }
            Err(error) => {
                logger_for_save.error(&format!("failed to save note: {}", error.message()));

                if let Some(window) = window_for_save.upgrade() {
                    window.set_error_message("Failed to save note. The note was not saved.".into());
                }
            }
        }
    });

    let window_for_cancel = window.as_weak();
    let logger_for_cancel = Arc::clone(&logger);

    window.on_cancel_note(move || {
        logger_for_cancel.info("note input cancelled");

        if let Some(window) = window_for_cancel.upgrade() {
            window.set_text(SharedString::default());
            window.set_error_message(SharedString::default());

            if let Err(error) = window.hide() {
                logger_for_cancel.error(&format!(
                    "failed to hide input window after cancellation: {error}"
                ));
            }
        }
    });

    let window_for_show_input = window.as_weak();
    let logger_for_show_input = Arc::clone(&logger);

    let show_input = move || {
        let window = window_for_show_input.clone();
        let logger_for_ui = Arc::clone(&logger_for_show_input);

        if let Err(error) = slint::invoke_from_event_loop(move || {
            if let Some(window) = window.upgrade() {
                window.set_text(SharedString::default());
                window.set_error_message(SharedString::default());

                if let Err(error) = window.show() {
                    logger_for_ui.error(&format!("failed to show input window: {error}"));
                    return;
                }

                center_input_window(&window, &logger_for_ui);

                window.invoke_focus_input();
            }
        }) {
            logger_for_show_input.error(&format!(
                "failed to invoke show-input callback on GUI event loop: {error}"
            ));
        }
    };

    let window_for_tray = window.as_weak();
    let logger_for_tray = Arc::clone(&logger);

    tray.on_new_note(move || {
        logger_for_tray.info("new note requested from tray");

        if let Some(window) = window_for_tray.upgrade() {
            window.set_text(SharedString::default());
            window.set_error_message(SharedString::default());

            if let Err(error) = window.show() {
                logger_for_tray.error(&format!("failed to show input window from tray: {error}"));
                return;
            }

            center_input_window(&window, &logger_for_tray);

            window.invoke_focus_input();
        }
    });

    let logger_for_quit = Arc::clone(&logger);

    tray.on_quit(move || {
        logger_for_quit.info("shutdown requested from tray");

        if let Err(error) = slint::quit_event_loop() {
            logger_for_quit.error(&format!("failed to quit GUI event loop: {error}"));
        }
    });

    let _platform_runtime = PlatformRuntime::start(shortcut, Arc::clone(&logger), show_input)
        .map_err(|error| {
            logger.error(&format!("failed to initialize Windows runtime: {error}"));

            format!("failed to initialize Windows runtime: {error}")
        })?;

    logger.info("Windows platform runtime initialized");

    tray.show().map_err(|error| {
        logger.error(&format!("failed to show system tray: {error}"));

        format!("failed to show system tray: {error}")
    })?;

    logger.info("system tray initialized");
    logger.info("Latchnott is ready");

    let result = slint::run_event_loop().map_err(|error| format!("GUI event loop failed: {error}"));

    logger.info("Latchnott GUI event loop stopped");
    logger.info("shutting down Latchnott");

    result
}

fn center_input_window(window: &InputWindow, logger: &Logger) {
    let position = window.window().with_winit_window(|winit_window| {
        let monitor = winit_window
            .current_monitor()
            .or_else(|| winit_window.primary_monitor())?;

        let monitor_position = monitor.position();
        let monitor_size = monitor.size();
        let window_size = winit_window.outer_size();

        let x = monitor_position.x + (monitor_size.width as i32 - window_size.width as i32) / 2;

        let y = monitor_position.y + (monitor_size.height as i32 - window_size.height as i32) / 2;

        Some((x, y))
    });

    match position {
        Some(Some((x, y))) => {
            window
                .window()
                .set_position(WindowPosition::Physical(PhysicalPosition::new(x, y)));
        }
        Some(None) => {
            logger.info("unable to determine monitor for input window; keeping platform position");
        }
        None => {
            logger.info("native Winit window is unavailable; keeping platform position");
        }
    }
}
