//! **An edit to part of a claim's text**, found by the words it replaces.
//!
//! A caller that wants to change one sentence of a long `details` should not
//! have to send the rest of it back, because every byte it resends is a byte it
//! can get wrong. It names the words to find and the words to put there instead.
//!
//! **The match is exact**: byte for byte, case-sensitive, no pattern. The words
//! have to occur exactly once in the text the edit is applied to. Zero
//! occurrences and two or more are both refused with the count, because both
//! mean the caller is not looking at the text it thinks it is, and guessing
//! which place was meant is how a loss goes unnoticed.
//!
//! **Occurrences are counted wherever they start**, so `aa` occurs twice in
//! `aaa`. Counting only the ones that do not overlap would let an edit through
//! on text where two places both match.

use super::{FactAddress, MemoryError};

/// Which text of a claim an edit changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextField {
    /// The claim's words.
    Content,
    /// What the claim says underneath.
    Details,
}

impl TextField {
    /// The argument spelling, which is also the key a receipt names it by.
    pub fn as_str(self) -> &'static str {
        match self {
            TextField::Content => "content",
            TextField::Details => "details",
        }
    }

    /// Read the argument spelling; anything else is not a field of a claim.
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "content" => Some(TextField::Content),
            "details" => Some(TextField::Details),
            _ => None,
        }
    }
}

/// The words to find and the words that take their place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// The text the edit changes.
    pub field: TextField,
    /// The words to find. Never empty.
    pub find: String,
    /// The words to put there. Empty deletes the found words.
    pub replace: String,
}

/// What an applied edit changed, measured in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdited {
    /// The text the edit changed.
    pub field: TextField,
    /// The length of the text before the edit.
    pub bytes_before: usize,
    /// The length of the text after the edit.
    pub bytes_after: usize,
    /// Where the inserted words start in the text AFTER the edit.
    pub start: usize,
    /// Where the inserted words end in the text AFTER the edit.
    pub end: usize,
    /// The words that were taken out.
    pub removed: String,
    /// The words that were put in.
    pub inserted: String,
}

/// Apply `edit` to `current`. Returns the new text and what changed.
pub fn apply_text_edit(
    address: &FactAddress,
    current: &str,
    edit: &TextEdit,
) -> Result<(String, TextEdited), MemoryError> {
    if edit.find.is_empty() {
        return Err(MemoryError::InvalidFact(
            "`find` is empty, and an empty string names no place in the text: send the words \
             to find"
                .into(),
        ));
    }
    // **Every character boundary where the words start**, so occurrences that
    // overlap are counted. A match can only start on a boundary, because `find`
    // is valid UTF-8 and begins with a whole character; slicing between two
    // bytes of one character would panic.
    let starts: Vec<usize> = current
        .char_indices()
        .map(|(at, _)| at)
        .filter(|at| current[*at..].starts_with(edit.find.as_str()))
        .collect();
    let [start] = starts[..] else {
        return Err(MemoryError::TextEditMatches {
            address: address.to_string(),
            field: edit.field.as_str().to_string(),
            found: starts.len(),
        });
    };
    let end = start + edit.find.len();
    let text = format!("{}{}{}", &current[..start], edit.replace, &current[end..]);
    let edited = TextEdited {
        field: edit.field,
        bytes_before: current.len(),
        bytes_after: text.len(),
        start,
        end: start + edit.replace.len(),
        removed: edit.find.clone(),
        inserted: edit.replace.clone(),
    };
    Ok((text, edited))
}

/// What reading a claim for an edit gave back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRead {
    /// The claim's text after the edit, to write as the new `content` or
    /// `details`.
    pub text: String,
    /// What the edit changed.
    pub edited: TextEdited,
    /// The revision the text was read at, to send as the write's
    /// `expected_revision` so the write lands only if the claim has not moved.
    pub revision: usize,
}

