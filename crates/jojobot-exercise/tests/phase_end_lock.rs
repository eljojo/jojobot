//! **A lock that asks its question at the end of its own phase, proved against a
//! real room.**
//!
//! `window own-phase` matches needles over a fixed snapshot. A question the
//! server computes, such as what is owed as of a day, cannot be read from a
//! snapshot, so `window phase-end` runs the lock's own query when its phase
//! ends and keeps the answer on the boundary. Three claims, each with a case:
//! the answer holds when a later phase changes the state, a lock without the
//! window still reads the end of the run, and the stored answer is the query's
//! own answer and not a needle over the snapshot.

use jojobot_exercise::lock::{Lock, read};
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{
    Boundary, Expectation, Observed, boundary, boundary_asking, phase_end_queries,
};
use jojobot_exercise::surface::{Seed, Surface};
use serde_json::json;

fn locks(window: &str) -> Vec<Box<dyn Expectation>> {
    let doc = format!(
        "## Phase 1 \u{2014} the room\n\n\
         ```locks\n\
         recall {{\"subject\": \"person:milhouse\", \"facts\": true}}\n\
         carries marker\n\
         lacks   laterword\n\
         say     the marker is on record\n\
         {window}\n\
         ```\n"
    );
    read(&doc)
        .expect("the lock reads")
        .into_iter()
        .map(|lock: Lock| Box::new(lock) as Box<dyn Expectation>)
        .collect()
}

async fn room_with_a_marker() -> (Room, Surface, String) {
    let (room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    Seed::new()
        .entity("person", "milhouse", "Milhouse")
        .expect("an entity builds")
        .furnish(&surface)
        .await
        .expect("furnished");
    let booted = surface
        .must("start_here", json!({"bot": "assistant", "brief": true}))
        .await
        .expect("boot");
    let sid = booted["session"]["sid"].as_str().expect("sid").to_string();
    let written = surface
        .call(
            "capture",
            json!({"subject": "person:milhouse", "content": "a marker for the first sitting",
                   "provenance": "testimony", "sid": &sid}),
        )
        .await;
    assert!(!written.contains("blocked"), "{written}");
    (room, surface, sid)
}

/// What the run would have recorded: one boundary ahead of each phase, the one
/// after phase 1 holding the answers phase 1's own locks asked for.
async fn boundaries_after_phase_one(
    surface: &Surface,
    expectations: &[Box<dyn Expectation>],
) -> Vec<Boundary> {
    vec![
        boundary(surface, "Phase 1 \u{2014} the room").await,
        boundary_asking(
            surface,
            "Phase 2 \u{2014} the next",
            &phase_end_queries(expectations, "Phase 1"),
        )
        .await,
    ]
}

/// A later phase writes what the first phase's lock says must not be there yet.
async fn phase_two_flips_the_state(surface: &Surface, sid: &str) {
    let said = surface
        .call(
            "capture",
            json!({"subject": "person:milhouse", "content": "a laterword from the second sitting",
                   "provenance": "testimony", "sid": sid}),
        )
        .await;
    assert!(!said.contains("blocked"), "{said}");
}

/// **A lock with the window answers from the end of its phase, though a later
/// phase flips the state.**
#[tokio::test]
async fn a_phase_end_lock_holds_the_answer_of_its_own_phase_when_a_later_phase_flips_it() {
    let (_room, surface, sid) = room_with_a_marker().await;
    let asked = locks("window  phase-end");
    let boundaries = boundaries_after_phase_one(&surface, &asked).await;
    phase_two_flips_the_state(&surface, &sid).await;
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = asked[0].check(&seen).await;
    assert!(outcome.held, "{}", outcome.saying);
}

/// **The same lock without the window reads the end of the run, as every lock
/// did before.** Paired with the case above: the state really did flip.
#[tokio::test]
async fn the_same_lock_without_the_window_reads_the_end_of_the_run() {
    let (_room, surface, sid) = room_with_a_marker().await;
    let plain = locks("");
    let boundaries = boundaries_after_phase_one(&surface, &plain).await;
    phase_two_flips_the_state(&surface, &sid).await;
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = plain[0].check(&seen).await;
    assert!(!outcome.held, "{}", outcome.saying);
}

/// **What is stored is the live query's answer.** A `recall` envelope carries a
/// key no listing or search snapshot has, so finding it on the boundary and not
/// in the snapshot says the query was sent.
#[tokio::test]
async fn the_answer_stored_on_the_boundary_is_the_live_querys_answer() {
    let (_room, surface, _sid) = room_with_a_marker().await;
    let asked = locks("window  phase-end");
    let boundaries = boundaries_after_phase_one(&surface, &asked).await;
    let after = &boundaries[1];
    assert_eq!(after.answers.len(), 1, "{:?}", after.answers);
    assert_eq!(after.answers[0].phase, "Phase 1");
    assert!(
        after.answers[0].answer.contains("\"overdue_as_of\""),
        "{}",
        after.answers[0].answer
    );
    assert!(!after.world.contains("\"overdue_as_of\""));
}

/// **A lock with no window asks nothing at a phase's end.**
#[tokio::test]
async fn a_lock_without_the_window_stores_no_answer() {
    let (_room, surface, _sid) = room_with_a_marker().await;
    let plain = locks("");
    let boundaries = boundaries_after_phase_one(&surface, &plain).await;
    assert!(boundaries[1].answers.is_empty());
}

/// **A lock naming a check cannot take the window**, and neither can a lock
/// under no phase, for the reasons `own-phase` already gives.
#[test]
fn the_window_is_refused_where_there_is_nothing_to_ask() {
    let on_a_check = "## Phase 1 \u{2014} x\n\n```locks\ncheck   a_named_check\nsay     s\nwindow  phase-end\n```\n";
    assert!(read(on_a_check).is_err());
    let no_phase = "```locks\nrecall {\"subject\": \"person:milhouse\"}\ncarries a\nsay     s\nwindow  phase-end\n```\n";
    assert!(read(no_phase).is_err());
}
