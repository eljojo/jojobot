//! The note an answer carries about what it left out.

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
