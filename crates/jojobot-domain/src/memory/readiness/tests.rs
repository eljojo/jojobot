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
/// **One unfinished dependency: not ready, and blocked_by names exactly it.**
/// **A held waiting_on blocks and is named**; the same work without it is ready.
/// **No dependencies at all: ready.**
/// **An archived dependency blocks and is named as archived**, although its
/// status says done. Its pair is the same dependency, live.
/// **A handle that names nothing blocks and is named as dangling.**
/// **Every blocker is named, dependencies first in the order held, then what it
/// waits on.**
/// **The tree and the blocker list never disagree.** Over a table of work, the
/// tree holds exactly when nothing blocks.
/// **Work that is itself finished is not ready, and blocked_by says why.** Its
/// pair is the same work one status earlier, which is ready.
/// **Archived work is not ready, and blocked_by says it is archived.** Its pair
/// is the same fields unarchived, which is ready. The tree and the blocker list
/// agree about both.
/// **Finished work that also waits on something names both**, finished first.
#[test]
fn readiness_and_blockers_cover_finished_waiting_and_missing_work() {
    let empty = World::new();
    let live = World::new().work("work:phi", "done");
    let finished = World::new()
        .work("work:phi", "done")
        .work("work:sigma", "done");
    let unfinished = World::new()
        .work("work:phi", "done")
        .work("work:tau", "now");
    let archived = World::new().archived_work("work:phi", "done");
    let blocked = World::new()
        .work("work:tau", "now")
        .archived_work("work:phi", "done");
    let waiting = World::new().work("work:tau", "now");
    let consistent = World::new()
        .work("work:phi", "done")
        .work("work:tau", "now")
        .archived_work("work:sigma", "done");
    let both_done = bag(&[("depends_on", "work:phi, work:sigma")]);
    let mixed = bag(&[("depends_on", "work:phi, work:tau")]);
    let needs_phi = bag(&[("depends_on", "work:phi")]);
    let waits = bag(&[
        ("depends_on", "work:phi"),
        ("waiting_on", "person:milhouse"),
    ]);
    let none = bag(&[]);
    let blank = bag(&[("depends_on", "")]);
    let dangling = bag(&[("depends_on", "work:phi, work:atlas-root")]);
    let all_blocked = bag(&[
        ("depends_on", "work:tau, work:phi, work:atlas-root"),
        ("waiting_on", "person:milhouse"),
    ]);
    let done = bag(&[("status", "done")]);
    let now = bag(&[("status", "now")]);
    let done_waiting = bag(&[
        ("status", "done"),
        ("depends_on", "work:tau"),
        ("waiting_on", "person:milhouse"),
    ]);
    let needs_tau = bag(&[("depends_on", "work:tau")]);
    let needs_sigma = bag(&[("depends_on", "work:sigma")]);
    let root = bag(&[("depends_on", "work:atlas-root")]);
    let waiting_only = bag(&[("waiting_on", "person:milhouse")]);
    let now_phi = bag(&[("status", "now"), ("depends_on", "work:phi")]);
    let waits_on = || Blocker::WaitingOn("person:milhouse".into());
    for (row, (world, fields, archived, expected)) in [
        (&finished, &both_done, false, vec![]),
        (
            &unfinished,
            &mixed,
            false,
            vec![dependency("work:tau", Why::Unfinished)],
        ),
        (&live, &waits, false, vec![waits_on()]),
        (&live, &needs_phi, false, vec![]),
        (&empty, &none, false, vec![]),
        (&empty, &blank, false, vec![]),
        (&live, &needs_phi, false, vec![]),
        (
            &archived,
            &needs_phi,
            false,
            vec![dependency("work:phi", Why::Archived)],
        ),
        (
            &live,
            &dangling,
            false,
            vec![dependency("work:atlas-root", Why::Dangling)],
        ),
        (
            &blocked,
            &all_blocked,
            false,
            vec![
                dependency("work:tau", Why::Unfinished),
                dependency("work:phi", Why::Archived),
                dependency("work:atlas-root", Why::Dangling),
                waits_on(),
            ],
        ),
        (&empty, &done, false, vec![Blocker::Finished]),
        (&empty, &now, false, vec![]),
        (&empty, &now, true, vec![Blocker::Archived]),
        (&empty, &now, false, vec![]),
        (
            &waiting,
            &done_waiting,
            false,
            vec![
                Blocker::Finished,
                dependency("work:tau", Why::Unfinished),
                waits_on(),
            ],
        ),
        (&consistent, &none, false, vec![]),
        (&consistent, &needs_phi, false, vec![]),
        (
            &consistent,
            &needs_tau,
            false,
            vec![dependency("work:tau", Why::Unfinished)],
        ),
        (
            &consistent,
            &needs_sigma,
            false,
            vec![dependency("work:sigma", Why::Archived)],
        ),
        (
            &consistent,
            &root,
            false,
            vec![dependency("work:atlas-root", Why::Dangling)],
        ),
        (&consistent, &waits, false, vec![waits_on()]),
        (&consistent, &waiting_only, false, vec![waits_on()]),
        (&consistent, &done, false, vec![Blocker::Finished]),
        (&consistent, &now_phi, false, vec![]),
    ]
    .into_iter()
    .enumerate()
    {
        let subject = Subject {
            archived,
            ..item(fields)
        };
        assert_eq!(
            is_ready(&subject, world),
            expected.is_empty(),
            "{row}: {fields:?}"
        );
        assert_eq!(blocked_by(&subject, world), expected, "{row}: {fields:?}");
    }
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

/// **The ready ask keeps what is ready; the plan ask keeps everything the work
/// itself has not ended.** Blocked work is in a plan and not in the ready list;
/// finished and archived work is in neither.
#[test]
fn the_plan_ask_keeps_blocked_work_and_neither_ask_keeps_finished_or_archived() {
    let blocked = [dependency("work:tau", Why::Unfinished)];
    assert!(Ask::Ready.keeps(true, &[]));
    assert!(!Ask::Ready.keeps(false, &blocked));
    assert!(Ask::Plan.keeps(true, &[]));
    assert!(Ask::Plan.keeps(false, &blocked));
    assert!(Ask::Plan.keeps(false, &[Blocker::WaitingOn("person:milhouse".into())]));
    assert!(!Ask::Plan.keeps(false, &[Blocker::Finished]));
    assert!(!Ask::Plan.keeps(false, &[Blocker::Archived, blocked[0].clone()]));
    assert!(!Ask::Ready.keeps(false, &[Blocker::Finished]));
}

/// **A view's `asks` word names its ask**, and any other word names none.
#[test]
fn an_asks_word_names_its_ask() {
    assert_eq!(Ask::named(ASK), Some(Ask::Ready));
    assert_eq!(Ask::named(ASK_PLAN), Some(Ask::Plan));
    assert_eq!(Ask::named("overdue"), None);
}
