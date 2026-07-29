//! **The words jojobot never says to an agent** — the ubiquitous language's
//! negative space.
//!
//! The software decides where a fact lands, and an agent must not learn where.
//! That is not tidiness: a caller that learns the layout starts reasoning about
//! it, which is both none of its business and eventually wrong — mail and
//! sessions have already moved stores once, and every sentence that had taught
//! their old shape became a lie the day they did.
//!
//! Three vocabularies are covered, because all three have leaked: the wiki
//! jojobot writes to today (documents filed in a collection, one page per
//! thing), the task board mail used to live on (cards in a funnel column), and
//! **the layout itself** — the words for how a thing is arranged once it gets
//! there.
//!
//! That third group is the one the first version of this list missed, and it is
//! the leak that mattered most. "Facts about this entity are in the table at the
//! bottom" names no store at all: it is pure layout, and a sweep for store names
//! sailed straight past the one sentence two independent sessions had quoted
//! back. Where a thing SITS is exactly as much not-your-business as which
//! product holds it.
//!
//! **One list, enforced at two edges**, because the leak has two shapes and
//! neither test can see the other's. What jojobot ANSWERS is swept in the MCP
//! crate, over every verb's serialized response. What jojobot STORES is swept
//! in the adapters, over the boilerplate it seeds into a page — which reaches
//! an agent later, as prose, through search. A list per edge is how one edge
//! gets a word the other never hears about.
//!
//! Test-only: behind the `testing` feature, like the shared contract spec.

/// Every retired or store-shaped word, with what an agent wrongly concludes
/// from meeting it.
pub const STORE_WORDS: &[(&str, &str)] = &[
    ("outline", "the wiki's name"),
    ("document", "an entity is not a document to a caller"),
    ("documents", "an entity is not a document to a caller"),
    ("doc", "…and neither is it a doc id"),
    ("docs", "…and neither is it a doc id"),
    ("collection", "where jojobot's pages are filed"),
    ("wiki", "what jojobot writes to"),
    (
        "page",
        "the unit the store keeps, never the unit a caller acts on",
    ),
    (
        "pages",
        "the unit the store keeps, never the unit a caller acts on",
    ),
    (
        "card",
        "a message is a message; it was a card on a board once",
    ),
    (
        "cards",
        "a message is a message; it was a card on a board once",
    ),
    ("kanban", "the board is gone"),
    ("column", "there are no columns to move between"),
    ("funnel", "there are no columns to move between"),
    // The layout. Naming no product does not make a sentence safe: "in the
    // table at the bottom" teaches an agent the shape of the page it must not
    // know it is reading.
    (
        "table",
        "how a record is laid out is not a caller's business",
    ),
    (
        "tables",
        "how a record is laid out is not a caller's business",
    ),
    ("row", "a fact is a fact, not a row somewhere"),
    ("rows", "a fact is a fact, not a row somewhere"),
    ("frontmatter", "…and neither is the block above it"),
];

/// Every store word this text uses, **as whole words**: `doc-alpha` is a doc id
/// and `document` is not two hits of `doc`. Each comes back with the reason it
/// is on the list, so a failure explains itself where it fires.
pub fn store_words(text: &str) -> Vec<(&'static str, &'static str)> {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    STORE_WORDS
        .iter()
        .filter(|(word, _)| words.contains(word))
        .copied()
        .collect()
}
