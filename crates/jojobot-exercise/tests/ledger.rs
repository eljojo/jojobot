//! **The ledger the repository keeps is the ledger the shipped rooms make.**
//!
//! `ledger/room-assertions.tsv` lists every lock of every shipped room (rule 767).
//! A lock added, removed or reworded without the file moving fails here; the
//! file is rewritten with `LEDGER_UPDATE=1`.

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

const MUTANTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/ledger/mutants.tsv");

/// **The mutation sample names sites `scripts/sabotage` can break**: each file
/// exists, each `from` text appears in it exactly once (the tool refuses any
/// other count), each `to` differs, and the sample holds about thirty.
#[test]
fn every_mutant_of_the_sample_names_one_site_that_exists() {
    let kept = std::fs::read_to_string(MUTANTS).expect("the mutation sample must exist");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut rows = 0;
    for row in kept.lines().skip(1) {
        let cells: Vec<&str> = row.split('\t').collect();
        assert_eq!(cells.len(), 3, "{row}");
        let (file, from, to) = (cells[0], cells[1], cells[2]);
        assert_ne!(from, to, "{row}");
        let text = std::fs::read_to_string(root.join(file))
            .unwrap_or_else(|e| panic!("{file} must read: {e}"));
        assert_eq!(text.matches(from).count(), 1, "{file}: {from}");
        rows += 1;
    }
    assert!(
        rows >= 30,
        "the sample holds about thirty mutants, not {rows}"
    );
}

/// Every row has every column, and every lock of the rooms has a row.
#[test]
fn every_row_has_all_its_columns_and_every_lock_has_a_row() {
    let made = ledger::render();
    let columns = ledger::HEADER.split('\t').count();
    for row in made.lines() {
        assert_eq!(row.split('\t').count(), columns, "{row}");
    }
    let locks = jojobot_exercise::failability::shipped_locks().len();
    assert_eq!(made.lines().count() - 1, locks);
    assert!(locks > 0);
}
