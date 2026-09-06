use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::{Uuid, Version};

use crate::domain::Note;

/// Errors returned by note repositories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryError {
    message: String,
}

impl RepositoryError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<std::io::Error> for RepositoryError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}

impl From<serde_json::Error> for RepositoryError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(error.to_string())
    }
}

/// Persistence boundary for notes.
pub trait NoteRepository {
    fn save(&mut self, note: Note) -> Result<(), RepositoryError>;
}

const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct StorageFile {
    schema_version: u32,
    entries: Vec<StorageEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct StorageEntry {
    id: String,
    text: String,
    created_at: String,
}

pub struct JsonNoteRepository {
    path: PathBuf,
    storage: StorageFile,
}

impl JsonNoteRepository {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, RepositoryError> {
        let path = path.into();

        let storage = match fs::read_to_string(&path) {
            Ok(contents) => Self::parse_storage(&contents)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => StorageFile {
                schema_version: CURRENT_SCHEMA_VERSION,
                entries: Vec::new(),
            },
            Err(error) => return Err(error.into()),
        };

        Ok(Self { path, storage })
    }

    fn parse_storage(contents: &str) -> Result<StorageFile, RepositoryError> {
        let storage: StorageFile = serde_json::from_str(contents)?;

        if storage.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(RepositoryError::new(format!(
                "unsupported storage schema version: {}",
                storage.schema_version
            )));
        }

        for entry in &storage.entries {
            entry_to_note(entry)?;
        }

        Ok(storage)
    }

    fn persist(&self, storage: &StorageFile) -> Result<(), RepositoryError> {
        let parent = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));

        fs::create_dir_all(parent)?;

        let temporary_path = parent.join(format!(
            ".{}.{}.tmp",
            self.path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("latchnott"),
            Uuid::now_v7()
        ));

        let result = (|| -> Result<(), RepositoryError> {
            let file = File::create(&temporary_path)?;

            Self::write_storage(file, storage)?;

            fs::rename(&temporary_path, &self.path)?;

            Ok(())
        })();

        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }

        result
    }

    fn write_storage(mut file: File, storage: &StorageFile) -> Result<(), RepositoryError> {
        serde_json::to_writer_pretty(&mut file, storage)?;
        file.write_all(b"\n")?;
        file.sync_all()?;

        Ok(())
    }
}

impl NoteRepository for JsonNoteRepository {
    fn save(&mut self, note: Note) -> Result<(), RepositoryError> {
        let entry = StorageEntry {
            id: note.id().to_string(),
            text: note.text().to_owned(),
            created_at: note
                .created_at()
                .format(&Rfc3339)
                .map_err(|error| RepositoryError::new(error.to_string()))?,
        };

        let mut next_storage = self.storage.clone();
        next_storage.entries.push(entry);

        self.persist(&next_storage)?;

        self.storage = next_storage;

        Ok(())
    }
}

