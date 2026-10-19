//! "I read every piece of work on the shed to find the ones in hand. Is there a
//! shorter way?"
//!
//! **A session that reads a whole kind to answer a question is told, by that
//! answer, that one `where` expression would have asked it directly.** The
//! teaching names the call, with an example built from the keys the read itself
//! returned, so it is never a sentence about keys the board does not hold. It rides
//! once per session. A read that already narrows carries none, and neither does a
//! read of a kind too small to be worth narrowing.
//!
//! The shed has eight pieces of work: four in hand, two queued and two done, each
//! owned by one of two people.

use serde_json::json;

use super::dsl::{Answer, Session, Story};

/// The reads that already narrow, each with the key that narrows it. None of them
/// reads a whole kind, so none is a moment to teach `where`.
fn narrowed_reads() -> Vec<(&'static str, serde_json::Value)> {
    vec![
        (
            "where",
            json!({"kind": "work", "where": "status = working", "facts": true}),
        ),
        (
            "fields",
            json!({"kind": "work", "fields": [{"key": "status", "value": "working"}],
                   "facts": true}),
        ),
        ("subject", json!({"subject": "work:gamma", "facts": true})),
        (
            "parent",
            json!({"kind": "work", "parent": "project:the-shed", "facts": true}),
        ),
        ("view", json!({"view": "ready", "facts": true})),
    ]
}

/// A piece of work on the shed with the keys a board holds.
async fn piece(s: &Session, slug: &str, status: &str, owner: &str) {
    s.call(
        "add_entity",
        json!({"kind": "work", "handle": slug, "name": slug.trim_start_matches("work:"),
               "parent": "project:the-shed",
               "source": "user-named", "sets": {"status": status, "owner": owner}}),
    )
    .await;
}

/// The shed, its two people and the eight pieces of work.
async fn the_shed(s: &Session, pieces: &[(&str, &str, &str)]) {
    s.add("project:the-shed", "The shed").await;
    s.add("person:bart", "Bart").await;
    s.add("person:lisa", "Lisa").await;
    for (slug, status, owner) in pieces {
        piece(s, slug, status, owner).await;
    }
}

const EIGHT: [(&str, &str, &str); 8] = [
    ("work:gamma", "working", "person:bart"),
    ("work:delta", "working", "person:lisa"),
    ("work:epsilon", "working", "person:bart"),
    ("work:sigma", "working", "person:bart"),
    ("work:tau", "queued", "person:lisa"),
    ("work:kappa", "queued", "person:bart"),
    ("work:omega", "done", "person:lisa"),
    ("work:xi", "done", "person:bart"),
];

/// The lines of an answer's teaching that name the `where` argument.
fn teaches_where(answer: &Answer) -> Vec<String> {
    answer
        .teachings()
        .into_iter()
        .filter(|line| line.contains("`where`"))
        .collect()
}

/// A session's first read of the whole kind carries the teaching, with an example
/// built from the keys it just read, and the answer is the one it always was.
#[tokio::test]
async fn the_first_read_of_a_whole_kind_names_where_with_an_example_from_its_own_keys() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    the_shed(&s, &EIGHT).await;

    let read = s
        .call("recall", json!({"kind": "work", "facts": true}))
        .await;
    assert_eq!(read.json()["count"], 8, "{}", read.raw());
    let lines = teaches_where(&read);
    assert_eq!(lines.len(), 1, "one teaching names where: {}", read.raw());
    // The example is a key every piece holds, with a value that more than one piece
    // holds: status, whose commonest value is working. It comes with the call.
    assert!(
        lines[0].contains("status = working"),
        "the example is built from the keys the read returned: {}",
        lines[0]
    );
    assert!(
        lines[0].contains("recall") && lines[0].contains("work"),
        "the teaching names the call that runs it: {}",
        lines[0]
    );

    // ── the second whole read in the session carries none ───────────────────
    let again = s
        .call("recall", json!({"kind": "work", "facts": true}))
        .await;
    assert_eq!(again.json()["count"], 8, "{}", again.raw());
    assert!(
        teaches_where(&again).is_empty(),
        "a teaching rides once per session: {}",
        again.raw()
    );

    story.finish().await;
}

