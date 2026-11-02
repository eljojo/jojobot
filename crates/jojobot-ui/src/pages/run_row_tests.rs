//! The runs table's row on a bot's page: a run whose record could not be read is
//! named, and a run that read prints its start.

use super::*;
use jojobot_domain::session::{SessionId, SessionState, SessionSummary};

fn run(unreadable: Option<&str>) -> SessionSummary {
    SessionSummary {
        id: SessionId("run-one".into()),
        sid: None,
        bot: EntityId("bot:gamma".into()),
        focus: "shelving".into(),
        started_at: jiff::Timestamp::UNIX_EPOCH,
        state: SessionState::Abandoned,
        entry_count: 0,
        last_beat: jiff::Timestamp::UNIX_EPOCH,
        served_chars: 0,
        started_on: None,
        compactions: 0,
        served_set_back: 0,
        unreadable: unreadable.map(str::to_string),
    }
}

/// **A run whose record could not be read is named on the page, with why, and
/// prints no moment.** The pair is a run that read, which prints its start.
#[test]
fn an_unreadable_run_is_named_on_the_runs_table_and_a_sound_one_prints_its_start() {
    let named = run_row(&run(Some("its start time is no time")));
    assert!(
        named.contains("unreadable: its start time is no time"),
        "{named}"
    );
    assert!(!named.contains("1970"), "{named}");
    let sound = run_row(&run(None));
    assert!(
        sound.contains("1970"),
        "a sound run prints its start: {sound}"
    );
    assert!(!sound.contains("unreadable"), "{sound}");
}
