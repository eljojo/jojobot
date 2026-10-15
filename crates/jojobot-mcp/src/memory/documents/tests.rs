//! The document verbs, through the served surface: a file put and read back,
//! one section edited, a document too long for one answer.

use super::*;
use crate::harness::*;
use crate::memory::testing::*;
use jojobot_domain::memory::search::Hit;
use jojobot_domain::text::ANSWER_CEILING;
use std::sync::Arc;

const NOTES: &str = "---\nname: alpha\n---\n\n# Alpha notes\n\nA first paragraph.\n\n## Beta part\n\nText with a fence:\n\n```\n# not a heading\n@person:ralph\n```\n\n## Gamma part\n\nThe end, with no final newline.";

async fn put_args(document: &str, markdown: &str) -> PutDocumentArgs {
    PutDocumentArgs {
        document: document.into(),
        markdown: Some(markdown.into()),
        restore_to: None,
        confirmed_by_user: None,
        after: None,
        provenance: None,
        source: None,
        source_ref: None,
        recorded_at: None,
        sid: Some(TEST_SID.into()),
    }
}

async fn put(jojobot: &Jojobot, args: PutDocumentArgs) -> CallToolResult {
    jojobot
        .put_document(Parameters(args))
        .await
        .expect("put_document answers")
}

/// A document that exists, and the markdown put onto it.
async fn put_ok(jojobot: &Jojobot, document: &str, markdown: &str) -> serde_json::Value {
    ensure(jojobot, document).await;
    let answered = json_of(&put(jojobot, put_args(document, markdown).await).await);
    assert_ne!(
        answered["status"], "blocked",
        "the put was refused: {answered}"
    );
    answered
}

async fn read_part(jojobot: &Jojobot, document: &str, offset: Option<usize>) -> serde_json::Value {
    json_of(
        &jojobot
            .read_document(Parameters(ReadDocumentArgs {
                document: document.into(),
                offset,
                as_of: None,
                sid: Some(TEST_SID.into()),
            }))
            .await
            .expect("read_document answers"),
    )
}

/// The whole document, read in as many parts as it takes.
async fn read_all(jojobot: &Jojobot, document: &str) -> String {
    let mut text = String::new();
    let mut offset = None;
    loop {
        let part = read_part(jojobot, document, offset).await;
        text.push_str(part["markdown"].as_str().expect("a part carries markdown"));
        match part["next_offset"].as_u64() {
            Some(next) => offset = Some(next as usize),
            None => return text,
        }
    }
}

/// The records of a document: address, words and own fields, in write order,
/// read in as many parts as `recall` needs under the answer ceiling.
async fn records(jojobot: &Jojobot, document: &str) -> Vec<serde_json::Value> {
    let mut held = Vec::new();
    let mut offset = 0;
    loop {
        let body = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    offset: Some(offset),
                    ..recall_args(document)
                }))
                .await
                .expect("recall answers"),
        );
        held.extend(
            body["objects"][0]["facts"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
        );
        match body["not_shown"]["offset"].as_u64() {
            Some(next) => offset = next as u32,
            None => return held,
        }
    }
}

/// **A whole put then a read is the file, byte for byte.** The positive half;
/// the edited-section case below is the negative one. The shapes are the ones a
/// trimming store would lose: a frontmatter block, a fence holding a `#` line
/// and a handle, no final newline, trailing spaces, CRLF, a bare heading.
#[tokio::test]
async fn a_whole_put_then_a_read_is_the_file_byte_for_byte() {
    let jojobot = handler_mentioning();
    for (document, text) in [
        ("document:homer", NOTES),
        ("document:marge", "# One  \n\nbody\n\n\n\n"),
        (
            "document:lisa",
            "preamble\n\n# One\r\n\r\nbody\r\n## Two\r\nmore\r\n",
        ),
        ("document:bart", "# Only a heading"),
        ("document:krusty", "\n\n  # Indented\ntext\n"),
        ("document:skinner", "# A\n# B\n# C\n"),
    ] {
        put_ok(&jojobot, document, text).await;
        assert_eq!(read_all(&jojobot, document).await, text, "{text:?}");
    }
}

/// **A frontmatter block in the file is its first section, verbatim**, at level
/// 0; the document's own fields ride beside the text on a read, never in it.
#[tokio::test]
async fn a_frontmatter_block_is_the_first_section_and_the_fields_ride_beside_the_text() {
    let jojobot = handler();
    put_ok(&jojobot, "document:alpha", NOTES).await;
    let held = records(&jojobot, "document:alpha").await;
    assert_eq!(held[0]["content"], "---");
    assert_eq!(held[0]["fields"]["level"], "0");
    assert_eq!(held[1]["fields"]["level"], "1");
    let part = read_part(&jojobot, "document:alpha", None).await;
    assert_eq!(part["name"], "alpha");
    assert!(
        part["markdown"]
            .as_str()
            .is_some_and(|m| m.starts_with("---\nname: alpha\n---")),
        "{part}"
    );
    assert_eq!(part["sections"], held.len());
}

/// **The stored spellings of a section's keys are pinned by their literals**:
/// a record written under another spelling is one a later build cannot read.
#[tokio::test]
async fn a_section_is_stored_under_the_keys_level_order_edges_and_continues() {
    let jojobot = handler();
    let answered = put_ok(&jojobot, "document:alpha", "# One\n\nbody\n# Two\n").await;
    let held = records(&jojobot, "document:alpha").await;
    assert_eq!(answered["first"], held[0]["address"]);
    assert_eq!(answered["last"], held[1]["address"]);
    assert_eq!(answered["bytes"], 18);
    let fields = held[0]["fields"].clone();
    let keys: Vec<&str> = fields
        .as_object()
        .expect("fields")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["edges", "level", "order"]);
    assert_eq!(fields["level"], "1");
    assert_eq!(fields["edges"], ";;\\n\\n;\\n");
}

/// **One edited section leaves every other section's bytes where they were.**
/// The negative half of the round trip: nothing else moved.
#[tokio::test]
async fn editing_one_section_leaves_every_other_sections_bytes() {
    let jojobot = handler_with_mentions();
    put_ok(&jojobot, "document:alpha", NOTES).await;
    let held = records(&jojobot, "document:alpha").await;
    let beta = held
        .iter()
        .find(|fact| fact["content"] == "## Beta part")
        .expect("the beta section")["address"]
        .as_str()
        .expect("an address")
        .to_string();
    let edited = jojobot
        .update_fact(Parameters(UpdateFactArgs {
            find: Some("Text with a fence:".into()),
            replace: Some("Text with a code block:".into()),
            field: Some("details".into()),
            ..update_args(&beta)
        }))
        .await
        .expect("update_fact answers");
    assert_ne!(json_of(&edited)["status"], "blocked", "{edited:?}");
    assert_eq!(
        read_all(&jojobot, "document:alpha").await,
        NOTES.replace("Text with a fence:", "Text with a code block:")
    );
}

/// **A put onto a document that has sections is refused unless it says where**,
/// and the refusal names both ways forward. Nothing is written: putting an
/// edited file again must not double the text.
#[tokio::test]
async fn a_put_onto_a_document_with_sections_is_refused_unless_it_names_after() {
    let jojobot = handler();
    put_ok(&jojobot, "document:alpha", "# One\n\nbody\n").await;
    let refused = put(
        &jojobot,
        put_args("document:alpha", "# Two\n\nmore\n").await,
    )
    .await;
    let body = blocked(&refused);
    assert_eq!(body["fix_by"], "change");
    let said = body.to_string();
    assert!(
        said.contains("after") && said.contains("update_fact"),
        "{said}"
    );
    assert_eq!(records(&jojobot, "document:alpha").await.len(), 1);
    assert_eq!(
        read_all(&jojobot, "document:alpha").await,
        "# One\n\nbody\n"
    );
}

/// **`after` places new sections right behind the section it names**, in the
/// middle of the document as well as at its end; and never between the halves of
/// a section that was cut for size.
#[tokio::test]
async fn after_places_the_new_sections_behind_the_one_it_names() {
    let jojobot = handler();
    put_ok(
        &jojobot,
        "document:alpha",
        "# One\n\none\n\n# Three\n\nthree\n",
    )
    .await;
    let one = records(&jojobot, "document:alpha").await[0]["address"]
        .as_str()
        .expect("address")
        .to_string();
    let answered = json_of(
        &put(
            &jojobot,
            PutDocumentArgs {
                after: Some(one),
                ..put_args("document:alpha", "# Two\n\ntwo\n\n").await
            },
        )
        .await,
    );
    assert_ne!(answered["status"], "blocked", "{answered}");
    assert_eq!(
        read_all(&jojobot, "document:alpha").await,
        "# One\n\none\n\n# Two\n\ntwo\n\n# Three\n\nthree\n"
    );
}

