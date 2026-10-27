//! "I have filed the same person twice, and neither page knows about the
//! other."
//!
//! A duplicate that gets past the write guard splits one thing across two
//! handles. Every read of either handle answers with half the file and reports
//! it as whole, and the split is invisible from each side, because each looks
//! like a complete record of itself. **With a repair the guard only has to be
//! good. Without one it has to be perfect, and nothing is.**
//!
//! `// NOTE —` marks something a SESSION did not do: jojobot answers nothing
//! about it, so no assertion can hold it and none pretends to.

use super::dsl::Story;
use serde_json::json;

#[tokio::test]
async fn one_thing_filed_twice_can_be_put_back_together() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · the same person, filed twice ────────────────────────────
    let s = story.session().await;

    s.add("person:nelson", "Nelson").await;
    s.fact("person:nelson", "plays the bass").await;
    s.wrap("filed him once").await;

    // ⭐ **A later session, not knowing, files him again — and the guard is
    // GOOD: it catches the near-slug and refuses.** So the duplicate only
    // exists because that session read the candidates and insisted the two were
    // different people. **That is how a duplicate gets past a guard that is
    // only good, and it is why the repair has to exist.**
    let s = story.session().await;
    let refusal = s
        .refused(
            "add_entity",
            json!({
                "kind": "person", "handle": "nelson-2", "name": "Nelson M",
                "source": "user-named",
            }),
        )
        .await;
    refusal.says("person:nelson");
    s.call(
        "add_entity",
        json!({
            "kind": "person", "handle": "nelson-2", "name": "Nelson M",
            "source": "user-named",
            "override_token": refusal.advised("override_token"),
        }),
    )
    .await
    .says("person:nelson-2");
    let second = s.fact("person:nelson-2", "reads the sleeve notes").await;

    // 🚨 **The fault, asserted before the repair.** Each handle answers with
    // its own half and neither says the other half exists. Without this the
    // whole answer below proves nothing.
    s.recall("person:nelson")
        .await
        .says("plays the bass")
        .never_says("reads the sleeve notes");
    s.recall("person:nelson-2")
        .await
        .says("reads the sleeve notes")
        .never_says("plays the bass");

    s.wrap("and again, under a second handle").await;

    // ── session 2 · somebody notices and repairs it ─────────────────────────
    let s = story.session().await;

    let done = s
        .merge_entities(
            "person:nelson-2",
            "person:nelson",
            "one person, filed twice",
            None,
        )
        .await;

    // ⚠️ **The answer says the claims changed address**, which is the thing a
    // caller holding one of those addresses has to know.
    assert_eq!(
        done["claims_moved"], 1,
        "the answer did not say what moved: {done}",
    );
    assert_eq!(done["addresses_changed"], true, "{done}");
    assert_eq!(
        done["now_resolves_to"], "person:nelson",
        "the answer did not say where the spare handle now sends a reader: {done}",
    );

    // ⭐ **Whole from the survivor, in one read.** This is the whole point.
    s.recall("person:nelson")
        .await
        .says("plays the bass")
        .says("reads the sleeve notes")
        // And the account of the repair is readable from the thing it happened
        // to, long after anybody remembers doing it.
        .says("one person, filed twice");

    // **The address a caller was holding names a claim that now sits under a
    // new number on the survivor.**
    assert!(
        second.starts_with("person:nelson-2#"),
        "the fixture did not capture the second claim under the spare handle: {second}",
    );

    // **The spare handle goes on answering, and says where it went.** A read
    // of it is never an empty thing: it names the survivor and holds nothing
    // of its own, so a session that only has the old handle is told to read the
    // survivor instead of concluding nobody wrote anything about it.
    s.recall("person:nelson-2")
        .await
        .says("\"status\":\"merged\"")
        .says("\"merged_into\":\"person:nelson\"")
        .never_says("plays the bass")
        .never_says("reads the sleeve notes");

    // ⭐ **A caller holding the stale ADDRESS itself — the second claim's own
    // `person:nelson-2#…`, not just the spare handle — still names the claim.**
    // The merge kept an event with the map from the old number to the new one,
    // so the address acts on the claim where it is now: the rule that a
    // session's own earlier testimony is not rewritten in place applies to it
    // there, under the survivor's address, and a retraction lands.
    s.refused(
        "update_fact",
        json!({"address": second, "content": "trying to correct a claim that already moved"}),
    )
    .await
    .says("person:nelson#f");
    s.call(
        "retract",
        json!({"address": second, "reason": "taking back a claim through the address it had"}),
    )
    .await
    .says("person:nelson#f");

    s.wrap("one person again, and the repair is on the record")
        .await;

    story.finish().await;
}

