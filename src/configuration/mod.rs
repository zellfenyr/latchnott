use std::env;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+Alt+L";
const DEFAULT_DATA_DIRECTORY_NAME: &str = "data";
const APPLICATION_DIRECTORY_NAME: &str = "Latchnott";
const CONFIGURATION_FILE_NAME: &str = "configuration.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configuration {
    shortcut: String,
    data_dir: PathBuf,
    font_family: Option<String>,
}

impl Configuration {
    pub fn shortcut(&self) -> &str {
        &self.shortcut
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn font_family(&self) -> Option<&str> {
        self.font_family.as_deref()
    }
}

#[derive(Debug, Deserialize)]
struct RawConfiguration {
    #[serde(default)]
    hotkey: RawHotkeyConfiguration,
    #[serde(default)]
    storage: RawStorageConfiguration,
    #[serde(default)]
    ui: RawUiConfiguration,
}

#[derive(Debug, Deserialize)]
struct RawHotkeyConfiguration {
    #[serde(default = "default_shortcut")]
    shortcut: String,
}

impl Default for RawHotkeyConfiguration {
    fn default() -> Self {
        Self {
            shortcut: default_shortcut(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct RawStorageConfiguration {
    data_dir: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RawUiConfiguration {
    font_family: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationError {
    message: String,
}

impl ConfigurationError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ConfigurationError {}

pub fn load_default() -> Result<Configuration, ConfigurationError> {
    let path = default_configuration_path()?;

    match std::fs::read_to_string(&path) {
        Ok(_) => load_from_path(&path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            load_from_str("", path.parent().unwrap_or_else(|| Path::new(".")))
        }
        Err(error) => Err(ConfigurationError::new(format!(
            "failed to read configuration '{}': {error}",
            path.display()
        ))),
    }
}

pub fn default_configuration_path() -> Result<PathBuf, ConfigurationError> {
    let executable = env::current_exe().map_err(|error| {
        ConfigurationError::new(format!("failed to determine executable path: {error}"))
    })?;

    let executable_directory = executable.parent().ok_or_else(|| {
        ConfigurationError::new("executable path does not have a parent directory")
    })?;

    Ok(executable_directory.join(CONFIGURATION_FILE_NAME))
}

pub fn load_from_path(path: &Path) -> Result<Configuration, ConfigurationError> {
    let contents = std::fs::read_to_string(path).map_err(|error| {
        ConfigurationError::new(format!(
            "failed to read configuration '{}': {error}",
            path.display()
        ))
    })?;

    load_from_str(&contents, path.parent().unwrap_or_else(|| Path::new(".")))
}

pub fn load_from_str(
    contents: &str,
    configuration_directory: &Path,
) -> Result<Configuration, ConfigurationError> {
    let raw: RawConfiguration = toml::from_str(contents)
        .map_err(|error| ConfigurationError::new(format!("invalid configuration: {error}")))?;

    validate_shortcut(&raw.hotkey.shortcut)?;

    let data_dir = normalize_data_dir(raw.storage.data_dir.as_deref(), configuration_directory)?;

    let font_family = normalize_font_family(raw.ui.font_family)?;

    Ok(Configuration {
        shortcut: raw.hotkey.shortcut,
        data_dir,
        font_family,
    })
}

fn default_shortcut() -> String {
    DEFAULT_SHORTCUT.to_owned()
}

fn validate_shortcut(shortcut: &str) -> Result<(), ConfigurationError> {
    if shortcut.trim().is_empty() {
        return Err(ConfigurationError::new("hotkey.shortcut must not be empty"));
    }

    Ok(())
}

fn normalize_data_dir(
    configured_data_dir: Option<&str>,
    configuration_directory: &Path,
) -> Result<PathBuf, ConfigurationError> {
    let path = match configured_data_dir {
        Some(value) => {
            if value.trim().is_empty() {
                return Err(ConfigurationError::new(
                    "storage.data_dir must not be empty",
                ));
            }

            let path = PathBuf::from(value);

            if path.is_absolute() {
                path
            } else {
                configuration_directory.join(path)
            }
        }
        None => default_data_dir()?,
    };

    Ok(path)
}

fn normalize_font_family(
    font_family: Option<String>,
) -> Result<Option<String>, ConfigurationError> {
    match font_family {
        None => Ok(None),
        Some(value) => {
            let normalized = value.trim().to_owned();

            if normalized.is_empty() {
                return Err(ConfigurationError::new(
                    "ui.font_family must not be empty when specified",
                ));
            }

            Ok(Some(normalized))
        }
    }
}

fn default_data_dir() -> Result<PathBuf, ConfigurationError> {
    #[cfg(windows)]
    {
        let app_data = env::var_os("APPDATA").ok_or_else(|| {
            ConfigurationError::new("APPDATA is not available; cannot determine default data_dir")
        })?;

        Ok(PathBuf::from(app_data)
            .join(APPLICATION_DIRECTORY_NAME)
            .join(DEFAULT_DATA_DIRECTORY_NAME))
    }

    #[cfg(not(windows))]
    {
        let home = env::home_dir()
            .ok_or_else(|| ConfigurationError::new("user home directory is not available"))?;

        Ok(home
            .join(APPLICATION_DIRECTORY_NAME)
            .join(DEFAULT_DATA_DIRECTORY_NAME))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_defaults_for_empty_configuration() {
        let configuration = load_from_str("", Path::new(r"C:\Latchnott"))
            .expect("empty configuration should be valid");

        assert_eq!(configuration.shortcut(), DEFAULT_SHORTCUT);

        let app_data =
            env::var_os("APPDATA").expect("APPDATA should be available on Windows tests");

        let expected_data_dir = PathBuf::from(app_data)
            .join(APPLICATION_DIRECTORY_NAME)
            .join(DEFAULT_DATA_DIRECTORY_NAME);

        assert_eq!(configuration.data_dir(), expected_data_dir);
        assert_eq!(configuration.font_family(), None);
    }

    #[test]
    fn parses_explicit_configuration() {
        let contents = r#"
[hotkey]
shortcut = "Ctrl+Shift+Alt+N"

[storage]
data_dir = "storage"

[ui]
font_family = "Segoe UI"
"#;

        let configuration = load_from_str(contents, Path::new(r"C:\Latchnott"))
            .expect("valid configuration should parse");

        assert_eq!(configuration.shortcut(), "Ctrl+Shift+Alt+N");
        assert_eq!(configuration.data_dir(), Path::new(r"C:\Latchnott\storage"));
        assert_eq!(configuration.font_family(), Some("Segoe UI"));
    }

    #[test]
    fn preserves_absolute_data_dir() {
        let contents = r#"
[storage]
data_dir = 'D:\LatchnottData'
"#;

        let configuration = load_from_str(contents, Path::new(r"C:\Latchnott"))
            .expect("valid absolute data_dir should parse");

        assert_eq!(configuration.data_dir(), Path::new(r"D:\LatchnottData"));
    }

    #[test]
    fn rejects_empty_shortcut() {
        let contents = r#"
[hotkey]
shortcut = ""
"#;

        let error = load_from_str(contents, Path::new(r"C:\Latchnott"))
            .expect_err("empty shortcut should be rejected");

        assert_eq!(error.to_string(), "hotkey.shortcut must not be empty");
    }

    #[test]
    fn rejects_empty_data_dir() {
        let contents = r#"
[storage]
data_dir = ""
"#;

        let error = load_from_str(contents, Path::new(r"C:\Latchnott"))
            .expect_err("empty data_dir should be rejected");

        assert_eq!(error.to_string(), "storage.data_dir must not be empty");
    }

    #[test]
    fn rejects_empty_font_family_when_specified() {
        let contents = r#"
[ui]
font_family = "   "
"#;

        let error = load_from_str(contents, Path::new(r"C:\Latchnott"))
            .expect_err("empty font family should be rejected");

        assert_eq!(
            error.to_string(),
            "ui.font_family must not be empty when specified"
        );
    }

    #[test]
    fn relative_data_dir_is_not_based_on_current_working_directory() {
        let configuration_directory = Path::new(r"D:\Configuration");

        let contents = r#"
[storage]
data_dir = "data"
"#;

        let configuration = load_from_str(contents, configuration_directory)
            .expect("valid configuration should parse");

        assert_eq!(
            configuration.data_dir(),
            Path::new(r"D:\Configuration\data")
        );
    }

    #[test]
    fn trims_font_family() {
        let contents = r#"
[ui]
font_family = "  Segoe UI  "
"#;

        let configuration = load_from_str(contents, Path::new(r"C:\Latchnott"))
            .expect("valid configuration should parse");

        assert_eq!(configuration.font_family(), Some("Segoe UI"));
    }

    #[test]
    fn default_configuration_path_is_next_to_executable() {
        let executable = env::current_exe().expect("current executable path should be available");

        let executable_directory = executable
            .parent()
            .expect("executable path should have a parent directory");

        let expected = executable_directory.join(CONFIGURATION_FILE_NAME);

        assert_eq!(
            default_configuration_path().expect("configuration path should resolve"),
            expected
        );
    }

    #[test]
    fn missing_configuration_uses_defaults() {
        let configuration =
            load_default().expect("missing configuration should fall back to defaults");

        assert_eq!(configuration.shortcut(), DEFAULT_SHORTCUT);
        assert_eq!(configuration.font_family(), None);
    }

    #[test]
    fn loads_configuration_from_file() {
        let directory =
            std::env::temp_dir().join(format!("latchnott-config-test-{}", std::process::id()));

        let path = directory.join(CONFIGURATION_FILE_NAME);

        std::fs::create_dir_all(&directory)
            .expect("test configuration directory should be created");

        std::fs::write(
            &path,
            r#"
[hotkey]
shortcut = "Ctrl+Shift+Alt+T"

[storage]
data_dir = "data"

[ui]
font_family = "Segoe UI"
"#,
        )
        .expect("test configuration should be written");

        let configuration = load_from_path(&path).expect("configuration file should load");

        assert_eq!(configuration.shortcut(), "Ctrl+Shift+Alt+T");
        assert_eq!(configuration.data_dir(), directory.join("data"));
        assert_eq!(configuration.font_family(), Some("Segoe UI"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&directory);
    }
}
