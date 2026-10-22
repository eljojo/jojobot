//! The reader reloads when jojobot commits to the index, and at no other time
//! (work: q-search-reader-manual-reload).
//!
//! The reader used to reload by itself a moment after every commit, and it took
//! the index's meta lock to do it. jojobot reloads by hand after each commit
//! too, so the two met on the lock and the one that lost slept 100 ms before it
//! tried again. Every write path that commits to the index now reloads the
//! reader itself, through one function, and the reader does nothing on its own.

use std::time::Duration;

use jojobot_domain::mailbox::MessageState;

use super::tests::{asking_for_mail, entity, index_of, message, run, scan};
use super::*;

/// **The reader does not reload on its own.** A document committed straight to
/// the writer is not in the reader until the reader is told to reload. Under the
/// default policy the reader reloads within a few milliseconds of the commit
/// without being told, and that reload is the one that takes the lock.
#[test]
fn the_reader_reloads_only_when_it_is_told_to() {
    let index = index_of(vec![]);
    let before = index.reader.searcher().num_docs();
    {
        let mut writer = index.writer.write().expect("index writer poisoned");
        let staged = index.write_message(
            &writer,
            &message("1", "pm", "dev", None, "a body", MessageState::New),
        );
        staged.expect("a document stages");
        writer.commit().expect("the commit lands");
    }
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        index.reader.searcher().num_docs(),
        before,
        "the reader reloaded by itself"
    );
    index.reader.reload().expect("a reload by hand");
    assert_eq!(index.reader.searcher().num_docs(), before + 1);
}

/// How many hits a plain text search finds, asked as `bot:milhouse` so a run of
/// that bot is in reach.
fn found(index: &FullTextIndex, text: &str) -> usize {
    index
        .search(&SearchQuery {
            include_mail: true,
            asked_by: Some(EntityId("bot:milhouse".into())),
            ..SearchQuery::text(text)
        })
        .expect("search ok")
        .len()
}

/// **A message written one at a time is found straight after.**
#[test]
fn a_message_ingested_alone_is_found_straight_after() {
    let index = index_of(vec![]);
    assert_eq!(found(&index, "kilnword"), 0, "nothing yet");
    index
        .ingest_message(&message(
            "1",
            "pm",
            "dev",
            None,
            "the kilnword is lit",
            MessageState::New,
        ))
        .expect("the message lands");
    assert_eq!(found(&index, "kilnword"), 1);
    let hits = index
        .search(&asking_for_mail("kilnword"))
        .expect("search ok");
    assert!(
        hits.iter()
            .any(|hit| matches!(hit, Hit::Message { message, .. } if message.id.as_str() == "1"))
    );
}

/// **A board read that changed mail is found straight after.**
#[test]
fn mail_read_from_the_board_is_found_straight_after() {
    let index = index_of(vec![]);
    index
        .ingest_mail(&[message(
            "1",
            "pm",
            "dev",
            None,
            "the glazeword is ready",
            MessageState::New,
        )])
        .expect("the board read lands");
    assert_eq!(found(&index, "glazeword"), 1);
}

/// **A document written one at a time is found straight after.**
#[test]
fn a_document_ingested_alone_is_found_straight_after() {
    let index = index_of(vec![]);
    let kiln = entity("thing:kettle", "Kiln");
    index
        .ingest_doc(
            &scan("doc-kiln", Some(kiln), "the potword page", vec![]),
            &Default::default(),
        )
        .expect("the document lands");
    assert_eq!(found(&index, "potword"), 1);
}

/// **A document that is forgotten is gone straight after.** The negative it
/// rests on: it was found before.
#[test]
fn a_document_that_is_forgotten_is_gone_straight_after() {
    let kiln = entity("thing:kettle", "Kiln");
    let index = index_of(vec![scan(
        "doc-kiln",
        Some(kiln.clone()),
        "the potword page",
        vec![],
    )]);
    assert_eq!(found(&index, "potword"), 1, "found before it is forgotten");
    index.forget(&kiln.id).expect("the document goes");
    assert_eq!(found(&index, "potword"), 0);
}

/// **A scan that changed documents is found straight after.**
#[test]
fn a_scan_that_changed_a_document_is_found_straight_after() {
    let index = index_of(vec![]);
    let kiln = entity("thing:kettle", "Kiln");
    index
        .ingest_all(
            &[scan("doc-kiln", Some(kiln), "the scanword page", vec![])],
            index.reading_begins(),
            &Default::default(),
        )
        .expect("the scan lands");
    assert_eq!(found(&index, "scanword"), 1);
}

/// **A sessions read that changed a run is found straight after.**
#[test]
fn a_run_read_from_the_runs_is_found_straight_after() {
    let index = FullTextIndex::open().expect("index opens");
    index
        .ingest_sessions(&[run(
            "r1",
            "bot:milhouse",
            "the focus",
            "a beat with runword",
        )])
        .expect("the runs land");
    assert_eq!(found(&index, "runword"), 1);
}