/// **An address held from before a merge still reaches its claim, and the verb
/// that merges does not tell a caller otherwise.**
///
/// The merge moves nothing: the survivor serves the duplicate's claims under
/// new numbers, and an address a caller kept names the claim where it is now.
/// The case reads the claim back through the held address, and checks that the
/// merge description still names the verb that address works with.
#[tokio::test]
async fn an_address_held_from_before_a_merge_still_reaches_its_claim() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("person:nelson", "Nelson").await;
    s.add("person:milhouse", "Milhouse").await;
    let held = s.fact("person:milhouse", "reads the sleeve notes").await;
    assert!(held.starts_with("person:milhouse#"), "{held}");

    s.merge_entities("person:milhouse", "person:nelson", "one person", None)
        .await;

    // The held address names the claim: its history reads through it, and the
    // write in it is the one that was made.
    let history = s
        .call("recall", json!({"history_record": held}))
        .await
        .json();
    let writes = &history["objects"][0]["record_history"]["writes"];
    assert_eq!(
        writes[0]["content"], "reads the sleeve notes",
        "the held address reads the claim it named: {history}"
    );

    // The verb that did the merge still names a verb the held address works with.
    let said = s.description("merge_entities").await;
    assert!(
        said.contains("update_fact"),
        "and it names a verb the held address still works with: {said}"
    );

    story.finish().await;
}

/// **A merge reaches a pointer wearing a rename's OLD name too.** A rename
/// rewrites nothing: an edge drawn at a handle keeps that handle, whichever
/// one was current the day it was drawn. `merge` already follows an edge
/// drawn at the folded side's CURRENT name; this proves it also follows one
/// drawn at a name that stopped being current before the fold ever happened —
/// otherwise the pointer dead-ends one hop short, at a row that only forwards.
///
/// **Paired, and not optional**: the second claim below is drawn AFTER the
/// typo is corrected, already at the current name. A fix that only chased
/// the former handle and stopped sweeping the direct case would pass the
/// first claim and lose this one.
#[tokio::test]
async fn a_merge_follows_a_pointer_drawn_before_the_folded_sides_own_rename() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("person:ned-flander", "Ned Flanders").await;
    s.add("person:homer", "Homer").await;

    // Homer's own claim, drawn at the typo — before anybody caught it.
    s.fact_about(
        "person:homer",
        "borrowed a ladder from the neighbor",
        "connection",
        "person:ned-flander",
    )
    .await;

    // The typo is corrected.
    s.call(
        "rename_entity",
        json!({"handle": "person:ned-flander", "to": "person:ned-flanders"}),
    )
    .await;

    // A second claim, drawn after the correction — already at the current
    // name.
    s.fact_about(
        "person:homer",
        "returned the ladder to the neighbor",
        "connection",
        "person:ned-flanders",
    )
    .await;

    // Ned turns out to have been filed once before, independently, months
    // earlier — the guard catches the same name and is overridden, the same
    // way the first duplicate above got past it.
    s.add_over_the_screen("person:flanders-original", "Ned Flanders")
        .await;
    // **The merge is recorded on the day the duplicate was actually made**,
    // months before today — the account should read as an old repair of an
    // old mistake, not as something that happened just now.
    let merged = s
        .merge_entities(
            "person:ned-flanders",
            "person:flanders-original",
            "one neighbor, filed under the typo and again months earlier",
            Some("2026-01-15"),
        )
        .await;
    assert_eq!(
        merged["record"]["recorded_at"], "2026-01-15",
        "the merge did not land on the day it was told to: {merged}",
    );

    // Both of Homer's claims — one drawn before the rename, one after — now
    // point at the survivor. Counted, not merely present: a fix that
    // repointed only one would still say "person:flanders-original" once.
    let homer = s.recall("person:homer").await;
    assert_eq!(
        homer
            .raw()
            .matches("\"object\":\"person:flanders-original\"")
            .count(),
        2,
        "not both of Homer's claims followed the fold to the survivor: {}",
        homer.raw(),
    );
    homer
        .never_says("\"object\":\"person:ned-flander\"")
        .never_says("\"object\":\"person:ned-flanders\"");

    s.wrap("found the neighbor was filed three times over, and put it right")
        .await;

    story.finish().await;
}

