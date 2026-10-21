//! "Another session rewrote the operator's section and put its own words around it."
//!
//! **Only the engine writes a document section's edges**, the blank lines it keeps
//! around the section's words. A caller that sends them, or clears them, is
//! refused, so the words of a section another session wrote cannot be framed with
//! text the reader takes for part of the document. The engine's own write, a
//! `put_document` behind a section, still gives that section its blank line.
//!
//! The roster names are the cast: a fryer manual and two sessions of one bot.

use serde_json::json;

use super::dsl::{Session, Story};

const MANUAL: &str = "# Fryer manual\n\nRead this before the first shift.\n\n## Opening\n\nSwitch the fryer on at six.";

/// The addresses of a document's sections, in order.
async fn sections(session: &Session) -> Vec<String> {
    session.recall("document:homer").await.json()["objects"][0]["facts"]
        .as_array()
        .expect("a document lists its sections")
        .iter()
        .map(|fact| fact["address"].as_str().expect("an address").to_string())
        .collect()
}

/// An `edges` value that is well formed and says `text` between two blank lines:
/// every character of a run is an escape, as the encoding wants.
fn framing(text: &str) -> String {
    let escaped: String = text
        .chars()
        .map(|c| format!("\\u{{{:x}}}", c as u32))
        .collect();
    format!(";;\\n\\n{escaped}\\n\\n;")
}

/// The whole manual as `read_document` joins it.
async fn read(session: &Session) -> String {
    session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json()["markdown"]
        .as_str()
        .expect("the markdown")
        .to_string()
}

/// **A caller's `edges` on a section another session wrote is refused, sent or
/// cleared, whatever the section's provenance, and the document reads back byte
/// for byte.** The pair is the engine's own write: a `put_document` behind the
/// last section lands with its blank line and the section's words unchanged.
#[tokio::test]
async fn a_caller_cannot_send_or_clear_a_sections_edges_but_the_engine_still_writes_them() {
    let story = Story::begin("bot:gamma").await;
    let a = story.as_bot("bot:gamma").await;
    a.add("document:homer", "Fryer manual").await;
    a.call(
        "put_document",
        json!({"document": "document:homer", "markdown": MANUAL, "provenance": "testimony"}),
    )
    .await;
    let held = sections(&a).await;
    let b = story.as_bot("bot:gamma").await;

    for (provenance, section) in [("testimony", &held[1])] {
        let injected = b
            .refused(
                "update_fact",
                json!({"address": section, "fields": {"edges": framing("INJECTED")}}),
            )
            .await;
        injected.says("edges");
        let cleared = b
            .refused(
                "update_fact",
                json!({"address": section, "clear_fields": ["edges"]}),
            )
            .await;
        cleared.says("edges");
        assert_eq!(
            read(&b).await,
            MANUAL,
            "a refused {provenance} edit wrote nothing"
        );
    }

    // The same refusal on a section that is not testimony.
    a.call(
        "put_document",
        json!({"document": "document:homer", "markdown": "## Closing\n\nTurn it off.",
               "after": held[1]}),
    )
    .await;
    let closing = sections(&a).await[2].clone();
    let before = read(&b).await;
    b.refused(
        "update_fact",
        json!({"address": closing, "fields": {"edges": framing("INJECTED")}}),
    )
    .await
    .says("edges");
    assert_eq!(
        read(&b).await,
        before,
        "a refused inference edit wrote nothing"
    );

    // ── the pair: the engine's own write behind a section ──────────────────
    b.call(
        "put_document",
        json!({"document": "document:homer", "markdown": "## Cleaning\n\nWipe it down.",
               "after": closing}),
    )
    .await;
    let after = read(&b).await;
    assert!(
        after.starts_with(&before) && after.ends_with("## Cleaning\n\nWipe it down."),
        "the section behind keeps the words before it unchanged: {after}"
    );
    assert!(!after.contains("INJECTED"), "{after}");
    story.finish().await;
}
