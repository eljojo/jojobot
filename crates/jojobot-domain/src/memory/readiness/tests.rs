use super::*;
use std::collections::BTreeMap;

/// The things a work item can name, by handle: kind, fields, archived.
struct World {
    named: BTreeMap<String, (String, BTreeMap<String, String>, bool)>,
    instance: BTreeMap<String, String>,
}

fn bag(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

impl World {
    fn new() -> World {
        World {
            named: BTreeMap::new(),
            instance: BTreeMap::new(),
        }
    }

    fn work(mut self, handle: &str, status: &str) -> World {
        self.named.insert(
            handle.to_string(),
            ("work".into(), bag(&[("status", status)]), false),
        );
        self
    }

    fn archived_work(mut self, handle: &str, status: &str) -> World {
        self.named.insert(
            handle.to_string(),
            ("work".into(), bag(&[("status", status)]), true),
        );
        self
    }
}

impl Lookup for World {
    fn thing(&self, handle: &str) -> Option<Subject<'_>> {
        self.named
            .get(handle)
            .map(|(kind, fields, archived)| Subject {
                kind,
                fields,
                archived: *archived,
            })
    }

    fn instance(&self) -> &BTreeMap<String, String> {
        &self.instance
    }
}

/// A work item holding these fields, as the tree sees it.
fn item(fields: &BTreeMap<String, String>) -> Subject<'_> {
    Subject {
        kind: "work",
        fields,
        archived: false,
    }
}

fn dependency(handle: &str, why: Why) -> Blocker {
    Blocker::Dependency {
        handle: handle.to_string(),
        why,
    }
}

/// **Two finished dependencies: ready, and nothing blocks.** Its pair is the
/// next case, one dependency unfinished.
#[test]
fn work_whose_dependencies_are_all_finished_is_ready() {
    let world = World::new()
        .work("work:phi", "done")
        .work("work:sigma", "done");
    let milhouse = bag(&[("depends_on", "work:phi, work:sigma")]);
    assert!(is_ready(&item(&milhouse), &world));
    assert!(blocked_by(&item(&milhouse), &world).is_empty());
}

/// **One unfinished dependency: not ready, and blocked_by names exactly it.**
#[test]
fn an_unfinished_dependency_blocks_and_is_named() {
    let world = World::new()
        .work("work:phi", "done")
        .work("work:tau", "now");
    let milhouse = bag(&[("depends_on", "work:phi, work:tau")]);
    assert!(!is_ready(&item(&milhouse), &world));
    assert_eq!(
        blocked_by(&item(&milhouse), &world),
        [dependency("work:tau", Why::Unfinished)]
    );
}

/// **A held waiting_on blocks and is named**; the same work without it is ready.
#[test]
fn a_held_waiting_on_blocks_and_is_named() {
    let world = World::new().work("work:phi", "done");
    let waiting = bag(&[
        ("depends_on", "work:phi"),
        ("waiting_on", "person:milhouse"),
    ]);
    let free = bag(&[("depends_on", "work:phi")]);
    assert!(!is_ready(&item(&waiting), &world));
    assert_eq!(
        blocked_by(&item(&waiting), &world),
        [Blocker::WaitingOn("person:milhouse".into())]
    );
    assert!(is_ready(&item(&free), &world));
}

/// **No dependencies at all: ready.**
#[test]
fn work_with_no_dependencies_is_ready() {
    let world = World::new();
    assert!(is_ready(&item(&bag(&[])), &world));
    assert!(is_ready(&item(&bag(&[("depends_on", "")])), &world));
    assert!(blocked_by(&item(&bag(&[])), &world).is_empty());
}

/// **An archived dependency blocks and is named as archived**, although its
/// status says done. Its pair is the same dependency, live.
#[test]
fn an_archived_dependency_blocks_and_is_named() {
    let live = World::new().work("work:phi", "done");
    let archived = World::new().archived_work("work:phi", "done");
    let milhouse = bag(&[("depends_on", "work:phi")]);
    assert!(is_ready(&item(&milhouse), &live));
    assert!(!is_ready(&item(&milhouse), &archived));
    assert_eq!(
        blocked_by(&item(&milhouse), &archived),
        [dependency("work:phi", Why::Archived)]
    );
}

/// **A handle that names nothing blocks and is named as dangling.**
#[test]
fn a_dangling_dependency_blocks_and_is_named() {
    let world = World::new().work("work:phi", "done");
    let milhouse = bag(&[("depends_on", "work:phi, work:atlas-root")]);
    assert!(!is_ready(&item(&milhouse), &world));
    assert_eq!(
        blocked_by(&item(&milhouse), &world),
        [dependency("work:atlas-root", Why::Dangling)]
    );
}

