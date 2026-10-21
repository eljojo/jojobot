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

/// How many times a section's record was written, as a read says it: a record
/// written once says nothing, a revised one says how many.
async fn revisions(session: &Session, address: &str) -> u64 {
    let facts = session.recall("document:homer").await.json();
    facts["objects"][0]["facts"]
        .as_array()
        .expect("a document lists its sections")
        .iter()
        .find(|fact| fact["address"] == address)
        .expect("the section is there")
        .get("revision_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1)
}

/// **A list delta on `edges` is refused like any other route to it**, and the
/// engine's put behind a section never rewrites that section. A caller declares a
/// list type on `edges` and adds or removes an item on another session's testimony
/// section: refused, and the document reads back unchanged. A put behind the last
/// section (which ended on its words) lands with exactly one blank line before the
/// new heading, by the new section's own lead, with the old section's record
/// written no more than before, whether the putting session is another one or the
/// section's own.
#[tokio::test]
async fn a_list_delta_on_edges_is_refused_and_a_put_behind_a_section_leaves_it_unwritten() {
    let story = Story::begin("bot:gamma").await;
    let a = story.as_bot("bot:gamma").await;
    a.add("document:homer", "Fryer manual").await;
    a.call(
        "put_document",
        json!({"document": "document:homer", "markdown": MANUAL, "provenance": "testimony"}),
    )
    .await;
    let opening = sections(&a).await[1].clone();
    let b = story.as_bot("bot:gamma").await;
    a.call(
        "declare_type",
        json!({"name": "scrap", "fields": [{"key": "edges", "holds": "list:text"}]}),
    )
    .await;

    for delta in ["sets_add", "sets_remove"] {
        b.refused(
            "update_fact",
            json!({"address": opening, delta: {"edges": "INJECTED"}}),
        )
        .await
        .says("edges");
        assert_eq!(read(&b).await, MANUAL, "a refused {delta} wrote nothing");
    }
    assert_eq!(
        revisions(&b, &opening).await,
        1,
        "the section was never written"
    );

    // ── the engine's put behind the last section ───────────────────────────
    b.call(
        "put_document",
        json!({"document": "document:homer", "markdown": "## Cleaning\n\nWipe it down.",
               "after": opening}),
    )
    .await;
    assert_eq!(
        read(&b).await,
        format!("{MANUAL}\n\n## Cleaning\n\nWipe it down."),
        "exactly one blank line before the new heading, the old words unchanged"
    );
    assert_eq!(
        revisions(&b, &opening).await,
        1,
        "the old section's record was not written by the put behind it"
    );

    // The section's own session does the same.
    let cleaning = sections(&a).await[2].clone();
    a.call(
        "put_document",
        json!({"document": "document:homer", "markdown": "## Closing\n\nSwitch it off.",
               "after": cleaning}),
    )
    .await;
    assert_eq!(
        read(&a).await,
        format!("{MANUAL}\n\n## Cleaning\n\nWipe it down.\n\n## Closing\n\nSwitch it off."),
    );
    assert_eq!(revisions(&a, &cleaning).await, 1);
    story.finish().await;
}
