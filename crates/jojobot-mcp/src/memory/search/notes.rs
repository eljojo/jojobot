//! Teach the notes that actually ship, once for each session and wording.

use super::*;
use crate::caller::Caller;

/// **What a reader that lists only entities and claims says of the hits it holds and does
/// not list**: how many, and where they are read. The count leads, so a reader can take it
/// off the line. The reader asks for no mail and no asker, so the only hits it holds and
/// does not list are in prose, and prose is the one source the line names. The way to read
/// them is the search verb, whose answer lists every hit kind, so the page and the verb name
/// one place.
pub fn not_listed_note(count: usize) -> String {
    format!(
        "{count} found in prose, which this page does not list. The search verb returns each \
         of them as a hit."
    )
}

impl Jojobot {
    async fn first_search_note(&self, note: &str, caller: &Caller) -> bool {
        let key = crate::teaching::ledger_key(crate::teaching::SEARCH_CAVEATS_DOMAIN, note);
        self.first_contact(&key, Some(caller)).await
    }

    pub(super) async fn teach_search_notes(
        &self,
        body: &mut serde_json::Value,
        caller: Option<&Caller>,
    ) {
        // No identity means no ledger to spend. Anonymous answers keep their notes.
        let Some(caller) = caller else {
            return;
        };
        for key in [
            "matching",
            "corpus",
            "memory",
            "mail",
            "sessions",
            "shares_a_word",
        ] {
            if let Some(note) = body[key]["note"].as_str().map(str::to_string)
                && !self.first_search_note(&note, caller).await
                && let Some(fields) = body[key].as_object_mut()
            {
                fields.remove("note");
                if fields.is_empty() {
                    body.as_object_mut().expect("search answer").remove(key);
                }
            }
        }
        for key in ["left_out", "type_displaced"] {
            if let Some(notes) = body.get_mut(key).and_then(serde_json::Value::as_array_mut) {
                for note in std::mem::take(notes) {
                    if let Some(text) = note.as_str() {
                        if self.first_search_note(text, caller).await {
                            notes.push(note);
                        }
                    } else {
                        notes.push(note);
                    }
                }
                if notes.is_empty() {
                    body.as_object_mut().expect("search answer").remove(key);
                }
            }
        }
        for key in ["read_the_document", "searched", "past_the_end"] {
            if let Some(note) = body[key].as_str().map(str::to_string)
                && !self.first_search_note(&note, caller).await
            {
                body.as_object_mut().expect("search answer").remove(key);
            }
        }
        for (key, instruction) in [
            ("details_left_out", "how_to_ask"),
            ("not_shown", "how_to_proceed"),
        ] {
            if let Some(note) = body[key][instruction].as_str().map(str::to_string)
                && !self.first_search_note(&note, caller).await
                && let Some(fields) = body[key].as_object_mut()
            {
                fields.remove(instruction);
            }
        }
    }
}