fn entry_to_note(entry: &StorageEntry) -> Result<Note, RepositoryError> {
    let id = Uuid::parse_str(&entry.id)
        .map_err(|error| RepositoryError::new(format!("invalid note id: {error}")))?;

    if id.get_version() != Some(Version::SortRand) {
        return Err(RepositoryError::new(format!(
            "note id is not UUIDv7: {}",
            entry.id
        )));
    }

    let created_at = OffsetDateTime::parse(&entry.created_at, &Rfc3339)
        .map_err(|error| RepositoryError::new(format!("invalid created_at: {error}")))?;

    Note::from_parts(id, entry.text.clone(), created_at)
        .ok_or_else(|| RepositoryError::new("storage contains an empty or whitespace-only note"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_storage_path() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after UNIX epoch")
            .as_nanos();

        std::env::temp_dir().join(format!(
            "latchnott-storage-test-{}-{timestamp}.json",
            std::process::id()
        ))
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_file(path);
    }

    fn temporary_file_pattern(path: &Path) -> String {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("test storage path should have a valid file name");

        format!(".{file_name}.")
    }

    fn count_temporary_files(path: &Path) -> usize {
        let prefix = temporary_file_pattern(path);

        fs::read_dir(path.parent().unwrap())
            .expect("temporary directory should be readable")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_name().to_string_lossy().starts_with(&prefix)
                    && entry.file_name().to_string_lossy().ends_with(".tmp")
            })
            .count()
    }

    #[test]
    fn creates_empty_storage_when_file_is_missing() {
        let path = temporary_storage_path();

        let repository =
            JsonNoteRepository::new(&path).expect("missing storage should start empty");

        assert_eq!(
            repository.storage,
            StorageFile {
                schema_version: 1,
                entries: Vec::new(),
            }
        );

        cleanup(&path);
    }

    #[test]
    fn saves_note_to_schema_v1_json() {
        let path = temporary_storage_path();

        let mut repository =
            JsonNoteRepository::new(&path).expect("missing storage should start empty");

        let created_at = OffsetDateTime::now_utc();
        let note = Note::from_parts(Uuid::now_v7(), "Persisted note".to_owned(), created_at)
            .expect("valid note should be created");

        repository
            .save(note.clone())
            .expect("saving note should succeed");

        let contents = fs::read_to_string(&path).expect("storage file should exist");

        let storage: StorageFile =
            serde_json::from_str(&contents).expect("storage should contain valid JSON");

        assert_eq!(storage.schema_version, 1);
        assert_eq!(storage.entries.len(), 1);
        assert_eq!(storage.entries[0].id, note.id().to_string());
        assert_eq!(storage.entries[0].text, note.text());
        assert_eq!(
            storage.entries[0].created_at,
            note.created_at()
                .format(&Rfc3339)
                .expect("timestamp should format as RFC3339")
        );

        cleanup(&path);
    }

    #[test]
    fn loads_existing_note() {
        let path = temporary_storage_path();

        let id = Uuid::now_v7();
        let created_at = OffsetDateTime::now_utc();

        let storage = StorageFile {
            schema_version: 1,
            entries: vec![StorageEntry {
                id: id.to_string(),
                text: "Existing note".to_owned(),
                created_at: created_at
                    .format(&Rfc3339)
                    .expect("timestamp should format as RFC3339"),
            }],
        };

        let contents =
            serde_json::to_string_pretty(&storage).expect("test storage should serialize");

        fs::write(&path, contents).expect("test storage should be written");

        let repository =
            JsonNoteRepository::new(&path).expect("existing storage should load successfully");

        assert_eq!(repository.storage.entries.len(), 1);

        let restored = entry_to_note(&repository.storage.entries[0])
            .expect("stored entry should convert into a note");

        assert_eq!(restored.id(), id);
        assert_eq!(restored.text(), "Existing note");
        assert_eq!(restored.created_at(), created_at);

        cleanup(&path);
    }

    #[test]
    fn rejects_corrupted_json() {
        let path = temporary_storage_path();

        let corrupted = "{ definitely not valid json";
        fs::write(&path, corrupted).expect("corrupted test storage should be written");

        let result = JsonNoteRepository::new(&path);

        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(&path).expect("original storage should remain readable"),
            corrupted
        );

        cleanup(&path);
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let path = temporary_storage_path();

        let storage = StorageFile {
            schema_version: 99,
            entries: Vec::new(),
        };

        let contents =
            serde_json::to_string_pretty(&storage).expect("test storage should serialize");

        fs::write(&path, contents).expect("test storage should be written");

        let result = JsonNoteRepository::new(&path);

        assert!(result.is_err());

        cleanup(&path);
    }

    #[test]
    fn rejects_invalid_uuid() {
        let path = temporary_storage_path();

        let storage = StorageFile {
            schema_version: 1,
            entries: vec![StorageEntry {
                id: "not-a-uuid".to_owned(),
                text: "Invalid note".to_owned(),
                created_at: OffsetDateTime::now_utc()
                    .format(&Rfc3339)
                    .expect("timestamp should format"),
            }],
        };

        let contents =
            serde_json::to_string_pretty(&storage).expect("test storage should serialize");

        fs::write(&path, contents).expect("test storage should be written");

        let result = JsonNoteRepository::new(&path);

        assert!(result.is_err());

        cleanup(&path);
    }

    #[test]
    fn rejects_non_v7_uuid() {
        let path = temporary_storage_path();

        let storage = StorageFile {
            schema_version: 1,
            entries: vec![StorageEntry {
                id: "550e8400-e29b-41d4-a716-446655440000".to_owned(),
                text: "Non-v7 note".to_owned(),
                created_at: OffsetDateTime::now_utc()
                    .format(&Rfc3339)
                    .expect("timestamp should format"),
            }],
        };

        let contents =
            serde_json::to_string_pretty(&storage).expect("test storage should serialize");

        fs::write(&path, contents).expect("test storage should be written");

        let result = JsonNoteRepository::new(&path);

        assert!(result.is_err());

        cleanup(&path);
    }

    #[test]
    fn rejects_invalid_timestamp() {
        let path = temporary_storage_path();

        let storage = StorageFile {
            schema_version: 1,
            entries: vec![StorageEntry {
                id: Uuid::now_v7().to_string(),
                text: "Invalid timestamp".to_owned(),
                created_at: "not-a-timestamp".to_owned(),
            }],
        };

        let contents =
            serde_json::to_string_pretty(&storage).expect("test storage should serialize");

        fs::write(&path, contents).expect("test storage should be written");

        let result = JsonNoteRepository::new(&path);

        assert!(result.is_err());

        cleanup(&path);
    }

    #[test]
    fn successful_save_replaces_existing_storage_atomically() {
        let path = temporary_storage_path();

        let first_note = Note::new("First note".to_owned(), OffsetDateTime::now_utc())
            .expect("first note should be valid");

        let second_note = Note::new("Second note".to_owned(), OffsetDateTime::now_utc())
            .expect("second note should be valid");

        let mut repository =
            JsonNoteRepository::new(&path).expect("missing storage should start empty");

        repository
            .save(first_note.clone())
            .expect("first save should succeed");

        assert_eq!(count_temporary_files(&path), 0);

        repository
            .save(second_note.clone())
            .expect("second save should succeed");

        let reloaded =
            JsonNoteRepository::new(&path).expect("replaced storage should remain valid");

        assert_eq!(reloaded.storage.entries.len(), 2);
        assert_eq!(reloaded.storage.entries[0].id, first_note.id().to_string());
        assert_eq!(reloaded.storage.entries[1].id, second_note.id().to_string());
        assert_eq!(count_temporary_files(&path), 0);

        cleanup(&path);
    }
}
