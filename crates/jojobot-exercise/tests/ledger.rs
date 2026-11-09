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

/// **A test that asserts through a helper of its own file carries that helper's
/// assertions**, each under the call that reaches it, so taking the helper's
/// `assert_eq!` out changes every row that calls it.
#[test]
fn ledger_case_row_carries_the_assertions_of_the_helpers_it_calls() {
    let file = "async fn check(n: usize, m: usize) {\n    assert_eq!(n, m, \"same\");\n}\n\n#[tokio::test]\nasync fn a_case() {\n    check(1, 1).await;\n}\n";
    let body = "async fn a_case() {\n    check(1, 1).await;\n}\n";
    let held = ledger::assertions_through_helpers(body, file);
    assert_eq!(held.len(), 1, "{held:?}");
    assert!(held[0].contains("check(1, 1)"), "{held:?}");
    assert!(held[0].contains("assert_eq!(n, m"), "{held:?}");
    let without = file.replace("    assert_eq!(n, m, \"same\");\n", "");
    assert!(ledger::assertions_through_helpers(body, &without).is_empty());

    let made = ledger::render();
    let row = made
        .lines()
        .find(|row| {
            row.starts_with("case\t")
                && row.contains("::january_launch_day_in_a_sentence_reds_only_the_launch_lock\t")
        })
        .expect("the agency room case has a row");
    let oracle = row.split('\t').nth(7).unwrap();
    assert!(oracle.contains("discriminates(0, 0)"), "{oracle}");
    assert!(oracle.contains("assert_eq!(held, all_but"), "{oracle}");
}

/// A kill means something only against a lane that passes unmutated. The sample
/// runs the lane once untouched and stops, mutating nothing and recording nothing,
/// when that run is red.
///
/// **The case runs the tool on a manifest and a file of its own**, in a directory
/// of its own: the real manifest's sites are the ones a mutant changes, and a
/// case that read them would fail under every mutant and record a kill no
/// behaviour made.
#[test]
fn the_mutation_sample_stops_when_its_lane_is_red_before_any_mutation() {
    let root = jojobot_exercise::expectations::room_document("../..");
    let dir = std::env::temp_dir().join(format!("mutant-sample-baseline-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let site = dir.join("site.txt");
    let tokens: Vec<String> = (0..30).map(|n| format!("SITE{n:02}")).collect();
    std::fs::write(&site, tokens.join("\n")).unwrap();
    let mut manifest = String::from("file\tfrom\tto\n");
    for token in &tokens {
        manifest.push_str(&format!("{}\t{token}\t{token}X\n", site.display()));
    }
    let manifest_path = dir.join("mutants.tsv");
    std::fs::write(&manifest_path, manifest).unwrap();
    let run = std::process::Command::new("python3")
        .current_dir(&dir)
        .env("MUTANT_MANIFEST", &manifest_path)
        .arg(root.join("scripts/mutant-sample"))
        .args(["--from", "0", "--count", "1", "--", "false"])
        .output()
        .unwrap();
    let said =
        String::from_utf8_lossy(&run.stdout).to_string() + &String::from_utf8_lossy(&run.stderr);
    let untouched = std::fs::read_to_string(&site).unwrap() == tokens.join("\n");
    let recorded = dir.join("target").exists();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        run.status.code(),
        Some(3),
        "a red lane before any mutation exits 3: {said}"
    );
    assert!(
        said.contains("baseline"),
        "the refusal names the baseline: {said}"
    );
    assert!(untouched, "no site was mutated: {said}");
    assert!(!recorded, "nothing is recorded: {said}");
}
