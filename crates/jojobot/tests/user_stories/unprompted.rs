//! "I'm talking to you in a browser, not in my repo. You've got jojobot and
//! nothing else — no files, no notes, no me explaining it first. Get it right
//! anyway."
//!
//! Every beat asks what the session had to know already. With nothing local
//! there are four places that knowledge can come from: the orientation essay,
//! the bot's own charter and rules, the tool descriptions, and a person pasting
//! the procedure into the chat — and the last one is a gap by definition.
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for.

use super::dsl::Story;

#[tokio::test]
async fn a_session_records_a_claim_with_its_source_unprompted() {
    let story = Story::begin("bot:otto").await;

    // ── beat 1 · a browser conversation, nothing local ──────────────────────
    let (booted, s) = story.full_boot().await;

    let essay = booted["orientation"]
        .as_str()
        .expect("the boot serves its orientation");

    // The boot names the verbs the rest of this story cannot be done without.
    // Pinned on identifiers, never on phrasing: a rename is a real change to
    // the surface and a rewording is not.
    for verb in ["add_entity", "capture", "recall"] {
        assert!(
            essay.contains(verb),
            "the boot must name {verb}, which beat 3 or 4 cannot be done without: {essay}"
        );
    }

    // ── beat 2 · a claim, and where it came from, in one sentence ───────────
    //
    // "Remember the diner does a Thursday special — I read it on their posted
    // menu."

    // The subject is a place jojobot does not know yet, and the essay teaches
    // this half: create it, then write, two deliberate steps.
    s.add("place:leftorium", "The Leftorium Diner").await;

    // It also teaches the other half — the four edge shapes, and a worked case
    // of hanging one on a claim.
    for shape in ["location", "membership", "attendance", "about"] {
        assert!(
            essay.contains(shape),
            "the boot must name the {shape} edge shape: {essay}"
        );
    }

    // Naming `about` proves only that it is in the list of four. These two
    // identifiers appear nowhere but inside the worked examples — the source
    // entity the write example creates, and the field the read example names.
    // Edit either example out and this goes red; reword everything around them
    // and it does not.
    assert!(
        essay.contains("thing:leftorium-menu"),
        "the write example that records a claim's source is gone: {essay}"
    );
    assert!(
        essay.contains("derived_from"),
        "the read example that walks back to a source is gone: {essay}"
    );

    // ── beat 3 · so the session writes it the way the essay showed ──────────
    s.add("thing:leftorium-menu", "The Leftorium's Posted Menu")
        .await;
    s.fact_about(
        "place:leftorium",
        "does a Thursday special, per its posted menu",
        "about",
        "thing:leftorium-menu",
    )
    .await;

    // GAP — the source here is a `thing`, which fits a posted menu. The sources
    // the operator cites most often are a link they saved, an entry on their
    // calendar, a card on their board — each already an object somewhere, with
    // an identity of its own, and the only way to point at one is to stand up a
    // `thing` that stands in for it. That is a second record of something
    // jojobot could have referenced.
    //   s.add_from_links("the diner's menu, saved last week").await;
    //   s.add_from_calendar("the tasting on the 3rd").await;

    // GAP — and nothing said the claim needed a source at all. The operator
    // volunteered one; had they not, nothing in the boot would have prompted
    // the session to ask.

    s.wrap("recorded the special and what it came from").await;

    // ── beat 4 · a later session is asked where it came from ────────────────
    let s = story.session().await;

    // The source rides on the claim, so answering needs no memory of the
    // conversation that recorded it.
    s.recall("place:leftorium")
        .await
        .says("does a Thursday special")
        .says("\"object\":\"thing:leftorium-menu\"");

    s.wrap("answered where the claim came from").await;

    // ── beat 5 · a claim with nothing behind it ─────────────────────────────
    let s = story.session().await;

    // "I think the other place does one too." Stated flat, with nothing behind
    // it. The essay teaches that inference is the honest provenance when
    // nothing backs a claim, and the record shows the absence rather than
    // hiding it.
    assert!(
        essay.contains("inference") && essay.contains("testimony"),
        "the boot must name both provenances, which beat 5 turns on: {essay}"
    );
    s.add("place:moes", "Moe's").await;
    s.guess("place:moes", "may do a Thursday special too").await;

    s.recall("place:moes")
        .await
        .says("may do a Thursday special")
        .says("\"provenance\":\"inference\"")
        .says("\"edge\":null");

    s.wrap("recorded a claim with nothing behind it, visibly")
        .await;

    // ── beat 6 · "I think it closes early on Sundays, but don't hold me to
    //             that" ─────────────────────────────────────────────────────
    let s = story.session().await;

    // The operator backs this claim and is not sure of it — two answers, and
    // `standing` is the field that carries the second one.
    let hedge = s.hedged("place:moes", "closes early on Sundays").await;
    s.recall("place:moes")
        .await
        .claim(&hedge)
        .says("\"provenance\":\"testimony\"")
        .says("\"standing\":\"open\"");

    // GAP — and a session with only the boot would not have written it that
    // way. The essay teaches the provenance pair and never teaches the second
    // field, so the shape it offers for "nobody is sure of this" is
    // `inference` — which answers who backs the claim, and answers it wrongly:
    // the operator did. Recording the hedge honestly needs a field the
    // orientation never mentions.
    //
    // The needle is `settled`, not `standing`: the field's own value token,
    // which the essay has no other use for, where the field's name is also
    // ordinary English and appears in three unrelated sentences.
    assert!(
        !essay.contains("settled"),
        "the essay now teaches the second field — flip this assertion, the gap is closed: {essay}"
    );

    s.wrap("recorded the hedge, using a field the boot never taught")
        .await;

    // ── beat 7 · the procedure it did not know to ask for ───────────────────
    //
    // The boot lists every shipped skill by name and when-to-use, so a browser
    // session can see what procedures exist without paying for their bodies,
    // and fetch one through the same door.
    let listed = booted["skills"]
        .as_array()
        .expect("the boot names the skills that exist");
    assert!(
        !listed.is_empty(),
        "a boot with no skills leaves nothing to fetch: {booted}"
    );
    assert!(
        listed.iter().all(|s| s.get("body").is_none()),
        "the index ships names and when-to-use, never bodies: {booted}"
    );

    let fetched = story.skill("recommend").await;
    assert!(
        fetched["skill"]["body"]
            .as_str()
            .is_some_and(|b| !b.is_empty()),
        "fetching a skill by name returns its body: {fetched}"
    );

    // GAP — nothing decides when one applies, which is deliberate: jojobot
    // performs no inference and the session chooses. What follows from it is
    // that a session which never reads the index never learns a procedure
    // exists, and no beat above was told to look. The one thing standing
    // between this conversation and the shipped method is the session
    // happening to ask.

    story.finish().await;
}
