//! **The ledger the repository keeps is the ledger the shipped rooms make.**
//!
//! `ledger/room-assertions.tsv` lists every lock of every shipped room and every
//! integration case of this crate (rule 767). A lock or a case added, removed or
//! reworded without the file moving fails here; the file is rewritten with
//! `LEDGER_UPDATE=1`.
//!
//! The mutation sample's manifest is checked by `scripts/mutant-sample --check`
//! and not here: a test that reads the sites fails under every mutant, and a lane
//! that runs it would record a kill no behavioural oracle made.

use jojobot_exercise::ledger;

fn file() -> std::path::PathBuf {
    jojobot_exercise::expectations::room_document("ledger/room-assertions.tsv")
}

#[test]
fn the_kept_ledger_is_the_one_the_shipped_rooms_make() {
    let made = ledger::render();
    if std::env::var_os("LEDGER_UPDATE").is_some() {
        std::fs::create_dir_all(file().parent().unwrap()).unwrap();
        std::fs::write(file(), &made).unwrap();
    }
    let kept = std::fs::read_to_string(file()).unwrap_or_else(|e| {
        panic!(
            "the ledger at {} must exist (LEDGER_UPDATE=1 writes it): {e}",
            file().display()
        )
    });
    assert!(
        kept == made,
        "the kept ledger differs from the shipped rooms; LEDGER_UPDATE=1 rewrites it"
    );
}

/// Every row has every column, every lock of the rooms has a row, and every
/// integration test has a row, counted a second way.
#[test]
fn every_row_has_all_its_columns_and_every_lock_and_case_has_a_row() {
    let made = ledger::render();
    let columns = ledger::HEADER.split('\t').count();
    for row in made.lines() {
        assert_eq!(row.split('\t').count(), columns, "{row}");
    }
    let rows = |kind: &str| {
        made.lines()
            .skip(1)
            .filter(|row| row.split('\t').next() == Some(kind))
            .count()
    };
    let locks = jojobot_exercise::failability::shipped_locks().len();
    assert!(locks > 0);
    assert_eq!(rows("lock"), locks);
    assert_eq!(rows("case"), ledger::counted_cases());
}

/// **A live lock reads the room as the run left it, so its history is the final
/// room; the other two windows read a stored boundary, so theirs is the phase.**
/// The authored phase stays in its own column.
#[test]
fn ledger_lock_history_follows_its_window() {
    let made = ledger::render();
    let mut live = 0;
    let mut bounded = 0;
    for row in made.lines().skip(1) {
        let cells: Vec<&str> = row.split('\t').collect();
        if cells[0] != "lock" {
            continue;
        }
        let (phase, window, history) = (cells[3], cells[4], cells[10]);
        match window {
            "live" => {
                live += 1;
                assert!(history.contains("end of the room"), "{row}");
                assert!(!history.contains("through"), "{row}");
            }
            _ if !phase.is_empty() => {
                bounded += 1;
                assert!(history.contains(&format!("through {phase}")), "{row}");
            }
            _ => {}
        }
    }
    assert!(live > 0 && bounded > 0, "both windows are exercised");
}

/// **A case row carries the assertions of its body**, so taking one out of a test
/// whose name and comment stay the same changes the ledger.
#[test]
fn ledger_case_row_carries_its_assertions() {
    let body = "fn a_case() {\n    let n = 1;\n    assert!(\n        n > 0,\n        \"it holds\"\n    );\n    assert_eq!(n, 1);\n}\n";
    let held = ledger::assertions_of(body);
    assert_eq!(held.len(), 2, "{held:?}");
    assert!(held[0].contains("n > 0"), "{held:?}");
    assert!(held[1].contains("assert_eq!(n, 1)"), "{held:?}");
    let without = body.replace("    assert_eq!(n, 1);\n", "");
    assert_ne!(held, ledger::assertions_of(&without));

    let made = ledger::render();
    let blank = made
        .lines()
        .skip(1)
        .map(|row| row.split('\t').collect::<Vec<_>>())
        .filter(|cells| cells[0] == "case" && cells[7].is_empty())
        .count();
    assert_eq!(blank, 0, "a case holds an assertion or says it holds none");
}
