//! **One damaged run row is named, and the other runs still read** (rule 130).
//! The summaries of a bot's runs feed `list_runs`, the thought captures and the
//! edit of a bot's claim; before, one stamp that was no moment failed every one.

use super::*;
use crate::dolt::tests::free_port;
use crate::dolt::{Dolt, migrate};
use crate::testing::Scratch;

/// Put a run of `bot:gamma` on the board with the stamps and state given.
async fn row(store: &Dolt, id: &str, started_at: &str, state: &str) {
    sqlx::query(
        "INSERT INTO session (id, sid, bot, focus, started_at, state)
         VALUES (?, NULL, 'bot:gamma', 'what it was doing', ?, ?)",
    )
    .bind(id)
    .bind(started_at)
    .bind(state)
    .execute(store.pool())
    .await
    .expect("the board takes the row");
}

/// One beat on a run, with the moment given as stored text.
async fn beat(store: &Dolt, run: &str, at: &str) {
    sqlx::query(
        "INSERT INTO journal_entry (session, id, ordinal, at, text)
         VALUES (?, 'e1', 1, ?, 'a beat')",
    )
    .bind(run)
    .bind(at)
    .execute(store.pool())
    .await
    .expect("the board takes the beat");
}

/// **A run whose row cannot be read comes back named and flagged, beside the runs
/// that can be**, from every read of summaries, in moment order for the ones that
/// read. The causes: a start that is no moment, a state that is no state, and a
/// beat whose time is no moment. The pair: a whole-run read of the damaged row
/// still refuses, because it has no record to give; and the sound runs carry no
/// flag.
#[tokio::test]
async fn one_unreadable_run_is_named_and_the_runs_beside_it_still_read() {
    let scratch = Scratch::new("unreadable-run-summaries");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    migrate::run(store.pool()).await.expect("the schema");
    migrate::seed_kinds(store.pool())
        .await
        .expect("the kinds are seeded");
    let sessions = DoltSessions::open(store.pool().clone());

    row(&store, "sound-new", "2026-01-02T00:00:00Z", "active").await;
    row(&store, "sound-old", "2026-01-01T00:00:00Z", "active").await;
    row(&store, "bad-stamp", "the day before yesterday", "active").await;
    row(&store, "bad-state", "2026-01-03T00:00:00Z", "in-flight").await;
    row(&store, "bad-beat", "2026-01-04T00:00:00Z", "active").await;
    beat(&store, "bad-beat", "last tuesday").await;
    beat(&store, "sound-old", "2026-01-01T00:00:05Z").await;

    let bot = EntityId("bot:gamma".into());
    let runs = sessions
        .summaries_of(&bot)
        .await
        .expect("one damaged row does not fail the read");
    let flagged: Vec<(&str, bool)> = runs
        .iter()
        .map(|run| (run.id.0.as_str(), run.unreadable.is_some()))
        .collect();
    assert_eq!(
        flagged.iter().filter(|(_, bad)| !bad).collect::<Vec<_>>(),
        [&("sound-new", false), &("sound-old", false)],
        "the sound runs read, newest first: {flagged:?}"
    );
    let mut named: Vec<&str> = flagged
        .iter()
        .filter(|(_, bad)| *bad)
        .map(|(id, _)| *id)
        .collect();
    named.sort_unstable();
    assert_eq!(named, ["bad-beat", "bad-stamp", "bad-state"], "{flagged:?}");
    // **A run that cannot be read is never live**, whatever its row says: the
    // damaged runs here were stored `active` and come back abandoned.
    for run in runs.iter().filter(|run| run.unreadable.is_some()) {
        assert_eq!(run.state, SessionState::Abandoned, "{run:?}");
    }
    let sound_old = runs
        .iter()
        .find(|run| run.id.0 == "sound-old")
        .expect("read");
    assert_eq!(sound_old.entry_count, 1, "a sound run keeps its beats");

    // The other two summary reads answer the same way.
    let every = sessions
        .all_summaries()
        .await
        .expect("one damaged row does not fail it");
    assert_eq!(every.len(), 5);
    let one = sessions
        .summary_of(&SessionId("bad-stamp".into()))
        .await
        .expect("a damaged run is named, not an error")
        .expect("the run exists");
    assert!(one.unreadable.is_some(), "{one:?}");

    // The pair: a read of the whole damaged record has nothing to give.
    assert!(matches!(
        sessions.read_session(&SessionId("bad-stamp".into())).await,
        Err(SessionError::Store(_))
    ));
    store.stop().await;
}