/// **`after` must name a live section of this document.** A record of another
/// thing, and an address that names nothing, are refused with nothing written.
#[tokio::test]
async fn after_naming_something_that_is_not_a_section_here_is_refused() {
    let jojobot = handler();
    put_ok(&jojobot, "document:alpha", "# One\n\nbody\n").await;
    put_ok(&jojobot, "document:beta", "# Other\n\nbody\n").await;
    let other = records(&jojobot, "document:beta").await[0]["address"]
        .as_str()
        .expect("address")
        .to_string();
    for after in [other.as_str(), "document:alpha#f999"] {
        let refused = put(
            &jojobot,
            PutDocumentArgs {
                after: Some(after.into()),
                ..put_args("document:alpha", "# Two\n").await
            },
        )
        .await;
        blocked(&refused);
    }
    assert_eq!(
        read_all(&jojobot, "document:alpha").await,
        "# One\n\nbody\n"
    );
}

/// **A document that does not exist is not created by writing onto it**, and a
/// thing of another kind is not a document.
#[tokio::test]
async fn a_put_creates_nothing_and_writes_only_onto_a_document() {
    let jojobot = handler();
    let missing = put(&jojobot, put_args("document:alpha", "# One\n").await).await;
    assert_eq!(blocked(&missing)["status"], "blocked");
    ensure(&jojobot, "place:springfield").await;
    let wrong = put(&jojobot, put_args("place:springfield", "# One\n").await).await;
    blocked(&wrong);
    let listed = jojobot
        .list_entities(Parameters(ListEntitiesArgs {
            kind: Some("document".into()),
            parent: None,
            offset: None,
            sid: Some(TEST_SID.into()),
        }))
        .await
        .expect("list answers");
    assert_eq!(json_of(&listed)["count"], 0, "{listed:?}");
}

/// **A text with nothing to put is refused by name.**
#[tokio::test]
async fn an_empty_or_blank_text_or_a_lone_carriage_return_is_refused() {
    let jojobot = handler();
    ensure(&jojobot, "document:alpha").await;
    for text in ["", "  \n \n", "# One\rtwo"] {
        let refused = put(&jojobot, put_args("document:alpha", text).await).await;
        blocked(&refused);
    }
    assert!(records(&jojobot, "document:alpha").await.is_empty());
}

/// **Provenance rides on every section; an observation names where it was
/// read.** Without `read_from` the call is refused before any section is
/// written.
#[tokio::test]
async fn an_observation_without_a_source_is_refused_and_one_with_it_is_stored() {
    let jojobot = handler();
    ensure(&jojobot, "document:alpha").await;
    let refused = put(
        &jojobot,
        PutDocumentArgs {
            provenance: Some("observation".into()),
            ..put_args("document:alpha", "# One\n\n# Two\n").await
        },
    )
    .await;
    blocked(&refused);
    assert!(records(&jojobot, "document:alpha").await.is_empty());
    let answered = json_of(
        &put(
            &jojobot,
            PutDocumentArgs {
                provenance: Some("observation".into()),
                source: Some("the wiki".into()),
                ..put_args("document:alpha", "# One\n\n# Two\n").await
            },
        )
        .await,
    );
    assert_ne!(answered["status"], "blocked", "{answered}");
    for held in records(&jojobot, "document:alpha").await {
        assert_eq!(held["provenance"], "observation");
        assert_eq!(held["fields"]["read_from"], "the wiki");
    }
}

/// **A handle in prose that names nothing refuses the whole put, before the
/// first section is written.** The same handle inside a fence is code and passes
/// (the round-trip case above).
#[tokio::test]
async fn a_handle_in_prose_that_names_nothing_refuses_the_put_with_nothing_landed() {
    let jojobot = handler_mentioning();
    ensure(&jojobot, "document:homer").await;
    let text = "# One\n\nfine\n\n# Two\n\nsee @person:ralph here\n\n# Three\n\nlast\n";
    let refused = put(&jojobot, put_args("document:homer", text).await).await;
    let body = blocked(&refused);
    assert!(body.to_string().contains("person:ralph"), "{body}");
    assert!(records(&jojobot, "document:homer").await.is_empty());
}

/// **A store that stops partway leaves the sections that landed, says which they
/// are and where the rest begins, and the rest goes in after the last of them.**
/// Nothing is taken back behind the caller.
#[tokio::test]
async fn a_store_that_stops_midway_names_what_landed_and_where_the_rest_begins() {
    let inner = Arc::new(InMemoryMemory::seeded());
    let failing = Jojobot::new(
        Arc::new(FailsOnCapture {
            inner: inner.clone(),
            fails_from: 3,
            seen: Default::default(),
            edits_fail_from: usize::MAX,
            edits_seen: Default::default(),
        }),
        Arc::new(SpySearch::default()),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    );
    ensure(&failing, "document:homer").await;
    let text = "# One\n\nfirst\n\n# Two\n\nsecond\n\n# Three\n\nthird\n";
    let stopped = failing
        .put_document(Parameters(put_args("document:homer", text).await))
        .await
        .expect_err("a store that stopped is an error");
    let body = stopped.data.clone().expect("the error carries data");
    assert_eq!(body["fix_by"], "retry", "{body}");
    assert_eq!(body["landed"].as_array().map(Vec::len), Some(2), "{body}");
    assert_eq!(body["not_written"], 1);
    let from = body["rest_begins_at_byte"].as_u64().expect("a byte offset") as usize;
    assert_eq!(&text[..from], "# One\n\nfirst\n\n# Two\n\nsecond\n\n");
    assert!(
        stopped.message.contains("landed and stay"),
        "the words say what is true: {}",
        stopped.message
    );
    assert_eq!(
        read_all(&failing, "document:homer").await,
        &text[..from],
        "what landed stays, and nothing was taken back"
    );

    // The store is back. The rest goes in behind the last section that landed.
    let healthy = handler_over(inner);
    let landed = body["landed"][1].as_str().expect("an address").to_string();
    let finished = json_of(
        &put(
            &healthy,
            PutDocumentArgs {
                after: Some(landed),
                ..put_args("document:homer", &text[from..]).await
            },
        )
        .await,
    );
    assert_ne!(finished["status"], "blocked", "{finished}");
    assert_eq!(read_all(&healthy, "document:homer").await, text);
}

/// **A write that landed is never reported as failed.** A store whose fold falls
/// behind after each write still has every section, and the put says so.
#[tokio::test]
async fn a_fold_that_falls_behind_does_not_make_the_put_a_failure() {
    let inner = Arc::new(InMemoryMemory::seeded());
    let jojobot = Jojobot::new(
        Arc::new(FoldBehindMemory(inner)),
        Arc::new(SpySearch::default()),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    );
    let answered = put_ok(&jojobot, "document:homer", "# One\n\na\n\n# Two\n\nb\n").await;
    assert_eq!(answered["sections"], 2);
    assert_eq!(answered["fold"]["behind"], "stale");
    // **A write that landed whose fold is behind cannot say what it changed**,
    // and says so rather than listing nothing.
    assert_eq!(answered["changed"], "not_read", "{answered}");
    assert_eq!(
        read_all(&jojobot, "document:homer").await,
        "# One\n\na\n\n# Two\n\nb\n"
    );
}

/// **New sections go behind a section's cut chunks, never between them.**
#[tokio::test]
async fn after_a_cut_section_lands_behind_its_last_chunk() {
    let jojobot = handler();
    let paragraph = "word ".repeat(400);
    let mut big = String::from("# Big\n\n");
    for _ in 0..60 {
        big.push_str(paragraph.trim_end());
        big.push_str("\n\n");
    }
    let text = format!("{big}# Last\n\nend\n");
    put_ok(&jojobot, "document:homer", &text).await;
    let held = records(&jojobot, "document:homer").await;
    assert!(held.len() >= 3, "the big section was cut");
    let head = held[0]["address"].as_str().expect("address").to_string();
    let placed = json_of(
        &put(
            &jojobot,
            PutDocumentArgs {
                after: Some(head),
                ..put_args("document:homer", "# Inserted\n\nnew\n\n").await
            },
        )
        .await,
    );
    assert_ne!(placed["status"], "blocked", "{placed}");
    assert_eq!(
        read_all(&jojobot, "document:homer").await,
        format!("{big}# Inserted\n\nnew\n\n# Last\n\nend\n")
    );
}

