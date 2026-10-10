//! "Did I ever say anything about the kite string?" — and the answer is a claim
//! that stopped being true last spring.
//!
//! **Search behaves like a mail client** (rule 269). An archived message is
//! still in the search, below the live ones; a deleted one is in the trash and
//! is asked for by name. Here a claim that was true once and was replaced or
//! archived is found by the default search, ranked strictly below every active
//! claim, while a claim that was never true — taken back with `retract` —
//! stays out until the caller asks for the archive.
//!
//! ⭐ **The order is the assertion.** Presence alone passes on a build that
//! lists the archived claim first, and that build answers the question with
//! last spring's truth.
//!
//! 🚨 **Every answer names what it left out and the argument that returns it**
//! (rule 261), the non-empty ones included: a caller who reads a full page as
//! the whole population has no way to learn that a retracted claim sat beside
//! it.

use serde_json::{Value, json};

use super::dsl::{Answer, Story};

/// The content of every fact hit, in the order the answer ranked them.
fn contents(found: &Answer) -> Vec<String> {
    found.json()["results"]
        .as_array()
        .unwrap_or_else(|| panic!("a search answers with results: {}", found.raw()))
        .iter()
        .filter(|hit| hit["hit"] == "fact")
        .map(|hit| hit["content"].as_str().unwrap_or_default().to_string())
        .collect()
}

fn position(found: &Answer, word: &str) -> usize {
    contents(found)
        .iter()
        .position(|content| content.contains(word))
        .unwrap_or_else(|| panic!("no fact hit carries {word:?}: {}", found.raw()))
}

fn hit_with<'a>(found: &'a Value, word: &str) -> &'a Value {
    found["results"]
        .as_array()
        .and_then(|hits| {
            hits.iter()
                .find(|hit| hit["content"].as_str().is_some_and(|c| c.contains(word)))
        })
        .unwrap_or_else(|| panic!("no hit carries {word:?}: {found}"))
}

#[tokio::test]
async fn an_archived_claim_is_found_below_the_active_ones_and_a_retracted_one_is_asked_for() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("person:homer", "Homer").await;

    // ── the order ─────────────────────────────────────────────────────────────
    //
    // The archived claim is the SHORTER and the NEWER, and it matches the query
    // word for word, so on relevance and on recency it would beat the active
    // claim. It is still below it.
    s.fact(
        "person:homer",
        "the string for the kite needs replacing before the fair next spring",
    )
    .await;
    let old = s.fact("person:homer", "kite string").await;
    s.supersede(&old).await;

    let found = s.find("kite string").await;
    assert_eq!(
        contents(&found).len(),
        2,
        "the archived claim is in the default search: {}",
        found.raw()
    );
    assert!(
        position(&found, "replacing") < position(&found, "kite string"),
        "an active claim ranks above an archived one: {}",
        found.raw()
    );

    // ── retracted stays out until the archive is asked for ────────────────────
    //
    // The pair: one claim taken back, one only archived, both about the same
    // word. The default finds the second and not the first; `status archived`
    // finds both.
    let sedan = s.fact("person:homer", "drives a pink sedan").await;
    s.retract(&sedan, "never true — mixed up with a neighbour")
        .await;
    let bicycle = s.fact("person:homer", "rides a pink bicycle").await;
    s.supersede(&bicycle).await;

    let default = s.find("pink").await;
    default.says("pink bicycle").never_says("pink sedan");

    let archive = s
        .call("search", json!({"query": "pink", "status": "archived"}))
        .await;
    archive.says("pink bicycle").says("pink sedan");

    // ── a non-empty answer names what it left out, and the way to get it ──────
    let left_out = default.json()["left_out"].to_string();
    assert!(
        left_out.contains("retracted") && left_out.contains("archived"),
        "a full answer names the retracted claims and the argument that returns them: {}",
        default.raw()
    );
    assert!(
        left_out.contains("include_history") && left_out.contains("include_mail"),
        "and the earlier wordings and the mail: {}",
        default.raw()
    );

    // ── `status active` is current truth only, and says it ────────────────────
    let current = s
        .call(
            "search",
            json!({"query": "kite string", "status": "active"}),
        )
        .await;
    current
        .says("replacing")
        .never_says("\"content\":\"kite string\"");

    s.wrap("looked for what the record says about the kite and the car")
        .await;
    story.finish().await;
}

/// A claim that was replaced says by what: the old hit carries the address of
/// the claim that took its place, and an active claim carries nothing.
#[tokio::test]
async fn a_replaced_claim_names_the_claim_that_replaced_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("person:maude", "Maude").await;
    let bees = s.fact("person:maude", "keeps bees on the roof").await;
    let goats = s.replace(&bees, "keeps goats in the yard", json!({})).await;

    let found = s.find("bees").await;
    let body = found.json();
    let old = hit_with(&body, "bees");
    assert_eq!(
        old["superseded_by"],
        json!(goats),
        "the old claim names its replacement: {body}"
    );

    let current = s.find("goats").await.json();
    assert!(
        hit_with(&current, "goats").get("superseded_by").is_none(),
        "an active claim names no replacement: {current}"
    );

    s.wrap("replaced a claim and read it back through the search")
        .await;
    story.finish().await;
}
