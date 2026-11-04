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

const FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/ledger/room-assertions.tsv");

#[test]
fn the_kept_ledger_is_the_one_the_shipped_rooms_make() {
    let made = ledger::render();
    if std::env::var_os("LEDGER_UPDATE").is_some() {
        std::fs::create_dir_all(std::path::Path::new(FILE).parent().unwrap()).unwrap();
        std::fs::write(FILE, &made).unwrap();
    }
    let kept = std::fs::read_to_string(FILE).unwrap_or_else(|e| {
        panic!("the ledger at {FILE} must exist (LEDGER_UPDATE=1 writes it): {e}")
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