/// **An anchor that matches nowhere, or in two places, is refused, and the
/// section stays as it was**; so is an edit built on a revision that is no
/// longer the section's.
#[tokio::test]
async fn an_anchor_matching_zero_or_two_places_or_a_stale_revision_is_refused() {
    let jojobot = handler_with_mentions();
    put_ok(&jojobot, "document:alpha", "# One\n\nsame same\n").await;
    let address = records(&jojobot, "document:alpha").await[0]["address"]
        .as_str()
        .expect("address")
        .to_string();
    for (find, found) in [("absent", "0 times"), ("same", "2 times")] {
        let refused = jojobot
            .update_fact(Parameters(UpdateFactArgs {
                find: Some(find.into()),
                replace: Some("x".into()),
                field: Some("details".into()),
                ..update_args(&address)
            }))
            .await
            .expect("update_fact answers");
        // Refused for the count, not for a store that cannot apply the edit.
        let said = blocked(&refused).to_string();
        assert!(said.contains(found), "{said}");
    }
    let stale = jojobot
        .update_fact(Parameters(UpdateFactArgs {
            details: Some("changed".into()),
            expected_revision: Some(7),
            ..update_args(&address)
        }))
        .await
        .expect("update_fact answers");
    blocked(&stale);
    assert_eq!(
        read_all(&jojobot, "document:alpha").await,
        "# One\n\nsame same\n"
    );
}

/// **A document longer than one answer is read in parts, and each part says what
/// it left out.** Concatenated, the parts are the file; no answer is over the
/// ceiling.
#[tokio::test]
async fn a_long_document_is_read_in_parts_that_name_what_they_left_out() {
    let jojobot = handler();
    let mut text = String::new();
    for n in 0..40 {
        text.push_str(&format!(
            "## Part {n}\n\n{}\n\n",
            "word ".repeat(200).trim_end()
        ));
    }
    put_ok(&jojobot, "document:alpha", &text).await;
    let first = read_part(&jojobot, "document:alpha", None).await;
    assert!(
        first["next_offset"].as_u64().is_some_and(|n| n > 0),
        "{first}"
    );
    assert!(first["not_shown"]["count"].as_u64().is_some_and(|n| n > 0));
    assert_eq!(first["sections"], 40);
    assert!(first.to_string().chars().count() <= ANSWER_CEILING);
    let mut joined = String::new();
    let mut offset = None;
    let mut parts = 0;
    loop {
        let part = read_part(&jojobot, "document:alpha", offset).await;
        assert!(
            part.to_string().chars().count() + crate::answer::STATUS_BAR_ROOM <= ANSWER_CEILING,
            "a part is over the ceiling"
        );
        joined.push_str(part["markdown"].as_str().expect("markdown"));
        parts += 1;
        match part["next_offset"].as_u64() {
            Some(next) => offset = Some(next as usize),
            None => break,
        }
    }
    assert!(parts > 1);
    assert_eq!(joined, text);
}

/// **A section too long for one record is cut at the last blank line that
/// fits**, every cut chunk says it carries on, and the file reads back whole.
#[tokio::test]
async fn a_section_too_long_for_a_record_is_cut_at_blank_lines_and_reads_back_whole() {
    let jojobot = handler();
    let paragraph = "word ".repeat(400);
    let mut text = String::from("# Big\n\n");
    for _ in 0..60 {
        text.push_str(paragraph.trim_end());
        text.push_str("\n\n");
    }
    let answered = put_ok(&jojobot, "document:alpha", &text).await;
    assert!(
        answered["cut_for_size"].as_u64().is_some_and(|n| n >= 1),
        "{answered}"
    );
    let held = records(&jojobot, "document:alpha").await;
    assert!(held.len() >= 2);
    assert!(held[0]["fields"].get("continues").is_none());
    let heading_id = held[0]["address"]
        .as_str()
        .and_then(|address| address.rsplit('#').next())
        .expect("an id")
        .to_string();
    assert!(
        held[1..]
            .iter()
            .all(|fact| fact["fields"]["continues"] == heading_id),
        "a chunk is linked to the record that holds the heading: {held:?}"
    );
    assert!(held[1..].iter().all(|fact| fact["fields"]["level"] == "0"));
    assert_eq!(read_all(&jojobot, "document:alpha").await, text);
}

/// **Archiving a section takes it out of the page and the read counts it.**
#[tokio::test]
async fn an_archived_section_is_left_out_of_the_page_and_counted() {
    let jojobot = handler();
    put_ok(
        &jojobot,
        "document:alpha",
        "# One\n\none\n\n# Two\n\ntwo\n\n# Three\n\nthree\n",
    )
    .await;
    let two = records(&jojobot, "document:alpha").await[1]["address"]
        .as_str()
        .expect("address")
        .to_string();
    let archived = jojobot
        .update_fact(Parameters(UpdateFactArgs {
            status: Some("archived".into()),
            details: Some("dropped from the file".into()),
            ..update_args(&two)
        }))
        .await
        .expect("update_fact answers");
    assert_ne!(json_of(&archived)["status"], "blocked", "{archived:?}");
    // An ordinary claim about the document, archived, is not a section left out.
    let remark = capture_ok(
        &jojobot,
        capture_args("document:alpha", "reviewed by a colleague"),
    )
    .await;
    let remark = remark["address"].as_str().expect("address").to_string();
    let archived_remark = jojobot
        .update_fact(Parameters(UpdateFactArgs {
            status: Some("archived".into()),
            details: Some("no longer so".into()),
            ..update_args(&remark)
        }))
        .await
        .expect("update_fact answers");
    assert_ne!(
        json_of(&archived_remark)["status"],
        "blocked",
        "{archived_remark:?}"
    );
    let part = read_part(&jojobot, "document:alpha", None).await;
    assert_eq!(part["markdown"], "# One\n\none\n\n# Three\n\nthree\n");
    assert_eq!(part["archived_sections_left_out"], 1);
    assert_eq!(part["sections"], 2);
}

/// **A caller that has not booted cannot put a document**, as for every other
/// write.
#[tokio::test]
async fn a_put_from_nobody_is_refused() {
    let jojobot = handler();
    ensure(&jojobot, "document:alpha").await;
    let refused = put(
        &jojobot,
        PutDocumentArgs {
            sid: None,
            ..put_args("document:alpha", "# One\n").await
        },
    )
    .await;
    assert!(
        text_of(&refused).contains("start_here"),
        "an unidentified write must point at the door: {}",
        text_of(&refused)
    );
    assert!(records(&jojobot, "document:alpha").await.is_empty());
}

/// **A handle in prose that names something is a link, and a document reads back
/// the handle its thing wears.** Outside code the mention is stored as the
/// permanent id; inside a fence it stays the characters typed.
#[tokio::test]
async fn a_mention_in_prose_round_trips_and_one_in_a_fence_stays_literal() {
    let jojobot = handler_mentioning();
    ensure(&jojobot, "place:springfield").await;
    let text = "# Visit\n\nWe went to @place:springfield today.\n\n```\n@place:springfield\n```\n";
    put_ok(&jojobot, "document:alpha", text).await;
    assert_eq!(read_all(&jojobot, "document:alpha").await, text);
}

/// **The document's own fields ride beside the text, never in it.**
#[tokio::test]
async fn the_documents_own_fields_ride_beside_the_text() {
    let jojobot = handler();
    put_ok(&jojobot, "document:homer", "# One\n\nbody\n").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [
                    ("one_liner".to_string(), "Notes for the shop".to_string()),
                    (
                        "resource".to_string(),
                        "https://example.test/notes".to_string(),
                    ),
                ]
                .into(),
            ),
            ..capture_args("document:homer", "set the document's own fields")
        },
    )
    .await;
    let part = read_part(&jojobot, "document:homer", None).await;
    assert_eq!(part["fields"]["one_liner"], "Notes for the shop");
    assert_eq!(part["fields"]["resource"], "https://example.test/notes");
    assert_eq!(part["markdown"], "# One\n\nbody\n");
}

/// **Only a document is read as one, and an offset past the end is an empty
/// page rather than a failure.**
#[tokio::test]
async fn a_thing_that_is_not_a_document_is_refused_and_an_offset_past_the_end_is_empty() {
    let jojobot = handler();
    ensure(&jojobot, "place:springfield").await;
    let refused = jojobot
        .read_document(Parameters(ReadDocumentArgs {
            document: "place:springfield".into(),
            offset: None,
            as_of: None,
            sid: Some(TEST_SID.into()),
        }))
        .await
        .expect("read_document answers");
    blocked(&refused);
    put_ok(&jojobot, "document:homer", "# One\n\nbody\n").await;
    let past = read_part(&jojobot, "document:homer", Some(99)).await;
    assert_eq!(past["markdown"], "");
    assert!(past.get("next_offset").is_none(), "{past}");
    assert_eq!(past["sections"], 1);
}

