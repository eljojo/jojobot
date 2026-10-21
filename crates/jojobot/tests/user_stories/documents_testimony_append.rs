//! "The fryer manual was put down in my words last week. Today the assistant
//! adds a Closing section at the end. Why would that touch my sections?"
//!
//! **A section put after the last one lands whoever wrote that section and
//! whatever its provenance.** The put gives the new heading a blank line, and
//! that separator is a mark the server keeps on the old last section; it is not
//! a rewrite of the old section's words. A real edit of those words by another
//! session is still refused.
//!
//! The roster names are the cast: a fryer manual and two bots.

use serde_json::json;

use super::dsl::Story;

const WHOLE: &str = "# Fryer manual\n\nRead this before the first shift.\n\n## Opening\n\nSwitch the fryer on at six.";

/// **Another session appends to a testimony document, and the words stay.**
/// Paired: the same put by the session that wrote the document lands, and a
/// content edit of its last section by the other session is still refused.
#[tokio::test]
async fn another_session_appends_to_a_testimony_document_and_its_words_are_untouched() {
    let story = Story::begin("bot:gamma").await;
    let writer = story.session().await;
    let other = story.as_bot("bot:assistant").await;
    writer.add("document:homer", "Fryer manual").await;
    writer
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": WHOLE, "provenance": "testimony"}),
        )
        .await;
    let before = writer.recall("document:homer").await.json();
    let last = &before["objects"][0]["facts"][1];
    let address = last["address"].as_str().expect("an address").to_string();
    assert_eq!(last["provenance"], "testimony", "{before}");

    // ── the other session appends behind the last section ────────────────────
    let landed = other
        .call(
            "put_document",
            json!({
                "document": "document:homer", "after": &address,
                "markdown": "## Closing\n\nDrain the oil.", "provenance": "inference",
            }),
        )
        .await
        .json();
    assert_ne!(landed["status"], "blocked", "{landed}");
    let read = writer
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        read["markdown"],
        "# Fryer manual\n\nRead this before the first shift.\n\n## Opening\n\nSwitch the fryer on at six.\n\n## Closing\n\nDrain the oil.",
        "one blank line before the new heading"
    );
    let after = writer.recall("document:homer").await.json();
    let kept = &after["objects"][0]["facts"][1];
    for key in ["content", "details", "provenance", "address"] {
        assert_eq!(
            kept[key], last[key],
            "{key} of the old section moved: {after}"
        );
    }

    // ── the positive: the writer's own put behind the new last section lands ─
    let closing = after["objects"][0]["facts"][2]["address"]
        .as_str()
        .expect("the new section has an address")
        .to_string();
    let own = writer
        .call(
            "put_document",
            json!({
                "document": "document:homer", "after": &closing,
                "markdown": "## Last\n\nLights off.", "provenance": "testimony",
            }),
        )
        .await
        .json();
    assert_ne!(own["status"], "blocked", "{own}");

    // ── the guard still holds: another session cannot rewrite the words ──────
    other
        .refused(
            "update_fact",
            json!({"address": &address, "content": "## Opening\n\nSwitch the fryer on at seven."}),
        )
        .await
        .says("derived_from");
    story.finish().await;
}