/// **The correction lands on a spelling that resembles somebody else** —
/// `rename_entity`'s destination faces the same near-miss guard `add_entity`
/// does, and the round trip is the same shape: blocked with candidates,
/// checked, and lifted with the token the refusal minted.
///
/// **The negative the override rests on**: Barney Gumble's own record is
/// never touched. The guard is asking whether the DESTINATION resembles
/// somebody real, not moving anything until it is answered.
#[tokio::test]
async fn a_rename_onto_a_spelling_that_resembles_someone_else_is_confirmed_and_lands() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("person:barney-gumble", "Barney Gumble").await;
    s.fact("person:barney-gumble", "drinks at Moe's").await;

    // Filed under a shorthand at the time — now corrected to what was
    // actually meant, which happens to closely resemble Barney's own
    // handle.
    s.add("person:barn", "Barn").await;
    s.fact("person:barn", "fixes the tractor").await;

    let refusal = s
        .refused(
            "rename_entity",
            json!({"handle": "person:barn", "to": "person:barney-gumbel"}),
        )
        .await;
    refusal.says("person:barney-gumble");

    s.call(
        "rename_entity",
        json!({
            "handle": "person:barn", "to": "person:barney-gumbel",
            "override_token": refusal.advised("override_token"),
        }),
    )
    .await
    .says("person:barney-gumbel");

    // **The positive**: the corrected handle now carries what was filed
    // under the shorthand.
    s.recall("person:barney-gumbel")
        .await
        .says("fixes the tractor");

    // **The negative**: the entity the guard flagged as a resemblance was
    // never moved, retyped or touched — it is exactly where it was.
    s.recall("person:barney-gumble")
        .await
        .says("drinks at Moe's")
        .never_says("fixes the tractor");

    s.wrap("confirmed the two were different people and pushed the correction through")
        .await;

    story.finish().await;
}

/// **A dated series is the ordinary shape of a recurring thing.** Rehearsals
/// on three different days are three records under one parent, and their
/// handles and names are one edit apart in the digits of the day. The guard
/// reads a full date as the difference between them, so none is refused.
#[tokio::test]
async fn three_dated_visits_filed_under_one_parent_are_not_refused() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("topic:stunt-rehearsals", "Stunt rehearsals").await;
    s.add_under(
        "topic:stunt-rehearsals",
        "event:stunt-rehearsal-2026-10-05",
        "Stunt rehearsal 2026-10-05",
    )
    .await;
    s.add_under(
        "topic:stunt-rehearsals",
        "event:stunt-rehearsal-2026-10-08",
        "Stunt rehearsal 2026-10-08",
    )
    .await;
    s.add_under(
        "topic:stunt-rehearsals",
        "event:stunt-rehearsal-2026-11-02",
        "Stunt rehearsal 2026-11-02",
    )
    .await;

    // **Each one landed as its own record**, not folded into a sibling.
    for (handle, day) in [
        ("event:stunt-rehearsal-2026-10-05", "2026-10-05"),
        ("event:stunt-rehearsal-2026-10-08", "2026-10-08"),
        ("event:stunt-rehearsal-2026-11-02", "2026-11-02"),
    ] {
        s.recall(handle).await.says(day);
    }

    s.wrap("filed three rehearsals as three records").await;
    story.finish().await;
}

