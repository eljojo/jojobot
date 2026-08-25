//! "Remember two things. The diner serves breakfast till 11 — that one I'm
//! sure of. And I think Moe's closes early on Sundays, but don't hold me to
//! that."
//!
//! Two questions live in that sentence, and they have different answers. Who
//! backs the claim: the operator, both times. How sure anyone is: settled, then
//! open. `provenance` carries the first and `standing` the second.
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for, and
//! an assertion beside it goes red on the day the capability lands.
//!
//! `// NOTE —` marks something a SESSION did not do. jojobot answers nothing
//! about it, so no assertion can hold it and none pretends to — and it
//! proposes no call either, because there is no verb that would fix it.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_hedged_claim_and_a_guess_no_longer_read_the_same() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · the two places ──────────────────────────────────────────
    let s = story.session().await;

    s.add("place:leftorium", "The Leftorium Diner").await;
    s.add("place:moes", "Moe's").await;

    s.wrap("standing up the two places").await;

    // ── session 2 · both things, in one breath ──────────────────────────────
    let s = story.session().await;

    // Settled and first hand.
    let breakfast = s
        .fact("place:leftorium", "serves breakfast until 11am on weekdays")
        .await;

    // Hedged and first hand: the operator backs it and says they are not sure.
    // Both halves go in as themselves, and nothing goes into free text.
    let hedged = s.hedged("place:moes", "closes early on Sundays").await;

    s.wrap("recorded both, each saying what it actually is")
        .await;

    // ── session 3 · a session works a third claim out of the first two ──────
    let s = story.session().await;

    // Genuinely derived: it follows from the hedged closing time and from the
    // settled breakfast hours, and nobody said it.
    let derived = s
        .guess_from(
            "place:moes",
            "a Sunday visit should be earlier than a weekday one",
            &hedged,
        )
        .await;

    // GAP — `derived_from` holds one address. This claim has two parents and
    // can name one, so the trail it leaves is not the trail it has. Which
    // parent to drop is a judgement the field forces and does not record.
    //   s.guess_from_all("place:moes", "…", &[hedged, breakfast]).await;
    s.recall("place:moes")
        .await
        .claim(&derived)
        .says(&format!("\"derived_from\":\"{hedged}\""))
        .never_says("\"derived_from\":[");

    s.wrap("worked out a third claim from the first two").await;

    // ── session 4 · which of these can be relied on? ────────────────────────
    let s = story.session().await;

    // What the operator stated flatly: they back it, and it is settled.
    s.recall("place:leftorium")
        .await
        .says("serves breakfast until 11am")
        .says("\"provenance\":\"testimony\"")
        .says("\"standing\":\"settled\"");

    // Both claims about the other place come back in one read — the operator's
    // hedge and the session's invention. They agree on how sure anyone is and
    // differ on who backs them, which is why one field could never carry both.
    // Each is checked as itself, bound to its address.
    let read = s.recall("place:moes").await;
    read.claim(&hedged)
        .says("closes early on Sundays")
        .says("\"provenance\":\"testimony\"")
        .says("\"standing\":\"open\"");
    read.claim(&derived)
        .says("a Sunday visit should be earlier")
        .says("\"provenance\":\"inference\"")
        .says("\"standing\":\"open\"");

    // And nothing was smuggled into free text to carry the hedge: it is a
    // field, so the note is empty on the claim that carries one.
    read.claim(&hedged)
        .says("\"details\":null")
        .never_says("was not sure");

    // ── and the question an agent asks about its own work ───────────────────
    //
    // *Which of the things I wrote down am I not sure about?* It is the whole
    // point of the axis, and for a long time it could only be answered one
    // claim at a time: `standing` was stored and served and nothing could
    // narrow by it.
    //
    // **Both halves.** The two claims nobody is sure of come back, and the one
    // the operator stated flatly stays out — without that, this passes against
    // a filter that ignores what it was given and hands back everything.
    let doubted = s.hedges().await;
    doubted
        .says("closes early on Sundays")
        .says("a Sunday visit should be earlier")
        .never_says("serves breakfast until 11am");

    // NOTE — nothing asks a session to weigh `standing` before repeating a
    // claim. Moving the hedge out of prose made it parseable, and now
    // askable; nothing yet directs a session to ask.

    s.wrap("asked which claims were reliable, and the answer separated them")
        .await;

    // ── session 5 · the operator confirms the one they were unsure of ───────
    let s = story.session().await;

    // Confirmation moves `standing` and leaves `provenance` alone: they backed
    // this claim the day they said it, and what changed is that they are now
    // sure. Promotion is gated on exactly this, so nothing can promote itself.
    s.confirm(&hedged).await;

    // The confirmation lands on their claim and leaves the session's guess
    // alone. A lone `settled` anywhere on the page would pass without this
    // being bound to an address.
    let after = s.recall("place:moes").await;
    after
        .claim(&hedged)
        .says("closes early on Sundays")
        .says("\"provenance\":\"testimony\"")
        .says("\"standing\":\"settled\"");
    after
        .claim(&derived)
        .says("\"provenance\":\"inference\"")
        .says("\"standing\":\"open\"");

    // GAP — the derived claim points at a parent that changed underneath it:
    // the claim it was drawn from was a hypothesis then and is settled now.
    // The link is intact and says nothing about that. It is harmless here,
    // because a promotion only makes the conclusion safer — and it would read
    // exactly the same if the parent had been refuted instead.
    //   s.derived_from(&derived).says_parent_moved().await;
    s.has_no_verb("restate", &["update_fact", "recall"]).await;
    s.recall("place:moes")
        .await
        .says(&format!("\"address\":\"{derived}\""))
        .says(&format!("\"derived_from\":\"{hedged}\""));

    s.wrap("confirmed the hedged claim; it is settled and the guess beside it is not")
        .await;

    // ── session 6 · "no, scratch that, I was wrong" ─────────────────────────
    let s = story.session().await;

    // A week on, the operator went on a Sunday and it was open. A claim they
    // settled is rewritten to the negative truth, which stays settled and
    // stays theirs: what is true now is that it does not close early.
    s.correct(
        &hedged,
        "does NOT close early on Sundays — the operator went and it was open",
    )
    .await;
    let walked_back = s.recall("place:moes").await;
    walked_back
        .claim(&hedged)
        .says("does NOT close early")
        .says("\"standing\":\"settled\"")
        .never_says("closes early on Sundays");

    // **Asking the read for how the hedge moved is a question the surface now
    // takes**, and the answer is that nothing was ever written under that
    // name. Every write of a KEY is kept, so a key's whole history comes back
    // on this read — and `standing` is not a key. It is a column of the claim,
    // rewritten in place by the confirmation and again by the walk-back, and a
    // rewrite leaves nothing behind.
    let moved = s
        .shape(
            "every time the hedge's standing moved",
            json!({"subject": "place:moes", "history": "standing"}),
        )
        .await;
    moved.says("\"key\":\"standing\"").says("\"count\":0");

    // **The claim's OWN writes are kept, and the same read reaches them** —
    // by the record's address rather than by a key. That is where `standing`,
    // `content` and `status` live, so this is the trace the key history above
    // cannot carry: the hedge as it was captured, the confirmation, and the
    // walk-back, oldest first.
    let rewritten = s
        .shape(
            "every time the hedged claim itself was written",
            json!({"subject": "place:moes", "history_record": hedged}),
        )
        .await;
    rewritten
        .number("/objects/0/record_history/count", 3)
        .says("\"closes early on Sundays\"")
        .says("\"nth\":1")
        .says("\"nth\":3");

    // ⚠️ **And a claim nobody rewrote reads as one write**, which is what makes
    // the three above mean something: without this, every claim on the page
    // would look like one somebody had corrected.
    s.shape(
        "every time the settled breakfast claim was written",
        json!({"subject": "place:leftorium", "history_record": breakfast}),
    )
    .await
    .number("/objects/0/record_history/count", 1);

    // GAP — the derived claim below still rests on a parent that has since
    // reversed, and nothing on it says so.
    walked_back
        .claim(&derived)
        .says("a Sunday visit should be earlier")
        .says("\"standing\":\"open\"");

    s.wrap("the hedge was confirmed, then reversed, and reads as neither")
        .await;

    story.finish().await;
}