async fn address_of(jojobot: &Jojobot, document: &str, first_line: &str) -> String {
    records(jojobot, document)
        .await
        .iter()
        .find(|fact| fact["content"] == first_line)
        .unwrap_or_else(|| panic!("no section headed {first_line}"))["address"]
        .as_str()
        .expect("an address")
        .to_string()
}

async fn move_section(jojobot: &Jojobot, address: &str, after: &str) -> CallToolResult {
    jojobot
        .update_fact(Parameters(UpdateFactArgs {
            after: Some(after.into()),
            ..update_args(address)
        }))
        .await
        .expect("update_fact answers")
}

/// **A section moves behind another, and nothing else is written.** The negative
/// half: the sections that did not move keep their revision, so no other record
/// was touched, and the bytes of every section are the ones put.
#[tokio::test]
async fn a_section_moves_behind_another_and_no_other_record_is_written() {
    let jojobot = handler();
    put_ok(
        &jojobot,
        "document:alpha",
        "# One\n\nfirst\n\n# Two\n\nsecond\n\n# Three\n\nthird\n",
    )
    .await;
    let one = address_of(&jojobot, "document:alpha", "# One").await;
    let three = address_of(&jojobot, "document:alpha", "# Three").await;
    let moved = move_section(&jojobot, &one, &three).await;
    assert_ne!(json_of(&moved)["status"], "blocked", "{moved:?}");
    assert_eq!(
        read_all(&jojobot, "document:alpha").await,
        "# Two\n\nsecond\n\n# Three\n\nthird\n# One\n\nfirst\n\n"
    );
    for held in records(&jojobot, "document:alpha").await {
        let moved = held["content"] == "# One";
        assert_eq!(
            held.get("revision_count").is_some(),
            moved,
            "only the moved section was written twice: {held}"
        );
    }
}

/// **A section addressed by a name its document wore once moves like any
/// other**, and the section it goes behind may be addressed by the same former
/// name. The document is renamed after it is put, so every address a caller
/// holds from before names a handle nothing wears now. The move by the current
/// name is the case above, and the document is read back by its current name, so
/// the case cannot pass by moving nothing.
#[tokio::test]
async fn a_section_addressed_by_a_former_handle_moves_behind_the_right_section() {
    let jojobot = handler();
    let sid = writing_as(&jojobot);
    put_ok(
        &jojobot,
        "document:alpha",
        "# One\n\nfirst\n\n# Two\n\nsecond\n\n# Three\n\nthird\n",
    )
    .await;
    let one = address_of(&jojobot, "document:alpha", "# One").await;
    let three = address_of(&jojobot, "document:alpha", "# Three").await;
    jojobot
        .rename_entity(Parameters(crate::memory::RenameEntityArgs {
            handle: "document:alpha".into(),
            to: "document:beta".into(),
            parent: None,
            recorded_at: None,
            override_token: None,
            override_reason: None,
            sid: Some(sid),
        }))
        .await
        .expect("rename_entity answers");

    // Both addresses carry the name the document wore before the rename.
    assert!(one.starts_with("document:alpha#"), "{one}");
    let moved = move_section(&jojobot, &one, &three).await;
    assert_ne!(json_of(&moved)["status"], "blocked", "{moved:?}");
    assert_eq!(
        read_all(&jojobot, "document:beta").await,
        "# Two\n\nsecond\n\n# Three\n\nthird\n# One\n\nfirst\n\n",
        "the section landed behind the one the former name pointed at"
    );
}

/// **A section can be moved and edited in one call.**
#[tokio::test]
async fn a_section_can_be_moved_and_edited_in_one_call() {
    let jojobot = handler_with_mentions();
    put_ok(
        &jojobot,
        "document:alpha",
        "# One\n\nfirst\n\n# Two\n\nsecond\n\n",
    )
    .await;
    let one = address_of(&jojobot, "document:alpha", "# One").await;
    let two = address_of(&jojobot, "document:alpha", "# Two").await;
    let answered = jojobot
        .update_fact(Parameters(UpdateFactArgs {
            after: Some(two),
            find: Some("first".into()),
            replace: Some("changed".into()),
            field: Some("details".into()),
            ..update_args(&one)
        }))
        .await
        .expect("update_fact answers");
    assert_ne!(json_of(&answered)["status"], "blocked", "{answered:?}");
    assert_eq!(
        read_all(&jojobot, "document:alpha").await,
        "# Two\n\nsecond\n\n# One\n\nchanged\n\n"
    );
}

/// **A cut section moves with the chunks that carry it on.**
#[tokio::test]
async fn a_cut_section_moves_with_its_chunks() {
    let jojobot = handler();
    let paragraph = "word ".repeat(400);
    let mut big = String::from("# Big\n\n");
    for _ in 0..60 {
        big.push_str(paragraph.trim_end());
        big.push_str("\n\n");
    }
    let text = format!("{big}# Small\n\nsmall\n\n# Last\n\nend\n");
    put_ok(&jojobot, "document:alpha", &text).await;
    assert!(records(&jojobot, "document:alpha").await.len() >= 4);
    let big_head = address_of(&jojobot, "document:alpha", "# Big").await;
    let last = address_of(&jojobot, "document:alpha", "# Last").await;
    let moved = move_section(&jojobot, &big_head, &last).await;
    assert_ne!(json_of(&moved)["status"], "blocked", "{moved:?}");
    assert_eq!(
        read_all(&jojobot, "document:alpha").await,
        format!("# Small\n\nsmall\n\n# Last\n\nend\n{big}")
    );
}

/// **A move that cannot be made is refused and writes nothing**: an ordinary
/// claim has no place to move, a chunk is moved with its section, a section
/// cannot follow itself, and the target must be a section of this document.
#[tokio::test]
async fn a_move_that_cannot_be_made_is_refused_with_the_document_as_it_was() {
    let jojobot = handler();
    let paragraph = "word ".repeat(400);
    let mut big = String::from("# Big\n\n");
    for _ in 0..60 {
        big.push_str(paragraph.trim_end());
        big.push_str("\n\n");
    }
    let text = format!("{big}# Last\n\nend\n");
    put_ok(&jojobot, "document:alpha", &text).await;
    put_ok(&jojobot, "document:beta", "# Other\n\nbody\n").await;
    let remark = capture_ok(
        &jojobot,
        capture_args("document:alpha", "reviewed by a colleague"),
    )
    .await["address"]
        .as_str()
        .expect("address")
        .to_string();
    let big_head = address_of(&jojobot, "document:alpha", "# Big").await;
    let last = address_of(&jojobot, "document:alpha", "# Last").await;
    let other = address_of(&jojobot, "document:beta", "# Other").await;
    let chunk = records(&jojobot, "document:alpha").await[1]["address"]
        .as_str()
        .expect("address")
        .to_string();
    for (what, address, after) in [
        ("an ordinary claim", &remark, &last),
        ("a chunk on its own", &chunk, &last),
        ("a section behind itself", &big_head, &big_head),
        ("a section behind its own chunk", &big_head, &chunk),
        ("a target in another document", &last, &other),
        ("a target that is no section", &last, &remark),
    ] {
        let refused = move_section(&jojobot, address, after).await;
        let body = blocked(&refused);
        assert!(
            body.to_string().contains("after"),
            "{what} was refused without saying what `after` is for: {body}"
        );
    }
    assert_eq!(read_all(&jojobot, "document:alpha").await, text);
}

/// **A move that stops partway says what moved, and the same call finishes it.**
#[tokio::test]
async fn a_move_that_stops_partway_is_finished_by_sending_it_again() {
    let paragraph = "word ".repeat(400);
    let mut big = String::from("# Big\n\n");
    for _ in 0..60 {
        big.push_str(paragraph.trim_end());
        big.push_str("\n\n");
    }
    let text = format!("{big}# Last\n\nend\n");
    let inner = Arc::new(InMemoryMemory::seeded());
    let healthy = handler_over(inner.clone());
    put_ok(&healthy, "document:alpha", &text).await;
    let big_head = address_of(&healthy, "document:alpha", "# Big").await;
    let last = address_of(&healthy, "document:alpha", "# Last").await;

    let failing = Jojobot::new(
        Arc::new(FailsOnCapture {
            inner: inner.clone(),
            fails_from: usize::MAX,
            seen: Default::default(),
            edits_fail_from: 2,
            edits_seen: Default::default(),
        }),
        Arc::new(SpySearch::default()),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    );
    let stopped = failing
        .update_fact(Parameters(UpdateFactArgs {
            after: Some(last.clone()),
            ..update_args(&big_head)
        }))
        .await
        .expect_err("a store that stopped is an error");
    let data = stopped.data.clone().expect("the error carries data");
    assert_eq!(data["moved"].as_array().map(Vec::len), Some(1), "{data}");
    assert!(data["not_moved"].as_u64().is_some_and(|n| n >= 1), "{data}");
    assert!(
        stopped.message.contains("same call again"),
        "{}",
        stopped.message
    );

    let finished = move_section(&healthy, &big_head, &last).await;
    assert_ne!(json_of(&finished)["status"], "blocked", "{finished:?}");
    assert_eq!(
        read_all(&healthy, "document:alpha").await,
        format!("# Last\n\nend\n{big}")
    );
}

