# Latchnott

Latchnott — минималистичное resident desktop-приложение для максимально быстрого создания коротких локальных заметок.

Основной сценарий:

```text
Global hotkey → input window → text → Enter → save
```

Проект ориентирован на Windows 11 и распространяется как ZIP без installer.

## MVP

Текущая версия MVP включает:

- resident application;
- system tray;
- single instance;
- local IPC;
- global hotkey `Ctrl + Shift + Alt + L`;
- компактное always-on-top input window;
- autofocus;
- `Enter` для сохранения;
- `Esc` для отмены;
- JSON storage;
- atomic write;
- schema versioning;
- UUIDv7-based note IDs;
- RFC 3339 timestamps;
- `configuration.toml`;
- configurable font family с fallback;
- file logging с простой ротацией;
- Windows user-session autostart;
- GitHub Actions CI;
- Windows ZIP distribution.

В MVP не входят список заметок, поиск, CLI/TUI, Web/API, PostgreSQL, synchronization, installer и auto-update.

## Требования

Для запуска готовой версии требуется Windows 11.

Для разработки требуется Rust 1.98.1 и Cargo.

## Запуск из исходников

Debug-версия:

```powershell
cargo run
```

Проверки:

```powershell
cargo fmt --check
cargo check
cargo test
cargo clippy --all-targets -- -D warnings
```

Release-сборка:

```powershell
cargo build --release
```

Executable:

```text
target\release\latchnott.exe
```

Debug-сборка не регистрирует себя в Windows user-session autostart.

## Использование

После запуска Latchnott работает в фоне и отображается в system tray.

Открыть окно создания заметки можно через:

- `Ctrl + Shift + Alt + L`;
- `New note` в system tray;
- запуск второго экземпляра приложения. Второй экземпляр передаёт `ShowInput` уже работающему процессу и завершает работу.

В окне:

- `Enter` — сохранить заметку;
- `Esc` — отменить ввод;
- потеря фокуса окна сама по себе не закрывает его.

После сохранения или отмены окно скрывается, а resident process продолжает работать.

`Quit` в system tray полностью завершает приложение.

## Конфигурация

Конфигурация редактируется вручную. GUI settings в MVP нет.

Файл:

```text
configuration.toml
```

является локальным пользовательским файлом и не должен коммититься в Git.

В репозитории хранится:

```text
configuration.toml.example
```

Минимальный пример:

```toml
[hotkey]
shortcut = "Ctrl+Shift+Alt+L"
```

Полный пример:

```toml
[hotkey]
shortcut = "Ctrl+Shift+Alt+L"

[storage]
data_dir = "data"

[ui]
font_family = "Segoe UI"
```

Отсутствующие безопасные параметры получают defaults. Явно некорректные значения отклоняются.

Относительный `data_dir` разрешается относительно директории конфигурации. Абсолютный путь сохраняется как есть.

### Font family

Например:

```toml
[ui]
font_family = "Fira Code"
```

Если параметр не указан, используется системный/default font.

Bundled fonts пока не являются частью MVP.

## Хранение данных

По умолчанию:

```text
%APPDATA%\Latchnott\data\
```

Файл заметок:

```text
%APPDATA%\Latchnott\data\notes.json
```

Storage использует schema versioning и atomic write.

При отсутствии файла storage создаётся автоматически.

Повреждённый существующий storage не должен молча заменяться новым пустым файлом.

## Логи

Основной лог:

```text
%APPDATA%\Latchnott\data\latchnott.log
```

При превышении лимита выполняется простая ротация:

```text
latchnott.log
latchnott.log.1
```

Основной лог ограничен примерно 1 MiB и хранится один rotated file.

В логах фиксируются startup/shutdown, configuration, hotkey, IPC и storage diagnostics.

Содержимое заметок в лог не записывается.

## Single instance и IPC

Первый экземпляр становится resident instance:

```text
SingleInstance
    ↓
GUI + tray + hotkey + IPC
```

При повторном запуске:

```text
SingleInstance → already running
                     ↓
             IPC: ShowInput
                     ↓
                  exit
```

IPC реализован через Windows named pipe.

## Autostart

Release-версия регистрирует Latchnott для user-session autostart через:

```text
HKCU\Software\Microsoft\Windows\CurrentVersion\Run
```

Административные права для этого не требуются.

Debug-сборка автоматически в autostart не регистрируется.

## System tray

Tray предоставляет:

```text
New note
Quit
```

`New note` использует уже существующее input window.

## Project structure

Упрощённая структура:

```text
latchnott/
├── .github/
│   └── workflows/
│       └── ci.yml
├── assets/
│   └── tray.svg
├── src/
│   ├── application/
│   ├── configuration/
│   ├── domain/
│   ├── gui/
│   ├── platform/
│   │   └── windows/
│   ├── storage/
│   └── logging.rs
├── ui/
│   └── main.slint
├── .gitignore
├── Cargo.toml
├── Cargo.lock
├── build.rs
├── configuration.toml.example
└── README.md
```

## CI

GitHub Actions выполняет Windows CI:

```text
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

После release build создаётся ZIP:

```text
latchnott-windows-x86_64.zip
```

и загружается как GitHub Actions artifact.

Для CI используется Rust 1.98.1.

## ZIP distribution

MVP распространяется как ZIP.

Типичное содержимое:

```text
latchnott.exe
configuration.toml.example
```

Slint UI и связанные runtime resources компилируются в executable, поэтому отдельные `.slint` файлы не требуются для запуска.

Для локальной конфигурации пользователь может скопировать:

```text
configuration.toml.example
```

в:

```text
configuration.toml
```

`configuration.toml` добавлен в `.gitignore`.

## Development principles

Архитектура разделяет:

```text
Domain
Application
Storage
Configuration
Platform
GUI
```

Windows-specific integration локализована в `platform/windows`.

Главный приоритет:

```text
reliability
→ predictability
→ maintainability
→ simplicity
→ reasonable resource footprint
```

Будущие CLI/TUI, Web/API, PostgreSQL, synchronization и другие GUI adapters не реализуются в MVP и не должны усложнять текущую Windows 11 реализацию.

## License

MIT
