//! ⚠️ UNFINISHED DRAFT — IGNORED, AND IT DOES NOT COUNT AS COVERAGE.
//!
//! It runs only with `--ignored`, deliberately, and its passing means nothing:
//! it is parked mid-thought rather than finished. **This repo has been bitten
//! before by an `#[ignore]`d test treated as if it ran**, so the marker is loud
//! on purpose. Finish it or delete it; do not let it sit here looking green.
//!
//! What is unresolved: the incident class it is about happens in the AGENT'S
//! MOUTH, not in jojobot's storage. jojobot can hold provenance perfectly and a
//! session can still state a guess as fact. So a test at this surface cannot
//! reach the failure — it can only check that jojobot handed over enough to
//! avoid it, which is real but far smaller than the story implies.
//!
//! "Where did you get that?" — jojobot said something, he acted on it, it was wrong.
//!
//! The incident class this whole project is blocked on: a claim nobody heard
//! from anybody, stated confidently, acted on, and false. The question is not
//! whether jojobot can store a correction. It is whether the wrong claim can
//! still hurt somebody after it has been corrected.
//!
//! Milestone: "I can ask why, and the answer holds."
//!
//! `// GAP —` marks a call that cannot be made today.

use super::dsl::Story;

#[tokio::test]
#[ignore = "UNFINISHED DRAFT — parked mid-thought; passing proves nothing"]
async fn where_did_you_get_that() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · research, and a claim nobody actually said ──────────────
    let s = story.session().await;

    s.add("place:moes", "Moe's").await;

    // Testimony and inference, side by side, about the same place. He said the
    // first. Nobody said the second — it came out of a search summary, which is
    // the exact door rule 47 calls prompt injection.
    s.fact(
        "place:moes",
        "he likes it, goes there when the week has been long",
    )
    .await;
    let claim = s.guess("place:moes", "open until 2am on Sundays").await;

    s.wrap("one thing he said, one thing we worked out").await;

    // ── session 2 · he went. it was shut. ───────────────────────────────────
    let s = story.session().await;

    // The correction, done properly: the claim is REWRITTEN in place, so a
    // reader gets one answer rather than two and a judgement. Rule 58.
    s.correct(
        &claim,
        "closes at 6 on Sundays — he went on a Sunday and it was shut",
    )
    .await;

    // THE ASSERTION THAT CAN ACTUALLY FAIL, and it is the point of the story:
    // the wrong wording is GONE, not outvoted. If a correction left the old
    // claim reachable, every later session would find both and pick one.
    // Each negative is PAIRED with the positive it needs. A bare "it is not in
    // the results" cannot tell a correction that worked from a read that
    // returned nothing at all — and the second is the failure worth catching.
    s.recall("place:moes")
        .await
        .says("closes at 6")
        .never_says("until 2am");
    s.find("Sundays")
        .await
        .says("place:moes")
        .never_says("until 2am");

    // And the thing he actually said is untouched by the correction.
    s.recall("place:moes").await.says("week has been long");

    // GAP — "where did you get that" is still unanswerable. The claim reads back
    // as an inference, which says it was DERIVED and not what from. There is no
    // source, no session, no earlier fact — and an inference with no visible
    // parent is exactly as unfalsifiable as a fabrication, which is what this
    // project is blocked on.
    // s.why(&claim).says("a search summary, session 1");

    // GAP — AND THE ONE I HAD NOT SEEN UNTIL THE STORY HAD STAKES. Nothing
    // records that a claim was ACTED ON. A wrong guess that sat unread and a
    // wrong guess that sent him across town on a Sunday are the same object
    // here. That difference is the whole difference between a tidy graph and a
    // wasted evening, and it is the difference the incidents in his history are
    // made of.
    // s.acted_on(&claim, "he went").await;

    s.wrap("corrected, and we still cannot say where it came from")
        .await;

    // ── session 3 · a fresh session, weeks later, reaches the same conclusion ─
    let s = story.session().await;

    // Nothing stops it. The corrected fact states the negative truth, but a new
    // session doing new research will infer the old claim again — and this time
    // it will be a NEW fact, not a rewrite, so the correction it never saw does
    // not apply to it.
    let again = s.guess("place:moes", "open until 2am on Sundays").await;
    s.recall("place:moes").await.says("until 2am");

    // GAP — the corrected claim and the re-inferred one now sit side by side,
    // and the graph cannot tell which one he already rejected. A rejection has
    // to be a durable negative that later derivations SUBTRACT, or he corrects
    // the same thing forever and eventually stops trusting the correction.
    // s.rejected(&claim).so_that(&again).is_blocked().await;

    // Cleaning up after ourselves, since this is his real store's shape: the
    // re-inference is itself corrected, which is what a session would have to
    // do by hand every single time.
    s.correct(
        &again,
        "closes at 6 on Sundays — already established, do not re-derive",
    )
    .await;

    s.wrap("the same wrong claim, twice, from the same absence")
        .await;

    story.finish().await;
}
