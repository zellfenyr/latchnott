use time::OffsetDateTime;

use crate::domain::Note;
use crate::storage::{NoteRepository, RepositoryError};

/// Creates and persists notes.
#[derive(Debug)]
pub struct CreateNote<'a, R> {
    repository: &'a mut R,
}

impl<'a, R> CreateNote<'a, R>
where
    R: NoteRepository,
{
    pub fn new(repository: &'a mut R) -> Self {
        Self { repository }
    }

    pub fn execute(&mut self, text: String) -> Result<Option<Note>, RepositoryError> {
        let Some(note) = Note::new(text, OffsetDateTime::now_utc()) else {
            return Ok(None);
        };

        self.repository.save(note.clone())?;

        Ok(Some(note))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct InMemoryNoteRepository {
        saved_notes: Vec<Note>,
    }

    impl NoteRepository for InMemoryNoteRepository {
        fn save(&mut self, note: Note) -> Result<(), RepositoryError> {
            self.saved_notes.push(note);
            Ok(())
        }
    }

    #[test]
    fn creates_and_saves_note() {
        let mut repository = InMemoryNoteRepository::default();
        let mut create_note = CreateNote::new(&mut repository);

        let note = create_note
            .execute("Application test".to_owned())
            .expect("repository save should succeed")
            .expect("non-empty text should create a note");

        assert_eq!(note.text(), "Application test");
        assert_eq!(repository.saved_notes, vec![note]);
    }

    #[test]
    fn rejects_whitespace_only_text_without_saving() {
        let mut repository = InMemoryNoteRepository::default();
        let mut create_note = CreateNote::new(&mut repository);

        let result = create_note
            .execute("   \t\n".to_owned())
            .expect("invalid note input should not be a repository error");

        assert!(result.is_none());
        assert!(repository.saved_notes.is_empty());
    }

    #[test]
    fn propagates_repository_error() {
        struct FailingRepository;

        impl NoteRepository for FailingRepository {
            fn save(&mut self, _note: Note) -> Result<(), RepositoryError> {
                Err(RepositoryError::new("repository failure"))
            }
        }

        let mut repository = FailingRepository;
        let mut create_note = CreateNote::new(&mut repository);

        let error = create_note
            .execute("Application test".to_owned())
            .expect_err("repository error should be propagated");

        assert_eq!(error.message(), "repository failure");
    }
}