/// **Archiving a cut section's heading takes the whole section out**, chunks
/// included, and the read counts every record it left out.
#[tokio::test]
async fn archiving_a_cut_sections_heading_takes_its_chunks_with_it() {
    let jojobot = handler();
    let paragraph = "word ".repeat(400);
    let mut big = String::from("# Big\n\n");
    for _ in 0..60 {
        big.push_str(paragraph.trim_end());
        big.push_str("\n\n");
    }
    put_ok(&jojobot, "document:alpha", &format!("{big}# Last\n\nend\n")).await;
    let all = records(&jojobot, "document:alpha").await.len();
    let head = address_of(&jojobot, "document:alpha", "# Big").await;
    let archived = jojobot
        .update_fact(Parameters(UpdateFactArgs {
            status: Some("archived".into()),
            details: Some("dropped".into()),
            ..update_args(&head)
        }))
        .await
        .expect("update_fact answers");
    assert_ne!(json_of(&archived)["status"], "blocked", "{archived:?}");
    let part = read_part(&jojobot, "document:alpha", None).await;
    assert_eq!(part["markdown"], "# Last\n\nend\n");
    assert_eq!(part["archived_sections_left_out"], all - 1);
}

/// **A section moved behind a cut section lands behind its last chunk**, never
/// between the pieces.
#[tokio::test]
async fn a_section_moved_behind_a_cut_section_lands_after_its_chunks() {
    let jojobot = handler();
    let paragraph = "word ".repeat(400);
    let mut big = String::from("# Big\n\n");
    for _ in 0..60 {
        big.push_str(paragraph.trim_end());
        big.push_str("\n\n");
    }
    let text = format!("# Small\n\nsmall\n\n{big}# Last\n\nend\n");
    put_ok(&jojobot, "document:alpha", &text).await;
    let small = address_of(&jojobot, "document:alpha", "# Small").await;
    let big_head = address_of(&jojobot, "document:alpha", "# Big").await;
    let moved = move_section(&jojobot, &small, &big_head).await;
    assert_ne!(json_of(&moved)["status"], "blocked", "{moved:?}");
    assert_eq!(
        read_all(&jojobot, "document:alpha").await,
        format!("{big}# Small\n\nsmall\n\n# Last\n\nend\n")
    );
}

/// **A `#` line inside the frontmatter block is a line of the block**, through the
/// served surface: the block is one section and the heading after it is the
/// next.
#[tokio::test]
async fn a_hash_line_inside_frontmatter_stays_in_the_first_section() {
    let jojobot = handler();
    let text = "---\ntitle: Notes\n# internal note\n---\n\n# Start\nbody\n";
    let answered = put_ok(&jojobot, "document:alpha", text).await;
    assert_eq!(answered["sections"], 2, "{answered}");
    let held = records(&jojobot, "document:alpha").await;
    assert_eq!(held[0]["content"], "---");
    assert_eq!(held[1]["content"], "# Start");
    assert_eq!(read_all(&jojobot, "document:alpha").await, text);
}

/// **Another session may move a section of the operator's words, and may not
/// rewrite them.** The positive and the negative on one store: a move by a
/// second session lands, an edit of the heading by the same session is refused, and
/// so is a caller's own value for `order`.
#[tokio::test]
async fn another_session_may_move_a_testimony_section_but_not_rewrite_it() {
    let jojobot = handler();
    ensure(&jojobot, "document:alpha").await;
    let wrote = json_of(
        &put(
            &jojobot,
            PutDocumentArgs {
                provenance: Some("testimony".into()),
                ..put_args("document:alpha", "# One\n\nfirst\n\n# Two\n\nsecond\n").await
            },
        )
        .await,
    );
    assert_ne!(wrote["status"], "blocked", "{wrote}");
    let one = address_of(&jojobot, "document:alpha", "# One").await;
    let two = address_of(&jojobot, "document:alpha", "# Two").await;
    let other = as_bot(&jojobot, "gamma");

    let moved = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                after: Some(two.clone()),
                sid: Some(other.clone()),
                ..update_args(&one)
            }))
            .await
            .expect("update_fact answers"),
    );
    assert_ne!(moved["status"], "blocked", "the move lands: {moved}");
    assert_eq!(
        read_all(&jojobot, "document:alpha").await,
        "# Two\n\nsecond\n# One\n\nfirst\n\n"
    );

    let edited = jojobot
        .update_fact(Parameters(UpdateFactArgs {
            content: Some("# One, said otherwise".into()),
            provenance: Some("inference".into()),
            sid: Some(other.clone()),
            ..update_args(&one)
        }))
        .await
        .expect("update_fact answers");
    assert_eq!(blocked(&edited)["fix_by"], "change");
    let moved_and_edited = jojobot
        .update_fact(Parameters(UpdateFactArgs {
            after: Some(one.clone()),
            content: Some("# Two, said otherwise".into()),
            provenance: Some("inference".into()),
            sid: Some(other.clone()),
            ..update_args(&two)
        }))
        .await
        .expect("update_fact answers");
    blocked(&moved_and_edited);
    for (what, args) in [
        (
            "fields",
            UpdateFactArgs {
                fields: Some([("order".to_string(), "a".to_string())].into()),
                ..update_args(&one)
            },
        ),
        (
            "sets",
            UpdateFactArgs {
                sets: Some([("order".to_string(), "a".to_string())].into()),
                ..update_args(&one)
            },
        ),
        (
            "clear_fields",
            UpdateFactArgs {
                clear_fields: Some(vec!["order".to_string()]),
                ..update_args(&one)
            },
        ),
    ] {
        let refused = jojobot
            .update_fact(Parameters(UpdateFactArgs {
                sid: Some(other.clone()),
                ..args
            }))
            .await
            .expect("update_fact answers");
        assert!(
            blocked(&refused).to_string().contains("after"),
            "a caller's own order in {what} is refused with the way to move"
        );
    }
    assert_eq!(
        read_all(&jojobot, "document:alpha").await,
        "# Two\n\nsecond\n# One\n\nfirst\n\n",
        "nothing of the refused calls was written"
    );
}

/// A fact hit on a record of a thing, as the index ranks it.
async fn hit_on(jojobot: &Jojobot, thing: &str, first_line: &str) -> Hit {
    let id = EntityId(thing.into());
    let fact = jojobot
        .memory
        .recall(&id)
        .await
        .expect("the thing is there")
        .into_iter()
        .find(|fact| fact.content == first_line)
        .unwrap_or_else(|| panic!("no record {first_line} on {thing}"));
    let entity = jojobot
        .memory
        .list_entities(None)
        .await
        .expect("entities")
        .into_iter()
        .find(|entity| entity.id == id)
        .expect("the entity");
    let home = jojobot_domain::memory::search::EntityRef::resolved(&entity);
    Hit::Fact {
        fact: Box::new(fact),
        subject: home.clone(),
        home,
        source: None,
        superseded_by: None,
    }
}

fn search_for_documents() -> SearchArgs {
    SearchArgs {
        query: Some("shop".into()),
        sid: Some(TEST_SID.into()),
        ..search_args()
    }
}

