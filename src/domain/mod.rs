use time::OffsetDateTime;
use uuid::Uuid;

/// A single note in the Latchnott domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    id: Uuid,
    text: String,
    created_at: OffsetDateTime,
}

impl Note {
    pub fn new(text: String, created_at: OffsetDateTime) -> Option<Self> {
        if text.trim().is_empty() {
            return None;
        }

        Some(Self {
            id: Uuid::now_v7(),
            text,
            created_at,
        })
    }

    pub(crate) fn from_parts(id: Uuid, text: String, created_at: OffsetDateTime) -> Option<Self> {
        if text.trim().is_empty() {
            return None;
        }

        Some(Self {
            id,
            text,
            created_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_note_from_non_empty_text() {
        let created_at = OffsetDateTime::now_utc();
        let note = Note::new("Test note".to_owned(), created_at);

        assert!(note.is_some());

        let note = note.expect("non-empty text should create a note");

        assert_eq!(note.text(), "Test note");
        assert_eq!(note.created_at(), created_at);
        assert_eq!(note.id().get_version(), Some(uuid::Version::SortRand));
    }

    #[test]
    fn rejects_empty_text() {
        let created_at = OffsetDateTime::now_utc();

        assert!(Note::new(String::new(), created_at).is_none());
    }

    #[test]
    fn rejects_whitespace_only_text() {
        let created_at = OffsetDateTime::now_utc();

        assert!(Note::new("   \t\n  ".to_owned(), created_at).is_none());
    }

    #[test]
    fn preserves_surrounding_whitespace_for_non_empty_text() {
        let created_at = OffsetDateTime::now_utc();
        let note = Note::new("  Test note  ".to_owned(), created_at)
            .expect("non-empty text should create a note");

        assert_eq!(note.text(), "  Test note  ");
    }

    #[test]
    fn restores_note_from_persisted_parts() {
        let id = Uuid::now_v7();
        let created_at = OffsetDateTime::now_utc();

        let note = Note::from_parts(id, "Restored note".to_owned(), created_at)
            .expect("valid persisted note should be restored");

        assert_eq!(note.id(), id);
        assert_eq!(note.text(), "Restored note");
        assert_eq!(note.created_at(), created_at);
    }

    #[test]
    fn rejects_empty_restored_text() {
        let id = Uuid::now_v7();
        let created_at = OffsetDateTime::now_utc();

        assert!(Note::from_parts(id, String::new(), created_at).is_none());
    }
}
