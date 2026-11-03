//! **Who first wrote a claim, as a table.** The rule has three outcomes and
//! one order: the bot the write recorded wins, the bot behind the run is read
//! only when none was recorded, and a write with neither names nobody. The run
//! is looked up lazily: a recorded bot never asks the run.

use super::*;
use std::cell::Cell;

fn bot(handle: &str) -> EntityId {
    EntityId(handle.into())
}

#[test]
fn the_recorded_bot_wins_then_the_bot_behind_the_run_then_nobody() {
    //   (recorded, behind the run, writer)
    let table = [
        // Recorded and a run behind it: the recorded bot, never the run's.
        (Some("bot:gamma"), Some("bot:delta"), Some("bot:gamma")),
        // Recorded and no run: the "no run" branch is not reached.
        (Some("bot:gamma"), None, Some("bot:gamma")),
        // Nothing recorded and a live run: read through the run.
        (None, Some("bot:delta"), Some("bot:delta")),
        // Nothing recorded and no run: nobody.
        (None, None, None),
    ];
    for (recorded, behind, writer) in table {
        let outcome = first_writer(recorded.map(bot), || behind.map(bot));
        assert_eq!(
            outcome,
            writer.map(bot),
            "recorded {recorded:?}, run {behind:?}"
        );
    }
}

#[test]
fn the_run_is_asked_only_when_no_bot_was_recorded() {
    let asked = Cell::new(0);
    let behind = || {
        asked.set(asked.get() + 1);
        Some(bot("bot:delta"))
    };
    let _ = first_writer(Some(bot("bot:gamma")), behind);
    assert_eq!(asked.get(), 0, "a recorded bot never asks its run");
    let _ = first_writer(None, behind);
    assert_eq!(asked.get(), 1, "an unrecorded write asks its run once");
}

/// The writer a rename never changes: the id the bot is filed under, else the
/// bot as it was named.
#[test]
fn a_stored_writer_is_the_id_the_bot_is_filed_under_else_the_bot_as_named() {
    let filed = bot("bot:epsilon");
    assert_eq!(
        stored_writer(&bot("bot:gamma"), |_| Some(filed.clone())),
        filed
    );
    assert_eq!(stored_writer(&bot("bot:gamma"), |_| None), bot("bot:gamma"));
}