/// A read that already narrows carries none, in a session that has not been taught
/// yet, and the same session's whole read after them carries it.
#[tokio::test]
async fn a_read_that_already_narrows_carries_no_teaching_and_leaves_it_for_the_whole_read() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    the_shed(&s, &EIGHT).await;

    for (narrowed_by, args) in narrowed_reads() {
        let read = s.call("recall", args).await;
        assert!(
            teaches_where(&read).is_empty(),
            "a read narrowed by {narrowed_by} was taught where: {}",
            read.raw()
        );
    }
    let whole = s
        .call("recall", json!({"kind": "work", "facts": true}))
        .await;
    assert_eq!(
        teaches_where(&whole).len(),
        1,
        "the reads that narrowed spent nothing: {}",
        whole.raw()
    );

    story.finish().await;
}

/// A kind with a few objects is read whole for free, so the read is not taught; the
/// same session is taught once the kind has grown past the threshold.
#[tokio::test]
async fn a_whole_read_of_a_small_kind_carries_none_and_a_larger_one_does() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    the_shed(&s, &EIGHT[..5]).await;

    let small = s
        .call("recall", json!({"kind": "work", "facts": true}))
        .await;
    assert_eq!(small.json()["count"], 5, "{}", small.raw());
    assert!(
        teaches_where(&small).is_empty(),
        "five pieces are read whole without a teaching: {}",
        small.raw()
    );
    for (slug, status, owner) in &EIGHT[5..] {
        piece(&s, slug, status, owner).await;
    }
    let grown = s
        .call("recall", json!({"kind": "work", "facts": true}))
        .await;
    assert_eq!(grown.json()["count"], 8, "{}", grown.raw());
    assert_eq!(
        teaches_where(&grown).len(),
        1,
        "the small read spent nothing: {}",
        grown.raw()
    );

    story.finish().await;
}

/// **The teaching rides once for each kind a session reads whole**, not once for the
/// session. A session that read one kind whole first and the board's work next is taught on
/// both, each with an example from its own read; a second whole read of either carries
/// none.
#[tokio::test]
async fn a_session_is_taught_once_for_each_kind_it_reads_whole() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    the_shed(&s, &EIGHT).await;
    // Five more projects beside the shed: six in all, five holding a status.
    for (slug, status) in [
        ("project:kitchen-floor", "now"),
        ("project:the-move", "now"),
        ("project:atlas", "now"),
        ("project:migration", "next"),
        ("project:backyard-shed", "done"),
    ] {
        s.call(
            "add_entity",
            json!({"kind": "project", "handle": slug, "name": slug.trim_start_matches("project:"),
                   "source": "user-named", "sets": {"status": status}}),
        )
        .await;
    }

    let projects = s
        .call("recall", json!({"kind": "project", "facts": true}))
        .await;
    let lines = teaches_where(&projects);
    assert_eq!(lines.len(), 1, "projects are taught: {}", projects.raw());
    assert!(
        lines[0].contains("kind \"project\"") || lines[0].contains("of kind project"),
        "the teaching names the kind it was taught on: {}",
        lines[0]
    );
    assert!(
        lines[0].contains("status = now"),
        "the example is from the projects' own keys: {}",
        lines[0]
    );

    // The same session's first whole read of work is taught as well, with its own example.
    let work = s
        .call("recall", json!({"kind": "work", "facts": true}))
        .await;
    let lines = teaches_where(&work);
    assert_eq!(
        lines.len(),
        1,
        "a session taught on projects is taught on work: {}",
        work.raw()
    );
    assert!(
        lines[0].contains("status = working") && lines[0].contains("of kind work"),
        "the example is from the work's own keys: {}",
        lines[0]
    );

    // Neither kind is taught a second time.
    for kind in ["project", "work"] {
        let again = s
            .call("recall", json!({"kind": kind, "facts": true}))
            .await;
        assert!(
            teaches_where(&again).is_empty(),
            "a second whole read of {kind} carries none: {}",
            again.raw()
        );
    }

    story.finish().await;
}
