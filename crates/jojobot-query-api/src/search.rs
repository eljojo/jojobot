//! The search read: the words a caller asks for and what the index answered.

use jojobot_domain::memory::search::Hit;

/// **A search by plain words**, as the search verb runs it with every default
/// the verb has. The asker comes beside it on the handle.
pub struct SearchQuestion {
    pub words: String,
}

/// **What the index answered**: the ranked hits and what the search could not
/// see, read after the search because the refresh it took is what the coverage
/// describes.
pub struct Searched {
    pub hits: Vec<Hit>,
    /// The note of the memory index when it has not caught up with the store.
    pub index_note: Option<String>,
    /// The note that the words are matched loosely, given on a thin answer.
    pub matching_note: Option<String>,
}
