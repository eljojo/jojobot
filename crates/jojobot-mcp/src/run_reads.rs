//! **A write reads its run once**, and a run that cannot be read still refuses
//! the write (rule 130).
//!
//! The write path identifies its caller and judges the run's wrap window. It reads
//! the run once for both, and a batch reads it once for every item. The
//! counting store counts the reads of one run as a summary, which is the read the
//! check makes of a live run. The tally of the beat that follows a write reads the
//! run whole, and is not the check.

use std::sync::Arc;

use crate::harness::*;
use crate::memory::capture::EntryItem;
use crate::memory::testing::*;
use crate::session::testing::CountingSessions;
use jojobot_domain::session::testing::InMemorySessions;

/// A handler over a counting store, a bot with a run that has written once (so
/// the run has its card), and a list for the writes.
async fn a_run_with_a_card() -> (crate::Jojobot, Arc<CountingSessions>, String) {
    let counting = Arc::new(CountingSessions::new(InMemorySessions::new()));
    let (jojobot, _memory) =
        crate::session::testing::with_sessions_port_and_memory(counting.clone());
    let bot = "bot:lisa";
    ensure(&jojobot, bot).await;
    ensure(&jojobot, "list:homer-chores").await;
    let sid = booted(&jojobot, bot).await;
    capture_ok(
        &jojobot,
        crate::memory::CaptureArgs {
            sid: Some(sid.clone()),
            provenance: Some("testimony".into()),
            ..capture_args("list:homer-chores", "warm the run up")
        },
    )
    .await;
    (jojobot, counting, sid)
}

fn entries(count: usize, sid: &str) -> crate::memory::CaptureArgs {
    crate::memory::CaptureArgs {
        sid: Some(sid.to_string()),
        provenance: Some("testimony".into()),
        entries: Some(
            (0..count)
                .map(|at| EntryItem::Text(format!("chore number {at}")))
                .collect(),
        ),
        ..capture_args("list:homer-chores", "")
    }
}

/// **Identifying a caller to write reads the run once**, and only as its
/// summary: a live run is judged by its state, so no chronology is read.
#[tokio::test]
async fn identifying_a_caller_to_write_reads_the_run_once() {
    let (jojobot, counting, sid) = a_run_with_a_card().await;
    let (summaries, whole) = (counting.summary_reads(), counting.run_reads());
    assert!(jojobot.identified_for_write(Some(&sid)).await.is_ok());
    assert_eq!(counting.summary_reads() - summaries, 1);
    assert_eq!(counting.run_reads() - whole, 0);
}

/// **A batch identifies its caller once, however many items it carries.** The
/// reads of the run's summary for a batch of fifty are the reads for a batch of
/// two, and a single capture's are the same again: the check is not repeated for
/// each item, once to check it and once to write it.
#[tokio::test]
async fn a_batch_reads_its_run_for_the_check_once_whatever_its_size() {
    let (jojobot, counting, sid) = a_run_with_a_card().await;
    let summaries_for =
        |counting: &CountingSessions, before: usize| counting.summary_reads() - before;
    let before = counting.summary_reads();
    capture_ok(
        &jojobot,
        crate::memory::CaptureArgs {
            sid: Some(sid.clone()),
            provenance: Some("testimony".into()),
            ..capture_args("list:homer-chores", "mow the lawn")
        },
    )
    .await;
    let single = summaries_for(&counting, before);
    let before = counting.summary_reads();
    let landed = capture_ok(&jojobot, entries(2, &sid)).await;
    assert_eq!(landed["count"], 2, "{landed}");
    let two = summaries_for(&counting, before);
    let before = counting.summary_reads();
    let landed = capture_ok(&jojobot, entries(50, &sid)).await;
    assert_eq!(landed["count"], 50, "{landed}");
    let fifty = summaries_for(&counting, before);
    assert_eq!(
        two, single,
        "a batch of two checks as a single capture does"
    );
    assert_eq!(
        fifty, single,
        "a batch of fifty checks as a single capture does"
    );
}

/// **A run that cannot be read refuses the write, one capture or a batch**, and
/// the same writes land when it reads again. The paired negative of the cases
/// above: reading less never means letting a write through unread.
#[tokio::test]
async fn a_run_that_cannot_be_read_still_refuses_the_write() {
    let (jojobot, counting, sid) = a_run_with_a_card().await;
    counting.make_runs_unreadable(true);
    for args in [
        entries(2, &sid),
        crate::memory::CaptureArgs {
            sid: Some(sid.clone()),
            provenance: Some("testimony".into()),
            ..capture_args("list:homer-chores", "mow the lawn")
        },
    ] {
        let refused = json_of(
            &jojobot
                .capture(rmcp::handler::server::wrapper::Parameters(args))
                .await
                .expect("an answer"),
        );
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert_eq!(refused["fix_by"], "retry", "{refused}");
        assert_eq!(refused["wrote"], false, "{refused}");
    }
    counting.make_runs_unreadable(false);
    let landed = capture_ok(&jojobot, entries(2, &sid)).await;
    assert_eq!(landed["count"], 2, "{landed}");
}