/// **Hits on sections of one document are grouped under the best of them**, and
/// each section hit says which document it is part of. An ordinary claim and the
/// rest of the ranking keep their places.
#[tokio::test]
async fn hits_on_sections_of_one_document_are_grouped_and_say_which_document() {
    let spy = Arc::new(SpySearch::default());
    let jojobot = handler_with(spy.clone());
    put_ok(
        &jojobot,
        "document:alpha",
        "# Alpha one\n\nshop\n\n# Alpha two\n\nshop\n",
    )
    .await;
    put_ok(
        &jojobot,
        "document:beta",
        "# Beta one\n\nshop\n\n# Beta two\n\nshop\n",
    )
    .await;
    capture_ok(&jojobot, capture_args("person:homer", "runs the shop")).await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some([("one_liner".to_string(), "Notes for the shop".to_string())].into()),
            ..capture_args("document:alpha", "set the one liner")
        },
    )
    .await;
    spy.answer_with(vec![
        hit_on(&jojobot, "document:alpha", "# Alpha one").await,
        hit_on(&jojobot, "person:homer", "runs the shop").await,
        hit_on(&jojobot, "document:beta", "# Beta one").await,
        hit_on(&jojobot, "document:alpha", "set the one liner").await,
        hit_on(&jojobot, "document:alpha", "# Alpha two").await,
        hit_on(&jojobot, "document:beta", "# Beta two").await,
    ]);
    let body = json_of(
        &jojobot
            .search(Parameters(search_for_documents()))
            .await
            .expect("search answers"),
    );
    let results = body["results"].as_array().expect("results");
    let order: Vec<&str> = results
        .iter()
        .map(|hit| hit["content"].as_str().expect("a fact hit"))
        .collect();
    assert_eq!(
        order,
        [
            "# Alpha one",
            "# Alpha two",
            "runs the shop",
            "# Beta one",
            "# Beta two",
            "set the one liner"
        ]
    );
    assert_eq!(results[0]["document"]["handle"], "document:alpha");
    assert_eq!(results[0]["document"]["name"], "alpha");
    assert_eq!(results[0]["document"]["one_liner"], "Notes for the shop");
    assert_eq!(results[1]["document"]["handle"], "document:alpha");
    assert_eq!(results[3]["document"]["handle"], "document:beta");
    assert!(results[3]["document"].get("one_liner").is_none());
    assert!(
        results[2].get("document").is_none(),
        "an ordinary claim is no part of a document: {}",
        results[2]
    );
    assert!(
        results[5].get("document").is_none(),
        "an ordinary claim about a document is no part of its text: {}",
        results[5]
    );
}

/// **A section hit names the call that reads its whole document**, once in the
/// answer however many sections matched, and following it returns the file. An
/// answer with no section hit carries no such line.
#[tokio::test]
async fn a_section_hit_names_the_call_that_reads_the_whole_document() {
    let spy = Arc::new(SpySearch::default());
    let jojobot = handler_with(spy.clone());
    put_ok(
        &jojobot,
        "document:alpha",
        "# Alpha one\n\nshop\n\n# Alpha two\n\nshop\n",
    )
    .await;
    capture_ok(&jojobot, capture_args("person:homer", "runs the shop")).await;
    spy.answer_with(vec![
        hit_on(&jojobot, "document:alpha", "# Alpha one").await,
        hit_on(&jojobot, "document:alpha", "# Alpha two").await,
    ]);
    let body = json_of(
        &jojobot
            .search(Parameters(search_for_documents()))
            .await
            .expect("search answers"),
    );
    let results = body["results"].as_array().expect("results");
    assert_eq!(results.len(), 2, "both sections matched: {body}");
    let handle = results[0]["document"]["handle"]
        .as_str()
        .expect("a section hit names its document");
    let line = body["read_the_document"]
        .as_str()
        .unwrap_or_else(|| panic!("the answer names the call that goes further: {body}"));
    assert!(line.contains("read_document"), "{line}");
    assert!(
        results
            .iter()
            .all(|hit| hit.get("read_the_document").is_none()),
        "the line rides the answer once, not each hit: {body}"
    );
    let whole = read_all(&jojobot, handle).await;
    assert!(
        whole.contains("# Alpha one") && whole.contains("# Alpha two"),
        "the handle in the document block reads the whole file: {whole}"
    );

    spy.answer_with(vec![
        hit_on(&jojobot, "person:homer", "runs the shop").await,
    ]);
    let plain = json_of(
        &jojobot
            .search(Parameters(search_for_documents()))
            .await
            .expect("search answers"),
    );
    assert_eq!(plain["count"], 1, "{plain}");
    assert!(
        plain.get("read_the_document").is_none(),
        "an answer with no section hit has no document to read: {plain}"
    );
}

/// **A walk by the offsets search returns yields every hit once**, though the
/// answer ceiling cuts a page whose ranking interleaves the sections of two
/// documents (A1 B1 A2 B2). Grouping brings A1 and A2 together, so a cut that
/// counted kept hits as the first of the ranking named an offset that repeated
/// A2 and lost B1.
///
/// **Paired in one read**: every hit is seen, which the loss breaks, and none
/// twice, which the repeat breaks. The sections are long enough that the ceiling
/// keeps two of the four, and the case first requires that, or it proves nothing
/// about a cut.
#[tokio::test]
async fn a_walk_by_the_returned_offsets_yields_each_hit_once_across_a_cut_through_grouped_sections()
{
    let spy = Arc::new(SpySearch::default());
    let jojobot = handler_with(spy.clone());
    let section = |heading: &str| format!("# {heading}\n\n{}\n", "shop ".repeat(1_800));
    put_ok(
        &jojobot,
        "document:alpha",
        &format!("{}\n{}", section("A1"), section("A2")),
    )
    .await;
    put_ok(
        &jojobot,
        "document:beta",
        &format!("{}\n{}", section("B1"), section("B2")),
    )
    .await;
    spy.answer_with(vec![
        hit_on(&jojobot, "document:alpha", "# A1").await,
        hit_on(&jojobot, "document:beta", "# B1").await,
        hit_on(&jojobot, "document:alpha", "# A2").await,
        hit_on(&jojobot, "document:beta", "# B2").await,
    ]);

    let mut seen: Vec<String> = Vec::new();
    let mut offset = 0u32;
    let mut pages = 0;
    loop {
        let body = json_of(
            &jojobot
                .search(Parameters(SearchArgs {
                    limit: Some(4),
                    offset: Some(offset),
                    ..search_for_documents()
                }))
                .await
                .expect("search answers"),
        );
        pages += 1;
        let results = body["results"].as_array().expect("results");
        if pages == 1 {
            assert_eq!(
                results.len(),
                2,
                "the ceiling must keep two of the four, or this proves nothing about a cut"
            );
        }
        seen.extend(
            results
                .iter()
                .map(|hit| hit["content"].as_str().expect("a fact hit").to_string()),
        );
        match body["not_shown"]["offset"].as_u64() {
            Some(next) => offset = next as u32,
            None => break,
        }
        assert!(pages < 10, "the walk does not end: {seen:?}");
    }

    let mut sorted = seen.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        ["# A1", "# A2", "# B1", "# B2"],
        "every hit once, none lost and none repeated: {seen:?}"
    );
}

/// **An `order` field on a claim of any other kind makes no section.** A person
/// who keeps an order of their own is not a document, and the hit is served as
/// the ordinary claim it is.
#[tokio::test]
async fn an_order_field_on_a_claim_that_is_no_document_makes_no_section() {
    let spy = Arc::new(SpySearch::default());
    let jojobot = handler_with(spy.clone());
    capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some([("order".to_string(), "1".to_string())].into()),
            ..capture_args("person:homer", "keeps an order of donuts")
        },
    )
    .await;
    spy.answer_with(vec![
        hit_on(&jojobot, "person:homer", "keeps an order of donuts").await,
    ]);
    let body = json_of(
        &jojobot
            .search(Parameters(search_for_documents()))
            .await
            .expect("search answers"),
    );
    assert!(body["results"][0].get("document").is_none(), "{body}");
}

/// **Two connections over one store that gives up the thread where a real one
/// does.** The transport builds a handler per connection and the process shares
/// one registry, so two handlers over one registry are the shape production has.
fn two_connections() -> (Jojobot, Jojobot, Arc<InMemoryMemory>) {
    connections_over(None)
}

/// [`two_connections`], over a store whose next listing of the entities can be
/// held after it has read.
fn connections_over(pause: Option<Arc<ListingPause>>) -> (Jojobot, Jojobot, Arc<InMemoryMemory>) {
    let memory = Arc::new(InMemoryMemory::seeded());
    let store: Arc<dyn Memory> = Arc::new(YieldsLikeAStore {
        inner: memory.clone(),
        pause,
    });
    let registry = seeded_registry();
    let on = || {
        Jojobot::new(
            store.clone(),
            Arc::new(SpySearch::default()),
            Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
            Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            registry.clone(),
        )
    };
    (on(), on(), memory)
}

/// The order keys of a document's live sections, in reading order, with the
/// first line of each.
async fn placed(memory: &Arc<InMemoryMemory>, document: &str) -> Vec<(String, String)> {
    let facts = memory
        .recall(&EntityId::person(document))
        .await
        .expect("the document reads");
    live_sections(&facts)
        .into_iter()
        .map(|held| (held.order.clone(), held.section.first_line.clone()))
        .collect()
}

