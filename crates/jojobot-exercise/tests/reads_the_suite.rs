//! **The reader is held against the real playbook, which is the primary
//! source.**
//!
//! The unit cases use a trivial fixture, and a fixture is something this crate
//! wrote — so on its own it proves the reader can read what this crate expects.
//! The document a run is actually driven by is authored elsewhere, and the seam
//! between the two is exactly where a slice like this breaks: a heading style
//! that changes, a marker written another way, a block that stops being a block
//! quote. None of that is anybody's mistake, and all of it must be loud.
//!
//! So this reads the shipped suite and asserts its SHAPE — never its words. It
//! goes red the day the document changes in a way the harness has to be told
//! about, which is the only day anybody wants to hear from it.

use jojobot_exercise::playbook::Playbook;

fn suite() -> Playbook {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("COLD-SESSION-SUITE.md");
    Playbook::read(&path).unwrap_or_else(|e| panic!("the shipped suite must read: {e:#}"))
}

/// Every phase carries something to say, and none of it is the maintainer's.
#[test]
fn the_shipped_suite_reads_as_phases_with_something_to_say() {
    let read = suite();
    assert!(
        read.phases.len() > 1,
        "a suite that spans sessions has more than one phase: {:?}",
        read.phases.iter().map(|p| &p.name).collect::<Vec<_>>(),
    );
    for phase in &read.phases {
        assert!(
            !phase.prompt.is_empty(),
            "{:?} has no block addressed to the model",
            phase.name,
        );
        // The document addresses its maintainer outside the block and the model
        // inside it. A reader that took the whole section would sweep up the
        // notes, and the model would be told about the harness testing it.
        assert!(
            !phase.prompt.contains("**Session:"),
            "{:?} carries the maintainer's session marker into what the model is \
             told: {:?}",
            phase.name,
            phase.prompt,
        );
    }
}

/// **Both markers really occur**, so the two branches of the run loop are both
/// reachable from the shipped document. A suite that read as all-fresh or
/// all-continuing would drive a run that exercises one of them and nothing
/// would say so.
#[test]
fn the_shipped_suite_spans_more_than_one_session() {
    let read = suite();
    assert!(
        read.phases[0].fresh_session,
        "the first phase of a cold suite starts cold: {:?}",
        read.phases[0].name,
    );
    assert!(
        read.phases.iter().skip(1).any(|p| p.fresh_session),
        "no later phase starts fresh, so nothing measures what a cold session finds",
    );
    assert!(
        read.phases.iter().any(|p| !p.fresh_session),
        "every phase starts fresh, so nothing measures a session carrying on",
    );
}
