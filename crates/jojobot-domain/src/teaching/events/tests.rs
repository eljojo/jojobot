use jiff::Timestamp;

use super::*;

fn run(name: &str) -> RunId {
    RunId(name.to_string())
}

fn at(second: i64) -> Timestamp {
    Timestamp::from_second(1_700_000_000 + second).expect("a moment")
}

/// Apply a prepared batch to the state and the log, as a writer would.
fn commit<T>(state: &mut State, log: &mut Vec<TeachingEvent>, prepared: Prepared<T>) -> T {
    for event in &prepared.events {
        state.apply(event);
    }
    log.extend(prepared.events);
    prepared.answer
}

/// **A duplicate contact appends nothing.** The first contact answers true with
/// one event; the second answers false with none.
#[test]
fn a_duplicate_contact_appends_nothing() {
    let mut state = State::default();
    let mut log = Vec::new();
    let first = state.first_contact(&run("homer"), "claims", at(0)).unwrap();
    assert_eq!(first.events.len(), 1);
    assert!(commit(&mut state, &mut log, first));
    let again = state.first_contact(&run("homer"), "claims", at(1)).unwrap();
    assert!(again.events.is_empty(), "{again:?}");
    assert!(!again.answer);
    assert_eq!(log.len(), 1);
}

/// **Forgetting a run with no marks appends nothing and is not an error.**
#[test]
fn an_empty_forget_appends_nothing() {
    let state = State::default();
    assert!(state.forget(&run("homer")).events.is_empty());
}

/// **A clear forgets the run, and the next contact is a first contact again.**
/// The history keeps both `Recorded` events.
#[test]
fn a_clear_then_a_contact_records_once_more() {
    let mut state = State::default();
    let mut log = Vec::new();
    let first = state.first_contact(&run("homer"), "claims", at(0)).unwrap();
    commit(&mut state, &mut log, first);
    let clear = state.forget(&run("homer"));
    assert_eq!(clear.events.len(), 1);
    commit(&mut state, &mut log, clear);
    assert!(!state.has_contact(&run("homer"), "claims"));
    let second = state.first_contact(&run("homer"), "claims", at(2)).unwrap();
    assert!(second.answer);
    commit(&mut state, &mut log, second);
    assert!(state.has_contact(&run("homer"), "claims"));
    assert_eq!(state.history().len(), 2);
    assert_eq!(state.history()[0].2, at(0));
    assert_eq!(state.history()[1].2, at(2));
}

/// **A clear touches only its own run.**
#[test]
fn a_clear_leaves_another_run_taught() {
    let mut state = State::default();
    let mut log = Vec::new();
    for name in ["homer", "marge"] {
        let prepared = state.first_contact(&run(name), "claims", at(0)).unwrap();
        commit(&mut state, &mut log, prepared);
    }
    let clear = state.forget(&run("homer"));
    commit(&mut state, &mut log, clear);
    assert!(!state.has_contact(&run("homer"), "claims"));
    assert!(state.has_contact(&run("marge"), "claims"));
}

/// **A domain is admitted up to 64 characters and refused at 65**, counting
/// characters and not bytes.
#[test]
fn a_domain_is_admitted_at_64_characters_and_refused_at_65() {
    let state = State::default();
    assert!(
        state
            .first_contact(&run("homer"), &"d".repeat(64), at(0))
            .is_ok()
    );
    assert_eq!(
        state.first_contact(&run("homer"), &"d".repeat(65), at(0)),
        Err(Refused::DomainTooWide(65))
    );
    let wide = "é".repeat(64);
    assert!(wide.len() > 64);
    assert!(state.first_contact(&run("homer"), &wide, at(0)).is_ok());
}

/// A small deterministic generator, so the sequences are the same every run.
struct Draw(u64);

impl Draw {
    fn next(&mut self, below: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % below
    }
}

/// **Applying a prepared batch equals replaying the same events from the same
/// prefix**, over generated sequences of contacts and clears across several runs
/// and domains, with duplicates, clears of runs with no marks and reteaching.
/// At every step the state held equals the state replayed from the whole log.
#[test]
fn applying_a_prepared_batch_equals_replaying_it() {
    let runs = ["homer", "marge", "bart"].map(run);
    let domains = ["claims", "mail", "rooms"];
    for seed in 0..40 {
        let mut draw = Draw(seed);
        let mut state = State::default();
        let mut log: Vec<TeachingEvent> = Vec::new();
        for step in 0..60 {
            let who = &runs[draw.next(3) as usize];
            let prefix = state.clone();
            let before = log.len();
            if draw.next(4) == 0 {
                let prepared = state.forget(who);
                commit(&mut state, &mut log, prepared);
            } else {
                let domain = domains[draw.next(3) as usize];
                let prepared = state.first_contact(who, domain, at(step)).unwrap();
                commit(&mut state, &mut log, prepared);
            }
            let batch = &log[before..];
            let mut replayed_batch = prefix;
            for event in batch {
                replayed_batch.apply(event);
            }
            assert_eq!(state, replayed_batch, "seed {seed} step {step}");
            assert_eq!(state, State::replay(&log), "seed {seed} step {step}");
        }
    }
}