/// **Two puts after the same section, at the same moment, mint different keys.**
/// Each reads the neighbours, mints between them and writes; with nothing
/// between the read and the write, both read the same neighbours and mint the
/// same key, and the order they land in is the record ids' and neither call's.
#[tokio::test]
async fn two_puts_after_the_same_section_mint_different_keys() {
    let (a, b, memory) = two_connections();
    let document = "document:homer";
    put_ok(&a, document, "# Anchor\n\nanchor\n").await;
    let anchor = address_of(&a, document, "# Anchor").await;

    let after_anchor = |markdown: &'static str| {
        let anchor = anchor.clone();
        async move {
            PutDocumentArgs {
                after: Some(anchor),
                ..put_args(document, markdown).await
            }
        }
    };
    let left_args = after_anchor("# Left\n\nleft\n").await;
    let right_args = after_anchor("# Right\n\nright\n").await;
    let (left, right) = tokio::join!(put(&a, left_args), put(&b, right_args));
    let (left, right) = (json_of(&left), json_of(&right));
    assert_ne!(left["status"], "blocked", "{left}");
    assert_ne!(right["status"], "blocked", "{right}");

    let held = placed(&memory, document).await;
    let lines: Vec<&str> = held.iter().map(|(_, line)| line.as_str()).collect();
    assert_eq!(lines.len(), 3, "the anchor and both sections: {held:?}");
    assert_eq!(lines[0], "# Anchor", "{held:?}");
    assert!(
        lines.contains(&"# Left") && lines.contains(&"# Right"),
        "both sections exist: {held:?}"
    );
    let keys: std::collections::BTreeSet<&str> = held.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys.len(), 3, "every section has its own key: {held:?}");
}

/// **Two moves behind the same section, at the same moment, mint different
/// keys.** The move reads the neighbours and mints between them as a put does.
#[tokio::test]
async fn two_moves_behind_the_same_section_mint_different_keys() {
    let (a, b, memory) = two_connections();
    let document = "document:homer";
    put_ok(
        &a,
        document,
        "# One\n\n1\n\n# Two\n\n2\n\n# Three\n\n3\n\n# Four\n\n4\n",
    )
    .await;
    let one = address_of(&a, document, "# One").await;
    let three = address_of(&a, document, "# Three").await;
    let four = address_of(&a, document, "# Four").await;

    let (first, second) = tokio::join!(
        move_section(&a, &three, &one),
        move_section(&b, &four, &one)
    );
    assert_ne!(json_of(&first)["status"], "blocked", "{first:?}");
    assert_ne!(json_of(&second)["status"], "blocked", "{second:?}");

    let held = placed(&memory, document).await;
    let keys: std::collections::BTreeSet<&str> = held.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(held.len(), 4, "{held:?}");
    assert_eq!(keys.len(), 4, "every section has its own key: {held:?}");
    assert_eq!(held[0].1, "# One", "{held:?}");
}

/// **Placing on one document never makes a call on another wait**, and a call on
/// the same document does wait for the one that holds it.
#[tokio::test]
async fn placing_waits_per_document_and_never_across_documents() {
    let (a, b, _memory) = two_connections();
    ensure(&a, "document:homer").await;
    ensure(&a, "document:marge").await;
    let held = a
        .hold_placing(&EntityId::person("document:homer"))
        .await
        .expect("the lock is taken");

    let other = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        put(&b, put_args("document:marge", "# Elsewhere\n\ne\n").await),
    )
    .await
    .expect("a different document does not wait");
    assert_ne!(json_of(&other)["status"], "blocked", "{other:?}");

    let same_args = put_args("document:homer", "# Here\n\nh\n").await;
    let same = std::pin::pin!(put(&b, same_args));
    let mut same = same;
    let early = tokio::time::timeout(std::time::Duration::from_millis(200), &mut same).await;
    assert!(early.is_err(), "the same document waits for the holder");
    drop(held);
    let done = tokio::time::timeout(std::time::Duration::from_secs(5), same)
        .await
        .expect("it goes on once the holder lets go");
    assert_ne!(json_of(&done)["status"], "blocked", "{done:?}");
}

/// Rename a document through the verb a caller uses.
async fn rename_document(jojobot: &Jojobot, from: &str, to: &str) {
    jojobot
        .rename_entity(Parameters(crate::memory::rename_entity::RenameEntityArgs {
            handle: from.into(),
            to: to.into(),
            parent: None,
            recorded_at: None,
            override_token: None,
            override_reason: None,
            sid: Some(TEST_SID.into()),
        }))
        .await
        .expect("rename_entity answers");
}

/// **Every name a document has answers to one placing lock.** The lock is held
/// under the name the document wears now; a move addressed by the name it used
/// to wear waits for it (positive), and a put on another document does not
/// (negative). Keyed on the handle as sent, the move takes another lock and goes
/// straight in.
#[tokio::test]
async fn a_move_under_a_former_name_waits_for_the_lock_held_under_the_current_one() {
    let (a, b, _memory) = two_connections();
    let (former, current) = ("document:homer", "document:krusty");
    put_ok(&a, former, "# One\n\n1\n\n# Two\n\n2\n").await;
    ensure(&a, "document:marge").await;
    rename_document(&a, former, current).await;
    let one = address_of(&a, current, "# One").await;
    let two = address_of(&a, current, "# Two").await;
    let two_by_former = two.replacen(current, former, 1);
    let held = a
        .hold_placing(&EntityId::person(current))
        .await
        .expect("the lock is taken");

    let mut moving = std::pin::pin!(move_section(&b, &two_by_former, &one));
    let early = tokio::time::timeout(std::time::Duration::from_millis(200), &mut moving).await;
    assert!(
        early.is_err(),
        "a move under a former name went in while the document was held"
    );
    let other = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        put(&b, put_args("document:marge", "# Elsewhere\n\ne\n").await),
    )
    .await
    .expect("a different document does not wait");
    assert_ne!(json_of(&other)["status"], "blocked", "{other:?}");
    drop(held);
    // What the move answers is the move's own business: it went on once the
    // holder let go, which is all this case is about.
    tokio::time::timeout(std::time::Duration::from_secs(5), moving)
        .await
        .expect("the move goes on once the holder lets go");
}

/// **The same, the other way round**: the lock is held under a name the document
/// used to wear, and a put under the name it wears now waits for it.
#[tokio::test]
async fn a_put_under_the_current_name_waits_for_the_lock_held_under_a_former_one() {
    let (a, b, _memory) = two_connections();
    let (former, current) = ("document:homer", "document:krusty");
    put_ok(&a, former, "# One\n\n1\n").await;
    rename_document(&a, former, current).await;
    let one = address_of(&a, current, "# One").await;
    let held = a
        .hold_placing(&EntityId::person(former))
        .await
        .expect("the lock is taken");

    let args = PutDocumentArgs {
        after: Some(one),
        ..put_args(current, "# Late\n\nl\n").await
    };
    let mut putting = std::pin::pin!(put(&b, args));
    let early = tokio::time::timeout(std::time::Duration::from_millis(200), &mut putting).await;
    assert!(early.is_err(), "a put went in while the document was held");
    drop(held);
    let done = tokio::time::timeout(std::time::Duration::from_secs(5), putting)
        .await
        .expect("the put goes on once the holder lets go");
    assert_ne!(json_of(&done)["status"], "blocked", "{done:?}");
}

/// **A rename that lands after a caller has read the document's name and before
/// it holds the lock still leaves one lock.** The first caller names the document
/// by its old handle and is held between its read and its lock; the rename lands
/// there; a second caller then names it by the new handle and has to wait for the
/// first. A key made from the handle the first caller resolved to is the old
/// handle, so the second caller takes another lock.
#[tokio::test]
async fn a_rename_landing_while_the_lock_is_being_taken_leaves_one_lock() {
    let pause = Arc::new(ListingPause::default());
    let (a, b, _memory) = connections_over(Some(pause.clone()));
    let (former, current) = ("document:homer", "document:krusty");
    put_ok(&a, former, "# One\n\n1\n").await;
    pause.arm();

    let old_name = EntityId::person(former);
    let new_name = EntityId::person(current);
    let taking = a.hold_placing(&old_name);
    let renaming = async {
        tokio::time::timeout(std::time::Duration::from_secs(5), pause.reached())
            .await
            .expect("the first caller never listed the entities before taking its lock");
        rename_document(&b, former, current).await;
        pause.release();
    };
    let (held, ()) = tokio::join!(taking, renaming);
    let held = held.expect("the lock is taken");

    let mut second = std::pin::pin!(b.hold_placing(&new_name));
    let early = tokio::time::timeout(std::time::Duration::from_millis(200), &mut second).await;
    assert!(
        early.is_err(),
        "a second caller took a lock of its own after the rename"
    );
    drop(held);
    tokio::time::timeout(std::time::Duration::from_secs(5), second)
        .await
        .expect("the second caller goes on once the first lets go")
        .expect("the lock is taken");
}

