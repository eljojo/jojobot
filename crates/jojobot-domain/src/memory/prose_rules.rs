//! **The two rules for a document's prose**: what it may not carry, and the form it
//! is stored in. Every adapter calls them, so none can be the lenient one.

use super::{FACTS_HEADER, MemoryError};

/// The lines a document reserves for its own structure. Prose may not carry
/// one, whatever store it is bound for — see [`validate_prose`].
///
/// A list rather than the single constant: this is the seam. When the schema
/// grows a second structural line, it is added here once and every adapter,
/// the fake included, refuses it from that moment.
pub const RESERVED_PROSE_LINES: &[&str] = &[FACTS_HEADER];

/// Validate an entity's prose — the human half of its doc, where a bot's
/// charter lives.
///
/// Permissive by design: paragraphs are the point, so only two things are
/// refused. **Emptiness**, because a page with nothing on it is not a charter.
/// And **a line the document schema reserves** ([`RESERVED_PROSE_LINES`]),
/// because the reader finds the fact table by the first such line: prose
/// carrying one moves the boundary, and every fact below it stops being read as
/// a fact. Refused rather than escaped — silently mangling somebody's charter
/// is worse than declining to write it.
///
/// The second rule lives here, not in the one adapter whose documents it would
/// corrupt, because a fake that waves it through is how a green suite ships a
/// store-corrupting write. Every adapter validates through this, so none can be
/// the lenient one.
pub fn validate_prose(prose: &str) -> Result<(), MemoryError> {
    if prose.trim().is_empty() {
        return Err(MemoryError::InvalidEntity(
            "prose is empty; there is nothing to write".into(),
        ));
    }
    if let Some(reserved) = prose
        .lines()
        .map(str::trim)
        .find(|line| RESERVED_PROSE_LINES.contains(line))
    {
        return Err(MemoryError::InvalidEntity(format!(
            "prose carries the line '{reserved}', which jojobot reserves for a record's own \
             structure; every fact below such a line would stop being read as a fact. Say it \
             some other way — the words on their own, not on a line of their own, are fine"
        )));
    }
    Ok(())
}

/// Normalize prose to the form that survives a round-trip: edge whitespace is
/// not significant and no store preserves it, and CRLF folds to `\n` because a
/// store that rebuilds text line by line strips the `\r`s. Both adapters call
/// this, which is what makes the returned prose identical to a later read's.
pub fn normalize_prose(prose: &str) -> String {
    let mut out = prose.to_string();
    while out.contains("\r\n") {
        out = out.replace("\r\n", "\n");
    }
    out.trim().to_string()
}
