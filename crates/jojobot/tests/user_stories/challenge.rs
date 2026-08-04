//! "You told me it was open till 2am. I went, it was shut. Where did you get
//! that — and don't tell me the same thing again next month."
//!
//! A claim can still be acted on wrongly inside a session's own reasoning,
//! which is not a failure jojobot's storage can reach. What it can be held to
//! is handing over enough to avoid it: provenance on the claim, and one
//! reachable answer after a correction.
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for.

use super::dsl::Story;

#[tokio::test]
async fn where_did_you_get_that() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · research, and a claim nobody actually said ──────────────
    let s = story.session().await;

    s.add("place:moes", "Moe's").await;

    // Testimony and inference about the same place. The operator said the
    // first. Nobody said the second — it came out of a search summary, which is
    // the door prompt injection walks through.
    s.fact(
        "place:moes",
        "a favourite of theirs, they go when the week has been long",
    )
    .await;
    let claim = s.guess("place:moes", "open until 2am on Sundays").await;

    s.wrap("one thing the operator said, one thing we worked out")
        .await;

    // ── session 2 · they went. it was shut. ─────────────────────────────────
    let s = story.session().await;

    // The claim is rewritten in place, so a reader gets one answer rather than
    // two and a judgement.
    s.correct(
        &claim,
        "closes at 6 on Sundays — they went on a Sunday and it was shut",
    )
    .await;

    // The old wording is gone, not outvoted. Each negative is paired with the
    // positive it depends on: a bare absence cannot tell a correction that
    // worked from a read that returned nothing at all.
    s.recall("place:moes")
        .await
        .says("closes at 6")
        .never_says("until 2am");
    s.find("Sundays")
        .await
        .says("place:moes")
        .never_says("until 2am");

    // And what the operator actually said is untouched by the correction.
    s.recall("place:moes").await.says("week has been long");

    // "Where did you get that" IS answerable when the claim was written with
    // its parent: `derived_from` carries an earlier claim's address and an
    // `about` edge carries the source it came off, both on the record.
    let backed = s
        .guess_from("place:moes", "probably busy on Sundays, then", &claim)
        .await;
    s.recall("place:moes").await.claim(&backed).says(&claim);

    // GAP — and nothing made the first guess carry one. Provenance says a
    // claim was derived and never that a parent is owed, so a session that
    // writes an inference with nothing behind it is refused by nothing, and
    // the claim above is unfalsifiable exactly as a fabrication would be.
    //   s.why(&claim).says("a search summary, session 1").await;
    s.recall("place:moes")
        .await
        .claim(&claim)
        .says("\"provenance\":\"inference\"")
        .says("\"derived_from\":null");

    // GAP — and nothing records that a claim was ACTED ON. A wrong guess that
    // sat unread and a wrong guess that sent somebody across town on a Sunday
    // are the same object here.
    //   s.acted_on(&claim, "they went").await;
    s.has_no_verb("acted_on", &["update_fact", "recall"]).await;

    s.wrap("corrected, and we still cannot say where it came from")
        .await;

    // ── session 3 · weeks later, a fresh session reaches the same conclusion ─
    let s = story.session().await;

    // Nothing stops it. The corrected fact states the negative truth, but a new
    // session doing new research infers the old claim again — and this time it
    // is a NEW fact, not a rewrite, so the correction it never saw does not
    // apply to it.
    let again = s.guess("place:moes", "open until 2am on Sundays").await;
    s.recall("place:moes").await.says("until 2am");

    // GAP — the corrected claim and the re-inferred one now sit side by side,
    // and nothing can say which one was already rejected. A rejection has to be
    // a durable negative that later derivations subtract, or the same
    // correction gets made forever and eventually stops being trusted.
    //   s.rejected(&claim).so_that(&again).is_blocked().await;
    s.has_no_verb("reject", &["update_fact", "retract"]).await;

    // Cleaning up by hand, which is what a session would have to do every time.
    s.correct(
        &again,
        "closes at 6 on Sundays — already established, do not re-derive",
    )
    .await;

    s.wrap("the same wrong claim, twice, from the same absence")
        .await;

    // ── session 4 · what else came from that summary? ───────────────────────
    let s = story.session().await;

    // The same search summary produced a second claim, about a different
    // place, and that one is still standing.
    s.add("place:riverbend", "Riverbend Grill").await;
    s.guess("place:riverbend", "serves food until midnight")
        .await;

    // What CAN be asked is everything nobody vouched for, across the store, in
    // one call — a question about provenance rather than about wording, so
    // both guesses come back and what the operator actually said does not.
    s.unbacked()
        .await
        .says("place:riverbend")
        .says("closes at 6 on Sundays")
        .never_says("week has been long");

    // And by where they came from, once the summary is a thing in its own
    // right. A source is an entity, the claims drawn from it point at it, and
    // asking what else it produced is one walk rather than somebody's memory.
    s.add("thing:that-search-summary", "The Search Summary")
        .await;
    s.fact_about(
        "place:riverbend",
        "listed as sourcing from a farm co-op",
        "about",
        "thing:that-search-summary",
    )
    .await;
    s.through_any("about", "thing:that-search-summary")
        .await
        .says("place:riverbend");

    // GAP — and discrediting the summary still reaches none of them. There is
    // no way to mark a source as unreliable, so each claim resting on it has
    // to be found by that walk and rewritten one at a time.
    //   s.discredit("thing:that-search-summary", "the listing was stale").await;
    s.has_no_verb("discredit", &["update_fact", "search"]).await;

    s.wrap("one bad source, two claims, and no way to reach the second")
        .await;

    story.finish().await;
}
