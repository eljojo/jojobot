//! **The change signal search reads for sessions, held to the store's own.**
//!
//! `SessionChanges` answers `write_summary` with a count of its own calls where
//! the store counts a row it appends to `session_write`. The two must move on
//! the same calls: a count that moved on more would send the index back to read
//! every run after each answer, and one that moved on fewer would leave a hit
//! stale. Each call below is made through both, and the store's count and the
//! signal's must agree on whether it moved.

use std::path::PathBuf;
use std::sync::Arc;

use jiff::civil::date;
use jojobot_adapters::changes::SessionChanges;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::sessions::DoltSessions;
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::memory::EntityId;
use jojobot_domain::session::testing::contract as sessions;
use jojobot_domain::session::{NewEntry, NewSession, SessionState, Sessions, Sid, WrapWindow};

struct Scratch(PathBuf);

impl Scratch {
    fn new(what: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "jojobot-changes-{}-{what}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("a clock after 1970")
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn the_sessions_signal_moves_on_the_calls_the_stores_own_count_moves_on() {
    let scratch = Scratch::new("sessions");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "signal")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");

    let store = Arc::new(DoltSessions::open(pool));
    let counted = SessionChanges::new(store.clone());
    let at = sessions::epoch();

    // Each step: the call, made through the signal's port, and whether the
    // store's own count is expected to move. The expectation is read off the
    // store below, not written here: the assertion compares the two counts.
    let begun = counted
        .begin(NewSession {
            bot: EntityId("bot:sigma".into()),
            sid: Sid("cnf1".into()),
            focus: "working".into(),
            started_at: at,
            timezone: None,
            started_on: None,
            id: None,
            key_hash: None,
        })
        .await
        .expect("begin ok");
    let id = begun.id.clone();

    let mut moved = Vec::new();
    macro_rules! step {
        ($label:expr, $call:expr) => {{
            let store_before = store
                .write_summary()
                .await
                .expect("store count")
                .expect("a count")
                .0;
            let signal_before = counted
                .write_summary()
                .await
                .expect("signal")
                .expect("a count")
                .0;
            let _ = $call.await.expect($label);
            let store_after = store
                .write_summary()
                .await
                .expect("store count")
                .expect("a count")
                .0;
            let signal_after = counted
                .write_summary()
                .await
                .expect("signal")
                .expect("a count")
                .0;
            assert_eq!(
                store_after != store_before,
                signal_after != signal_before,
                "{}: the store's count moved: {}, the signal's moved: {}",
                $label,
                store_after != store_before,
                signal_after != signal_before,
            );
            moved.push(($label, signal_after != signal_before));
        }};
    }

    step!(
        "append",
        counted.append(&id, NewEntry::manual("first beat", at, None))
    );
    step!("amend_last", counted.amend_last(&id, "first beat, amended"));
    step!("set_focus", counted.set_focus(&id, "still working"));
    step!(
        "set_timezone",
        counted.set_timezone(&id, Some("America/New_York"))
    );
    step!(
        "set_stated_day",
        counted.set_stated_day(&id, Some(date(2026, 9, 1)))
    );
    step!("add_served", async {
        counted
            .add_served(&id, 12, jiff::civil::date(2026, 9, 2))
            .await
            .map(|_| ())
    });
    step!("count_call", {
        let key = jojobot_domain::usage::CallKey {
            bot: Some(jojobot_domain::memory::EntityId("bot:gamma".into())),
            day: date(2026, 9, 1),
            verb: "recall".into(),
            outcome: jojobot_domain::usage::CallOutcome::Landed,
            run: None,
        };
        let counted = &counted;
        async move { counted.count_call(&key).await }
    });
    step!("set_key_hash", async {
        counted
            .set_key_hash(&id, "hash", &jojobot_domain::session::KeyTrail::default())
            .await
            .map(|_| ())
    });
    step!("close", counted.close(&id, SessionState::Wrapped));
    step!(
        "set_wrap_window",
        counted.set_wrap_window(&id, Some(WrapWindow::Offered("code".into())))
    );

    // **The positive half**: the calls that write must be among the ones that
    // moved, or the comparison above held on a signal that never moves.
    let moving: Vec<&str> = moved.iter().filter(|(_, m)| *m).map(|(l, _)| *l).collect();
    assert_eq!(
        moving,
        vec!["append", "amend_last", "set_focus"],
        "the signal moves on the calls that change what the index holds: {moved:?}"
    );
    server.stop().await;
}
