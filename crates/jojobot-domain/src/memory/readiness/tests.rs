use super::*;

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
        .work("work:bart", "done")
        .work("work:lisa", "done");
    let milhouse = bag(&[("depends_on", "work:bart, work:lisa")]);
    assert!(is_ready(&milhouse, &world));
    assert!(blocked_by(&milhouse, &world).is_empty());
}

/// **One unfinished dependency: not ready, and blocked_by names exactly it.**
#[test]
fn an_unfinished_dependency_blocks_and_is_named() {
    let world = World::new()
        .work("work:bart", "done")
        .work("work:homer", "now");
    let milhouse = bag(&[("depends_on", "work:bart, work:homer")]);
    assert!(!is_ready(&milhouse, &world));
    assert_eq!(
        blocked_by(&milhouse, &world),
        [dependency("work:homer", Why::Unfinished)]
    );
}

/// **A held waiting_on blocks and is named**; the same work without it is ready.
#[test]
fn a_held_waiting_on_blocks_and_is_named() {
    let world = World::new().work("work:bart", "done");
    let waiting = bag(&[("depends_on", "work:bart"), ("waiting_on", "person:marge")]);
    let free = bag(&[("depends_on", "work:bart")]);
    assert!(!is_ready(&waiting, &world));
    assert_eq!(
        blocked_by(&waiting, &world),
        [Blocker::WaitingOn("person:marge".into())]
    );
    assert!(is_ready(&free, &world));
}

/// **No dependencies at all: ready.**
#[test]
fn work_with_no_dependencies_is_ready() {
    let world = World::new();
    assert!(is_ready(&bag(&[]), &world));
    assert!(is_ready(&bag(&[("depends_on", "")]), &world));
    assert!(blocked_by(&bag(&[]), &world).is_empty());
}

/// **An archived dependency blocks and is named as archived**, although its
/// status says done. Its pair is the same dependency, live.
#[test]
fn an_archived_dependency_blocks_and_is_named() {
    let live = World::new().work("work:bart", "done");
    let archived = World::new().archived_work("work:bart", "done");
    let milhouse = bag(&[("depends_on", "work:bart")]);
    assert!(is_ready(&milhouse, &live));
    assert!(!is_ready(&milhouse, &archived));
    assert_eq!(
        blocked_by(&milhouse, &archived),
        [dependency("work:bart", Why::Archived)]
    );
}

/// **A handle that names nothing blocks and is named as dangling.**
#[test]
fn a_dangling_dependency_blocks_and_is_named() {
    let world = World::new().work("work:bart", "done");
    let milhouse = bag(&[("depends_on", "work:bart, work:nobody")]);
    assert!(!is_ready(&milhouse, &world));
    assert_eq!(
        blocked_by(&milhouse, &world),
        [dependency("work:nobody", Why::Dangling)]
    );
}

/// **A cycle reads as blocked on both sides.** A needs B and B needs A, neither
/// finished. One hop: each is blocked by the other being unfinished.
#[test]
fn a_cycle_is_blocked_on_both_sides() {
    let mut world = World::new();
    world.named.insert(
        "work:bart".into(),
        (
            "work".into(),
            bag(&[("status", "next"), ("depends_on", "work:lisa")]),
            false,
        ),
    );
    world.named.insert(
        "work:lisa".into(),
        (
            "work".into(),
            bag(&[("status", "next"), ("depends_on", "work:bart")]),
            false,
        ),
    );
    for (holder, other) in [("work:bart", "work:lisa"), ("work:lisa", "work:bart")] {
        let fields = world.named[holder].1.clone();
        assert!(!is_ready(&fields, &world), "{holder}");
        assert_eq!(
            blocked_by(&fields, &world),
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
        .work("work:homer", "now")
        .archived_work("work:bart", "done");
    let milhouse = bag(&[
        ("depends_on", "work:homer, work:bart, work:nobody"),
        ("waiting_on", "person:marge"),
    ]);
    assert_eq!(
        blocked_by(&milhouse, &world),
        [
            dependency("work:homer", Why::Unfinished),
            dependency("work:bart", Why::Archived),
            dependency("work:nobody", Why::Dangling),
            Blocker::WaitingOn("person:marge".into()),
        ]
    );
}

/// **The tree and the blocker list never disagree.** Over a table of work, the
/// tree holds exactly when nothing blocks.
#[test]
fn ready_is_exactly_nothing_blocking() {
    let world = World::new()
        .work("work:bart", "done")
        .work("work:homer", "now")
        .archived_work("work:lisa", "done");
    let cases = [
        bag(&[]),
        bag(&[("depends_on", "work:bart")]),
        bag(&[("depends_on", "work:homer")]),
        bag(&[("depends_on", "work:lisa")]),
        bag(&[("depends_on", "work:nobody")]),
        bag(&[("depends_on", "work:bart"), ("waiting_on", "person:marge")]),
        bag(&[("waiting_on", "person:marge")]),
    ];
    let ready: Vec<bool> = cases
        .iter()
        .map(|fields| is_ready(fields, &world))
        .collect();
    let unblocked: Vec<bool> = cases
        .iter()
        .map(|fields| blocked_by(fields, &world).is_empty())
        .collect();
    assert_eq!(ready, unblocked);
    assert_eq!(ready, [true, true, false, false, false, false, false]);
}