/// **A date sets two entries apart only under one parent.** Beside the dated
/// series, the shapes that are still one record seen twice stay refused: the
/// same day again, and a dated handle filed under another parent. (A digit
/// that is no date is a number, and the numbered series below covers it.)
#[tokio::test]
async fn a_different_date_sets_two_entries_apart_only_under_the_same_parent() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("topic:stunt-rehearsals", "Stunt rehearsals").await;
    s.add("topic:stunt-schedule", "Stunt schedule").await;
    s.add_under(
        "topic:stunt-rehearsals",
        "event:stunt-rehearsal-2026-10-05",
        "Stunt rehearsal 2026-10-05",
    )
    .await;

    // The same day again, with a suffix: the same-day collision stays.
    s.refused(
        "add_entity",
        json!({
            "kind": "event", "handle": "stunt-rehearsal-2026-10-05-final",
            "name": "Stunt rehearsal 2026-10-05 final",
            "source": "user-named", "parent": "topic:stunt-rehearsals",
        }),
    )
    .await
    .says("event:stunt-rehearsal-2026-10-05");

    // A different day, but under another parent: not the same series.
    s.refused(
        "add_entity",
        json!({
            "kind": "event", "handle": "stunt-rehearsal-2026-10-09",
            "name": "Stunt rehearsal 2026-10-09",
            "source": "user-named", "parent": "topic:stunt-schedule",
        }),
    )
    .await
    .says("event:stunt-rehearsal-2026-10-05");

    s.wrap("the date exemption left the other near-misses refused")
        .await;
    story.finish().await;
}

/// **A numbered series is the ordinary shape of a queue.** Work items q-1001,
/// q-1002 and q-1010 under one parent are one edit apart (or two) in the digits
/// of the number, and each is made in one call with no override. Two made at
/// the same moment both land. Their display names are task titles that differ
/// by the same number only ("Sand deck panel 1001"), so the name channels
/// answer the same question as the handle channel.
#[tokio::test]
async fn numbered_siblings_under_one_parent_are_not_refused() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let t = story.session().await;

    s.add("work:alpha-queue", "Alpha queue").await;
    s.add_under("work:alpha-queue", "work:q-1001", "Sand deck panel 1001")
        .await;
    s.add_under("work:alpha-queue", "work:q-1002", "Sand deck panel 1002")
        .await;
    s.add_under("work:alpha-queue", "work:q-1010", "Sand deck panel 1010")
        .await;

    // Two calls at the same moment both land.
    tokio::join!(
        s.add_under("work:alpha-queue", "work:q-1020", "Sand deck panel 1020"),
        t.add_under("work:alpha-queue", "work:q-1021", "Sand deck panel 1021"),
    );

    for handle in [
        "work:q-1001",
        "work:q-1002",
        "work:q-1010",
        "work:q-1020",
        "work:q-1021",
    ] {
        s.recall(handle).await.says(handle);
    }

    s.wrap("filed a numbered series as separate records").await;
    story.finish().await;
}

/// **The numbered exemption is a different number under the same parent and
/// nothing wider.** A typo-shaped neighbour of a numbered handle is still
/// screened, and so is the same number under another parent.
#[tokio::test]
async fn only_a_different_number_under_the_same_parent_sets_numbered_handles_apart() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("work:alpha-queue", "Alpha queue").await;
    s.add("work:beta-queue", "Beta queue").await;
    s.add_under("work:alpha-queue", "work:q-1001", "Sand deck panel 1001")
        .await;

    // A neighbour one letter away, not one number away.
    s.refused(
        "add_entity",
        json!({
            "kind": "work", "handle": "q-1001x", "name": "Wire the shed lights",
            "source": "user-named", "parent": "work:alpha-queue",
        }),
    )
    .await
    .says("work:q-1001");

    // The same on the name channel: the handle is a different number, the
    // name is a typo of a name that is filed.
    s.refused(
        "add_entity",
        json!({
            "kind": "work", "handle": "q-1002", "name": "Sand deck pnael 1001",
            "source": "user-named", "parent": "work:alpha-queue",
        }),
    )
    .await
    .says("work:q-1001");

    // The same series shape under another parent: no series, as today.
    s.refused(
        "add_entity",
        json!({
            "kind": "work", "handle": "q-1002", "name": "Sand deck panel 1002",
            "source": "user-named", "parent": "work:beta-queue",
        }),
    )
    .await
    .says("work:q-1001");

    s.wrap("the numbered exemption left the other near-misses refused")
        .await;
    story.finish().await;
}

