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

/// **The seam the engine may set on another session's section is not open to a
/// caller.** The testimony gate lets a lead of line feeds change, and the verb
/// refuses `edges` before the gate is asked. The patch sent here is exactly what a
/// put or a move writes, a lead of two more line feeds, and it is refused on the
/// section of another session, whatever its provenance, with nothing written.
#[tokio::test]
async fn a_caller_cannot_send_the_seam_the_engine_writes() {
    let story = Story::begin("bot:gamma").await;
    let a = story.as_bot("bot:gamma").await;
    a.add("document:homer", "Fryer manual").await;
    a.call(
        "put_document",
        json!({"document": "document:homer", "markdown": MANUAL, "provenance": "testimony"}),
    )
    .await;
    let held = sections(&a).await;
    let facts = a.recall("document:homer").await.json();
    let edges = facts["objects"][0]["facts"][1]["fields"]["edges"]
        .as_str()
        .expect("a section keeps its edges")
        .to_string();
    let b = story.as_bot("bot:gamma").await;
    let before = read(&b).await;

    b.refused(
        "update_fact",
        json!({"address": held[1], "fields": {"edges": format!("\\n\\n{edges}")}}),
    )
    .await
    .says("edges");
    assert_eq!(read(&b).await, before, "the refused seam wrote nothing");
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

/// A manual with its sections written, ready for a second session to try things on.
async fn manual() -> (Story, Session) {
    let story = Story::begin("bot:gamma").await;
    let a = story.as_bot("bot:gamma").await;
    a.add("document:homer", "Fryer manual").await;
    a.call(
        "put_document",
        json!({"document": "document:homer", "markdown": MANUAL}),
    )
    .await;
    (story, a)
}

/// **A caller's own place for a section is refused on every verb that writes
/// settings, not only on `capture` and `update_fact`.** `update_entity` and
/// `add_entity` carry the same `sets`, and a capture's `sets` is the bag the
/// place would ride in. Each is refused with nothing written, and the manual
/// reads back as it was.
#[tokio::test]
async fn a_callers_place_for_a_section_is_refused_on_every_verb_that_sets_keys() {
    let (story, a) = manual().await;
    let held = sections(&a).await;
    let b = story.as_bot("bot:gamma").await;

    b.refused(
        "update_entity",
        json!({"handle": "document:homer", "sets": {"order": "a0", "edges": framing("INJECTED")}}),
    )
    .await
    .says("order");
    b.refused(
        "capture",
        json!({"subject": "document:homer", "content": "a section nobody wrote",
               "sets": {"order": "a0"}}),
    )
    .await
    .says("order");
    b.refused(
        "add_entity",
        json!({"kind": "document", "handle": "marge", "name": "Marge", "source": "user-named",
               "sets": {"order": "a0"}}),
    )
    .await
    .says("order");
    assert_eq!(read(&b).await, MANUAL, "a refused write placed nothing");
    assert_eq!(sections(&b).await, held, "and added no section");

    // The batch form reaches the same gate: it writes one capture per entry.
    a.add("list:checklist", "Checklist").await;
    b.refused(
        "capture",
        json!({"subject": "list:checklist", "entries": ["one", "two"], "fields": {"order": "a0"}}),
    )
    .await
    .says("order");
    story.finish().await;
}

/// **The gate judges the kind the store holds, not the kind the address was typed
/// with.** A thing typed `thing:` and retyped into a document still answers to its
/// old handle; a caller's place or edges sent under that handle are refused, and
/// the document reads back as it was.
#[tokio::test]
async fn a_thing_retyped_into_a_document_is_judged_as_a_document_under_its_old_handle() {
    let story = Story::begin("bot:gamma").await;
    let a = story.as_bot("bot:gamma").await;
    a.add("thing:sigma", "Sigma").await;
    a.call(
        "rename_entity",
        json!({"handle": "thing:sigma", "to": "document:lisa"}),
    )
    .await;
    a.call(
        "put_document",
        json!({"document": "document:lisa", "markdown": "# Lisa\n\n## Notes\n\nNone yet."}),
    )
    .await;
    let section = a.recall("document:lisa").await.json()["objects"][0]["facts"][1]["address"]
        .as_str()
        .expect("a section address")
        .replace("document:lisa", "thing:sigma");
    let before = a
        .call("read_document", json!({"document": "document:lisa"}))
        .await
        .json()["markdown"]
        .clone();

    a.refused(
        "capture",
        json!({"subject": "thing:sigma", "content": "a section nobody wrote",
               "fields": {"order": "a0"}}),
    )
    .await
    .says("order");
    a.refused(
        "update_fact",
        json!({"address": section, "fields": {"edges": framing("INJECTED")}}),
    )
    .await
    .says("edges");
    let after = a
        .call("read_document", json!({"document": "document:lisa"}))
        .await
        .json()["markdown"]
        .clone();
    assert_eq!(after, before, "the refused writes changed nothing");
    story.finish().await;
}

/// **`edges` is refused on a SECTION, not on every claim about a document.** A
/// plain note about a manual that happens to name a field `edges` is an ordinary
/// claim: a capture with no place is accepted, and so is an edit of that note.
/// Paired with the refusals above, so the gate has not simply gone quiet.
#[tokio::test]
async fn a_plain_field_named_edges_on_a_claim_that_is_not_a_section_is_accepted() {
    let (story, a) = manual().await;
    let held = sections(&a).await;
    let note = a
        .call(
            "capture",
            json!({"subject": "document:homer", "content": "Printed copy lives in the office",
                   "fields": {"edges": "back shelf"}}),
        )
        .await
        .json()["address"]
        .as_str()
        .expect("the note's address")
        .to_string();
    a.call(
        "update_fact",
        json!({"address": note, "fields": {"edges": "front shelf"}}),
    )
    .await;
    assert_eq!(read(&a).await, MANUAL, "the note is not part of the text");
    assert_eq!(
        sections(&a).await.len(),
        held.len() + 1,
        "it is one more claim"
    );

    a.refused(
        "update_fact",
        json!({"address": held[1], "fields": {"edges": framing("INJECTED")}}),
    )
    .await
    .says("edges");
    story.finish().await;
}

/// **A section addressed as it was before a merge is still a section.** The
/// folded document holds nothing; its sections are served as the survivor's. An
/// edit that names `edges` through the folded document's address is refused like
/// one through the survivor's, and the merged manual reads back as it was.
#[tokio::test]
async fn a_section_edited_through_its_pre_merge_address_is_still_refused_its_edges() {
    let (story, a) = manual().await;
    a.add("document:marge", "Griddle logbook").await;
    a.call(
        "put_document",
        json!({"document": "document:marge", "markdown": "# Griddle notes\n\n## Oil\n\nChange it weekly."}),
    )
    .await;
    let folded = a.recall("document:marge").await.json()["objects"][0]["facts"][1]["address"]
        .as_str()
        .expect("a section address")
        .to_string();
    assert!(folded.starts_with("document:marge#"), "{folded}");
    let note = a
        .call(
            "capture",
            json!({"subject": "document:marge", "content": "Kept in the back office"}),
        )
        .await
        .json()["address"]
        .as_str()
        .expect("the note's address")
        .to_string();
    a.call(
        "merge_entities",
        json!({"duplicate": "document:marge", "survivor": "document:homer", "reason": "one manual twice"}),
    )
    .await;
    let before = read(&a).await;
    assert!(
        before.contains("Change it weekly."),
        "the merge moved the section: {before}"
    );

    a.refused(
        "update_fact",
        json!({"address": folded, "fields": {"edges": framing("INJECTED")}}),
    )
    .await
    .says("edges");
    assert_eq!(read(&a).await, before, "the refused edit wrote nothing");

    // A claim the store cannot find is judged a section, not "not one": the gate
    // refuses the `edges` before the edit would say the claim is missing.
    a.refused(
        "update_fact",
        json!({"address": "document:homer#f99", "fields": {"edges": framing("INJECTED")}}),
    )
    .await
    .says("edges");

    // The pair: a plain note on the folded document is not a section, so its
    // `edges` field is an ordinary field through the same pre-merge route.
    a.call(
        "update_fact",
        json!({"address": note, "fields": {"edges": "back shelf"}}),
    )
    .await;
    assert_eq!(read(&a).await, before, "the note is not part of the text");
    story.finish().await;
}