/// Read the claim's newest text from `store` and apply `edit` to it.
///
/// **The edit is applied to a read, so the write that follows has to carry the
/// revision of that read** ([`TextRead::revision`]): the store's own check then
/// refuses it if the claim has been written since, and the text it was applied
/// to is never one the claim has moved past.
///
/// A caller that named the revision it read ([`FactPatch::expected_revision`])
/// is answered first: a claim at another revision is a stale edit whatever the
/// words say, and "no occurrence" would send the caller to the wrong place.
pub async fn read_text_edit<M: super::Memory + ?Sized>(
    store: &M,
    address: &FactAddress,
    edit: &TextEdit,
    expected_revision: Option<usize>,
) -> Result<TextRead, MemoryError> {
    let chain = store.claim_history(address).await?;
    if let Some(refused) = super::refuses_a_stale_revision(address, expected_revision, chain.len())
    {
        return Err(refused);
    }
    let newest = chain.last().ok_or_else(|| MemoryError::UnknownFact {
        attempted: address.to_string(),
        nearest: Vec::new(),
    })?;
    let current = match edit.field {
        TextField::Content => newest.content.as_str(),
        TextField::Details => newest.details.as_deref().unwrap_or(""),
    };
    let (text, edited) = apply_text_edit(address, current, edit)?;
    Ok(TextRead {
        text,
        edited,
        revision: chain.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address() -> FactAddress {
        FactAddress::new(
            super::super::EntityId::person("person:alpha"),
            super::super::FactId("f1".into()),
        )
    }

    fn edit(find: &str, replace: &str) -> TextEdit {
        TextEdit {
            field: TextField::Details,
            find: find.into(),
            replace: replace.into(),
        }
    }

    /// **A unique find lands, and every other byte is unchanged.** The whole
    /// text is asserted, not a piece of it: a build that rewrote the text around
    /// the match would still contain the replacement.
    #[test]
    fn a_unique_find_is_replaced_and_every_other_byte_stays() {
        let before = "first line\n\nsecond line has a typo in it\n\nthird line";
        let (after, _) = apply_text_edit(&address(), before, &edit("a typo", "a fix"))
            .expect("one occurrence is an edit");
        assert_eq!(
            after,
            "first line\n\nsecond line has a fix in it\n\nthird line"
        );
    }

    /// **Two places that match is a refusal naming the count.** The first match
    /// is not taken: that is the edit this rule exists to refuse.
    #[test]
    fn a_find_that_matches_twice_is_refused_with_the_count() {
        let refused = apply_text_edit(&address(), "one two one", &edit("one", "1"));
        match refused {
            Err(MemoryError::TextEditMatches { found, field, .. }) => {
                assert_eq!(found, 2);
                assert_eq!(field, "details");
            }
            other => panic!("two occurrences were {other:?}"),
        }
    }

    /// **Occurrences that overlap count.** `aa` starts at two places in `aaa`.
    #[test]
    fn occurrences_that_overlap_are_counted() {
        match apply_text_edit(&address(), "aaa", &edit("aa", "b")) {
            Err(MemoryError::TextEditMatches { found, .. }) => assert_eq!(found, 2),
            other => panic!("overlapping occurrences were {other:?}"),
        }
    }

    /// **No occurrence is a refusal naming the count, 0.** The match is exact,
    /// so a different case is no occurrence.
    #[test]
    fn a_find_that_matches_nothing_is_refused_with_a_count_of_zero() {
        match apply_text_edit(&address(), "Typo here", &edit("typo", "fix")) {
            Err(MemoryError::TextEditMatches { found, .. }) => assert_eq!(found, 0),
            other => panic!("no occurrence was {other:?}"),
        }
    }

    /// **An empty find names no place**, so it is refused rather than matching
    /// everywhere.
    #[test]
    fn an_empty_find_is_refused() {
        assert!(matches!(
            apply_text_edit(&address(), "text", &edit("", "x")),
            Err(MemoryError::InvalidFact(_))
        ));
    }

    /// **An empty replacement deletes the found words**, and nothing else.
    #[test]
    fn an_empty_replace_deletes_the_found_words() {
        let (after, done) =
            apply_text_edit(&address(), "keep this, drop that", &edit(", drop that", ""))
                .expect("a deletion");
        assert_eq!(after, "keep this");
        assert_eq!((done.start, done.end), (9, 9));
        assert_eq!(done.removed, ", drop that");
        assert_eq!(done.inserted, "");
    }

    /// **What changed is measured in bytes of the text as stored**, not in
    /// characters, and the span is in the text after the edit. `é` is two bytes.
    #[test]
    fn the_receipt_numbers_are_bytes_and_the_span_is_in_the_new_text() {
        let before = "café au lait";
        let (after, done) =
            apply_text_edit(&address(), before, &edit("au", "avec du")).expect("an edit");
        assert_eq!(after, "café avec du lait");
        assert_eq!(done.bytes_before, 13);
        assert_eq!(done.bytes_after, 18);
        assert_eq!((done.start, done.end), (6, 13));
        assert_eq!(&after[done.start..done.end], "avec du");
        assert_eq!(done.removed, "au");
        assert_eq!(done.inserted, "avec du");
        assert_eq!(done.field, TextField::Details);
    }
}