/// **A cycle reads as blocked on both sides.** A needs B and B needs A, neither
/// finished. One hop: each is blocked by the other being unfinished.
#[test]
fn a_cycle_is_blocked_on_both_sides() {
    let mut world = World::new();
    world.named.insert(
        "work:phi".into(),
        (
            "work".into(),
            bag(&[("status", "next"), ("depends_on", "work:sigma")]),
            false,
        ),
    );
    world.named.insert(
        "work:sigma".into(),
        (
            "work".into(),
            bag(&[("status", "next"), ("depends_on", "work:phi")]),
            false,
        ),
    );
    for (holder, other) in [("work:phi", "work:sigma"), ("work:sigma", "work:phi")] {
        let fields = world.named[holder].1.clone();
        assert!(!is_ready(&item(&fields), &world), "{holder}");
        assert_eq!(
            blocked_by(&item(&fields), &world),
            [dependency(other, Why::Unfinished)],
            "{holder}"
        );
    }
}

/// **Every blocker is named, dependencies first in the order held, then what it
/// waits on.**
#[test]
fn every_blocker_is_named_in_order() {
    let world = World::new()
        .work("work:tau", "now")
        .archived_work("work:phi", "done");
    let milhouse = bag(&[
        ("depends_on", "work:tau, work:phi, work:atlas-root"),
        ("waiting_on", "person:milhouse"),
    ]);
    assert_eq!(
        blocked_by(&item(&milhouse), &world),
        [
            dependency("work:tau", Why::Unfinished),
            dependency("work:phi", Why::Archived),
            dependency("work:atlas-root", Why::Dangling),
            Blocker::WaitingOn("person:milhouse".into()),
        ]
    );
}

/// **The tree and the blocker list never disagree.** Over a table of work, the
/// tree holds exactly when nothing blocks.
#[test]
fn ready_is_exactly_nothing_blocking() {
    let world = World::new()
        .work("work:phi", "done")
        .work("work:tau", "now")
        .archived_work("work:sigma", "done");
    let cases = [
        bag(&[]),
        bag(&[("depends_on", "work:phi")]),
        bag(&[("depends_on", "work:tau")]),
        bag(&[("depends_on", "work:sigma")]),
        bag(&[("depends_on", "work:atlas-root")]),
        bag(&[
            ("depends_on", "work:phi"),
            ("waiting_on", "person:milhouse"),
        ]),
        bag(&[("waiting_on", "person:milhouse")]),
        bag(&[("status", "done")]),
        bag(&[("status", "now"), ("depends_on", "work:phi")]),
    ];
    let ready: Vec<bool> = cases
        .iter()
        .map(|fields| is_ready(&item(fields), &world))
        .collect();
    let unblocked: Vec<bool> = cases
        .iter()
        .map(|fields| blocked_by(&item(fields), &world).is_empty())
        .collect();
    assert_eq!(ready, unblocked);
    assert_eq!(
        ready,
        [true, true, false, false, false, false, false, false, true]
    );
}

/// **Work that is itself finished is not ready, and blocked_by says why.** Its
/// pair is the same work one status earlier, which is ready.
#[test]
fn finished_work_is_not_ready_and_says_so() {
    let world = World::new();
    let done = bag(&[("status", "done")]);
    let under_way = bag(&[("status", "now")]);
    assert!(!is_ready(&item(&done), &world));
    assert_eq!(blocked_by(&item(&done), &world), [Blocker::Finished]);
    assert!(is_ready(&item(&under_way), &world));
    assert!(blocked_by(&item(&under_way), &world).is_empty());
}

/// **Archived work is not ready, and blocked_by says it is archived.** Its pair
/// is the same fields unarchived, which is ready. The tree and the blocker list
/// agree about both.
#[test]
fn archived_work_is_not_ready_and_says_so() {
    let world = World::new();
    let fields = bag(&[("status", "now")]);
    let archived = Subject {
        archived: true,
        ..item(&fields)
    };
    assert!(!is_ready(&archived, &world));
    assert_eq!(blocked_by(&archived, &world), [Blocker::Archived]);
    assert!(is_ready(&item(&fields), &world));
    assert!(blocked_by(&item(&fields), &world).is_empty());
}

/// **Finished work that also waits on something names both**, finished first.
#[test]
fn finished_work_names_every_blocker() {
    let world = World::new().work("work:tau", "now");
    let fields = bag(&[
        ("status", "done"),
        ("depends_on", "work:tau"),
        ("waiting_on", "person:milhouse"),
    ]);
    assert_eq!(
        blocked_by(&item(&fields), &world),
        [
            Blocker::Finished,
            dependency("work:tau", Why::Unfinished),
            Blocker::WaitingOn("person:milhouse".into()),
        ]
    );
}

/// **A status word the instance maps to done finishes the work too**, by the
/// same definition a dependency is held to.
#[test]
fn the_work_itself_follows_the_instance_status_words() {
    let mut world = World::new();
    world
        .instance
        .insert("status_word/shipped".into(), "done".into());
    let shipped = bag(&[("status", "shipped")]);
    assert!(!is_ready(&item(&shipped), &world));
    let unmapped = bag(&[("status", "shipped")]);
    assert!(is_ready(&item(&unmapped), &World::new()));
}