/// **Two puts after the same section, one under the document's current handle and
/// one under a handle it used to wear, mint different keys.** The placing lock
/// belongs to the thing, so every name it has answered to shares it. Keyed on the
/// handle as sent, the two puts hold different locks, read the same neighbours and
/// mint the same key.
#[tokio::test]
async fn two_puts_under_two_names_of_one_document_mint_different_keys() {
    let (a, b, memory) = two_connections();
    let (former, current) = ("document:homer", "document:krusty");
    put_ok(&a, former, "# Anchor\n\nanchor\n").await;
    a.rename_entity(Parameters(crate::memory::rename_entity::RenameEntityArgs {
        handle: former.into(),
        to: current.into(),
        parent: None,
        recorded_at: None,
        override_token: None,
        override_reason: None,
        sid: Some(TEST_SID.into()),
    }))
    .await
    .expect("rename_entity answers");
    let anchor = address_of(&a, current, "# Anchor").await;

    let after_anchor = |document: &'static str, markdown: &'static str| {
        let anchor = anchor.clone();
        async move {
            PutDocumentArgs {
                after: Some(anchor),
                ..put_args(document, markdown).await
            }
        }
    };
    let left_args = after_anchor(current, "# Left\n\nleft\n").await;
    let right_args = after_anchor(former, "# Right\n\nright\n").await;
    let (left, right) = tokio::join!(put(&a, left_args), put(&b, right_args));
    let (left, right) = (json_of(&left), json_of(&right));
    assert_ne!(left["status"], "blocked", "{left}");
    assert_ne!(right["status"], "blocked", "{right}");

    let held = placed(&memory, current).await;
    let lines: Vec<&str> = held.iter().map(|(_, line)| line.as_str()).collect();
    assert_eq!(lines.len(), 3, "the anchor and both sections: {held:?}");
    assert_eq!(lines[0], "# Anchor", "{held:?}");
    assert!(
        lines.contains(&"# Left") && lines.contains(&"# Right"),
        "both sections exist: {held:?}"
    );
    let keys: std::collections::BTreeSet<&str> = held.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys.len(), 3, "every section has its own key: {held:?}");
}

/// Every whole number in `said`, in the order it is written.
fn numbers_in(said: &str) -> Vec<usize> {
    said.split(|c: char| !c.is_ascii_digit())
        .filter(|digits| !digits.is_empty())
        .filter_map(|digits| digits.parse().ok())
        .collect()
}

/// **A section whose one fenced block is bigger than a record is refused with the
/// section, the size against the room, and both ways through.** The block is
/// never cut, so nothing of the put is written. **Paired**: the same put without
/// the big block is stored, and a document that holds only a section that fits
/// reads back whole.
#[tokio::test]
async fn a_fenced_block_bigger_than_a_record_is_refused_naming_the_section_and_the_ways_through() {
    let jojobot = handler();
    ensure(&jojobot, "document:homer").await;
    let big = "word ".repeat(10_000);
    let refused = put(
        &jojobot,
        put_args(
            "document:homer",
            &format!("# Small\n\nfits\n\n## Config\n\n```\n{big}\n```\n"),
        )
        .await,
    )
    .await;

    let body = blocked(&refused);
    assert_eq!(body["fix_by"], "change");
    let said = body.to_string();
    assert!(said.contains("Config"), "the section is named: {said}");
    let figures = numbers_in(&said);
    assert!(
        figures.windows(2).any(|pair| pair == [2, 2]),
        "the section is placed as 2 of 2: {said}"
    );
    let size = figures
        .iter()
        .copied()
        .find(|n| *n >= big.len())
        .unwrap_or_else(|| panic!("the size of the block is given: {said}"));
    let room = figures
        .iter()
        .copied()
        .find(|n| *n >= 1_000 && *n < size)
        .unwrap_or_else(|| panic!("the room of a record is given: {said}"));
    assert!(
        room <= ANSWER_CEILING,
        "the room is what one answer can carry: {room}"
    );
    assert!(said.contains("split"), "the first way through: {said}");
    assert!(said.contains("outside"), "the second way through: {said}");
    assert!(
        !said.contains("name, aliases and keys"),
        "the document's own keys are not the cause: {said}"
    );
    assert_eq!(
        records(&jojobot, "document:homer").await.len(),
        0,
        "a refused put writes no section"
    );

    // The positive beside it: the section that fits is stored.
    let stored = put_ok(&jojobot, "document:marge", "# Small\n\nfits\n").await;
    assert_eq!(stored["sections"], 1, "{stored}");
    assert_eq!(
        read_all(&jojobot, "document:marge").await,
        "# Small\n\nfits\n"
    );
}

/// **A put names each section it wrote by its address**, so a caller knows
/// which sections moved. A second put names only its own sections, and the
/// ones already there are not listed.
#[tokio::test]
async fn a_put_lists_one_created_entry_per_section_it_wrote() {
    let jojobot = handler();
    let first = put_ok(
        &jojobot,
        "document:krusty",
        "# One\n\nThe first part.\n\n# Two\n\nThe second part.\n",
    )
    .await;
    let changed = first["changed"]
        .as_array()
        .unwrap_or_else(|| panic!("a list of changes: {first}"));
    assert_eq!(changed.len(), 2, "{first}");
    assert!(changed.iter().all(|c| c["what"] == "created"), "{first}");
    assert_eq!(changed[0]["on"], first["first"], "{first}");
    assert_eq!(changed[1]["on"], first["last"], "{first}");

    let more = json_of(
        &put(
            &jojobot,
            PutDocumentArgs {
                after: first["last"].as_str().map(str::to_string),
                ..put_args("document:krusty", "# Three\n\nThe third part.\n").await
            },
        )
        .await,
    );
    let changed = more["changed"]
        .as_array()
        .unwrap_or_else(|| panic!("a list of changes: {more}"));
    assert_eq!(changed.len(), 1, "{more}");
    assert_eq!(changed[0]["what"], "created", "{more}");
    assert_eq!(changed[0]["on"], more["first"], "{more}");
    assert_ne!(changed[0]["on"], first["last"], "{more}");
}

/// **A record that names a version in `derived_from` and carries no mark is never
/// taken for that version's restored copy**, however it reads. The plan is worked
/// out from the records alone. The closing stands under another record that names
/// it in `derived_from`, once with the closing's own words and once with others:
/// both are sections made since, so the plan archives each and writes the closing
/// again.
///
/// **Paired**: the same record carrying the mark IS the copy, and the plan writes
/// nothing.
#[tokio::test]
async fn an_unmarked_record_derived_from_a_version_is_never_taken_for_its_restored_copy() {
    let jojobot = handler();
    put_ok(
        &jojobot,
        "document:homer",
        "# Manual\n\n## Closing\n\nDrain the oil.\n",
    )
    .await;
    let facts = jojobot
        .memory
        .recall(&EntityId("document:homer".into()))
        .await
        .expect("recall ok");
    let then = live_sections(&facts);
    let closing = then
        .iter()
        .find(|held| held.fact.content == "## Closing")
        .expect("the closing")
        .fact;

    let plan_with = |copy_of: Fact| {
        let others: Vec<Fact> = facts
            .iter()
            .filter(|fact| fact.id != closing.id)
            .cloned()
            .chain(std::iter::once(copy_of))
            .collect();
        let now = live_sections(&others);
        plan_restore(
            &EntityId("document:homer".into()),
            &then,
            &now,
            "s",
            jiff::civil::date(2026, 10, 8),
        )
    };
    let mut record = closing.clone();
    record.id = FactId("f99".into());
    record.derived_from = Some(closing.address());
    assert!(
        !record.fields.contains_key(document::RESTORED_FROM),
        "a record from before the mark has none"
    );
    for details in [
        closing.details.clone(),
        Some("Something else entirely.".into()),
    ] {
        record.details = details;
        let plan = plan_with(record.clone());
        assert!(
            plan.steps.iter().any(|step| matches!(
                step,
                Step::Archive { address } if *address == record.address()
            )),
            "unmarked, it is archived as a section made since: {:?}",
            record.details
        );
        assert!(
            plan.steps
                .iter()
                .any(|step| matches!(step, Step::Recreate { .. })),
            "…and the closing is written again: {:?}",
            record.details
        );
    }

    record.details = closing.details.clone();
    record.fields.insert(
        document::RESTORED_FROM.to_string(),
        closing.address().to_string(),
    );
    assert!(
        plan_with(record).steps.is_empty(),
        "marked, it is the copy and nothing is written"
    );
}
