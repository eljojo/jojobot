//! **A status word outside the words a work item or project accepts — named on
//! the write that put it there.**
//!
//! A status never refuses (rule 426): the word lands. What the answer owes the
//! writer is the three facts a refusal used to carry. Which words govern this
//! thing, that the word counts as not finished until the instance says what it
//! means, and the one write that says so.

use super::*;
use jojobot_domain::memory::kinds;

impl Jojobot {
    /// **Note it on the receipt when this write set a status that the thing's
    /// accepted words do not hold and the instance has not mapped.** A no-op
    /// unless `fact` is about a work item or a project and sets `status`. A word
    /// the five, the instance's columns or the governing project's columns hold
    /// is quiet, and so is a word mapped to a shipped one.
    ///
    /// A read failure adds no note: this is jojobot's own bookkeeping riding on
    /// somebody else's write, and it must never be the reason that write fails
    /// or reads as one. A failed read of the instance record is named in
    /// `unread`, so the missing note never reads as a word the instance maps.
    pub(crate) async fn note_status_word(
        &self,
        fact: &Fact,
        body: &mut serde_json::Value,
        unread: &super::instance_unread::InstanceUnread,
    ) {
        if !kinds::holds_columns(fact.subject.kind_token()) {
            return;
        }
        let Some(word) = fact.fields.get(kinds::STATUS).map(|word| word.trim()) else {
            return;
        };
        // A shipped word means itself whatever the instance maps, so it needs no
        // note and no read of the record.
        if kinds::meaning_of(word, &Default::default()).is_some() {
            return;
        }
        // A record that cannot be read adds no note, because without it no word
        // can be called unmapped; the receipt says the note was not worked out.
        let instance = match self.instance_fields().await {
            Ok(instance) => instance,
            Err(e) => {
                unread.left(super::instance_unread::STATUS_WORD_NOTE, &e);
                return;
            }
        };
        if kinds::meaning_of(word, &instance).is_some() {
            return;
        }
        // **A board that cannot be read is named, never read as no board**: without its
        // columns a word the board holds would be called unmapped, a note that is wrong.
        let reads = super::reads_left::ReadsLeft::default();
        let project = match self.governing_project(&fact.subject).await {
            Ok(project) => project,
            Err(e) => {
                reads.left(super::reads_left::BOARD_WORD_NOTE, &fact.subject, &e);
                reads.note(body);
                return;
            }
        };
        let project_fields = match &project {
            Some(project) => match self.memory.fields(project).await {
                Ok(fields) => Some(fields),
                Err(e) => {
                    reads.left(super::reads_left::BOARD_WORD_NOTE, project, &e);
                    reads.note(body);
                    return;
                }
            },
            None => None,
        };
        let accepted = kinds::accepted_words(&instance, project_fields.as_ref());
        if accepted.iter().any(|held| held == word) {
            return;
        }
        let instance_record = crate::orientation::instance_zone::INSTANCE_RECORD;
        let board = match &project {
            Some(project) => format!(" or on {project} (that board only)"),
            None => String::new(),
        };
        let note = format!(
            "'{word}' is not one of the words this {} holds: {}. It counts as not finished. \
             One write in `sets` binds it: add it to `{}` on {instance_record} (every \
             board){board}, or write `{}{word}` on {instance_record} set to the shipped word it \
             means, one of {}.",
            fact.subject.kind_token(),
            accepted.join(", "),
            kinds::COLUMNS,
            kinds::STATUS_WORD,
            kinds::WORK_STATUSES.join(", "),
        );
        if let Some(object) = body.as_object_mut() {
            object.insert("status_word_note".into(), note.into());
        }
    }

    /// **The project whose columns govern a thing's status**: the project a work
    /// item is filed under, the project a project is filed under, or the project
    /// itself when it is filed under none. `None` for a thing filed under no
    /// project, and when the roster cannot be read.
    async fn governing_project(
        &self,
        subject: &EntityId,
    ) -> Result<Option<EntityId>, jojobot_domain::memory::MemoryError> {
        let Some(kind) = subject.kind() else {
            return Ok(None);
        };
        let held = self.memory.list_entities(Some(kind)).await?;
        let Some(entity) = held.iter().find(|entity| &entity.id == subject) else {
            return Ok(None);
        };
        let parent = entity
            .parent
            .clone()
            .filter(|parent| parent.kind() == Some(EntityKind::PROJECT));
        Ok(parent.or_else(|| (kind == EntityKind::PROJECT).then(|| subject.clone())))
    }
}