/// **Two numbered roots are refused, and the refusal says a series lands under
/// one parent.** The second of two entries of one series, made with no parent,
/// is a near miss of the first. The way forward names `parent`, beside the
/// override that was always offered. The series made under one parent lands
/// first time: `numbered_siblings_under_one_parent_are_not_refused` holds that.
#[tokio::test]
async fn a_numbered_root_refused_beside_another_is_told_to_make_the_series_under_one_parent() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("work:q-1001", "Queue: card 1001 on homer").await;
    let refused = s
        .refused(
            "add_entity",
            json!({
                "kind": "work", "handle": "q-1002", "name": "Queue: card 1002 on homer",
                "source": "user-named",
            }),
        )
        .await;
    refused.says("work:q-1001").says("`parent`");
    // **The first member is a root, so the same parent alone does not help**:
    // the refusal names the verb that reparents it.
    refused.says("rename_entity");
    // The override is still offered: the refusal is the same gate with a
    // longer way forward.
    refused.says("override_token");

    s.wrap("a numbered series refused at the root named its way forward")
        .await;
    story.finish().await;
}

/// An added numeric part names another member of a series, whose refusal at
/// the root still explains how to put both members under one parent.
#[tokio::test]
async fn a_numbered_root_with_an_added_part_is_told_to_make_the_series_under_one_parent() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("work:contract-deploy-9", "Deploy 9").await;
    let refused = s
        .refused(
            "add_entity",
            json!({
                "kind": "work", "handle": "contract-deploy-9-3", "name": "Deploy 9.3",
                "source": "user-named",
            }),
        )
        .await;
    refused
        .says("work:contract-deploy-9")
        .says("`parent`")
        .says("rename_entity")
        .says("override_token");
    s.wrap("an added numbered part at the root named its way forward")
        .await;
    story.finish().await;
}

/// **A pair that differs by a letter is not a series**, so its refusal does not
/// send the caller to a parent.
#[tokio::test]
async fn a_root_one_letter_from_another_is_refused_without_the_series_advice() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("work:q-1001", "Queue: card 1001 on homer").await;
    let refused = s
        .refused(
            "add_entity",
            json!({
                "kind": "work", "handle": "q-1001x", "name": "Bart",
                "source": "user-named",
            }),
        )
        .await;
    let candidates = refused.json()["candidates"].clone();
    assert!(
        candidates
            .as_array()
            .is_some_and(|all| all.iter().any(|one| one["handle"] == "work:q-1001")),
        "the refusal lists the near miss as a candidate: {candidates}"
    );
    refused.says("override_token");
    refused.never_says("`parent`");

    s.wrap("a letter apart is refused as before").await;
    story.finish().await;
}

/// "I filed the council twice, once under its own office." **A thing is not
/// folded into one of its own descendants, and the refusal says what to do
/// instead.** The folded thing's children are served under the survivor, so a
/// survivor below it would be its own parent. The way forward is read from the
/// refusal and taken: the same two things fold the other way. **Paired**: nothing
/// moved on the refusal, and the fold the other way lands.
#[tokio::test]
async fn a_thing_is_not_folded_into_its_own_descendant_and_the_refusal_names_the_way() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("org:springfield-town-council", "Town council").await;
    s.add_under(
        "org:springfield-town-council",
        "org:springfield-cyclery",
        "Cyclery",
    )
    .await;

    let refused = s
        .refused(
            "merge_entities",
            json!({"duplicate": "org:springfield-town-council",
                   "survivor": "org:springfield-cyclery",
                   "reason": "one body under two names"}),
        )
        .await;
    refused.says("rename_entity");
    // Nothing moved: the office still answers to its own parent.
    s.recall("org:springfield-cyclery")
        .await
        .says("\"parent\":\"org:springfield-town-council\"");

    // The way the refusal names, taken: the other way round.
    let merged = s
        .merge_entities(
            "org:springfield-cyclery",
            "org:springfield-town-council",
            "one body under two names",
            None,
        )
        .await;
    assert!(merged.get("claims_moved").is_some(), "{merged}");
    story.finish().await;
}
