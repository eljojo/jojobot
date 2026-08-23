//! "If I write this down, does it wipe out what I wrote before?"
//!
//! **The measured failure this story is the reachability check for:** an agent
//! declined to record a second account of an event because it believed the
//! write would overwrite the first. It would not have. The account was lost
//! permanently, and the cost surfaced months later as a confident summary that
//! knew only one side of it.
//!
//! Nothing on the surface said what a write does to what is already there, and
//! the descriptions that could have said it are read once and discounted. **A
//! receipt is the one channel that reaches a session in the middle of its
//! run**, so the answer to a write now states two things it never did: what
//! stands after the call, and any value the store did not keep as it was sent.
//!
//! **Two lines, and the second is why the first can be believed.** `capture`
//! appends and `update_fact` rewrites in place, so a postcondition that said
//! *nothing was changed* on both would be false on one of them.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn two_accounts_of_one_evening_and_the_receipt_that_says_both_stand() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("person:barney-gumble", "Barney Gumble").await;

    // ── ① the first account ─────────────────────────────────────────────────
    let first = s
        .call(
            "capture",
            json!({
                "subject": "person:barney-gumble",
                "content": "said the jukebox was working when he left",
                "provenance": "testimony",
                "date": "2026-08-14",
            }),
        )
        .await;
    first.says("postcondition");

    // ── ② the account that contradicts it ───────────────────────────────────
    //
    // ⭐ **This is the write an agent talked itself out of.** The receipt has
    // to say, in the answer to this very call, that the first account is still
    // there.
    let second = s
        .call(
            "capture",
            json!({
                "subject": "person:barney-gumble",
                "content": "said the jukebox had been dead all week",
                "provenance": "testimony",
                "date": "2026-08-14",
            }),
        )
        .await;
    let standing = second.json()["postcondition"]
        .as_str()
        .expect("a write states what now stands")
        .to_string();
    assert!(
        standing.contains('2'),
        "the receipt has to say both accounts stand, in the answer to the write itself: \
         {standing}",
    );

    // …and the store agrees with the receipt, which is the half a line about
    // itself cannot prove.
    let both = s.recall("person:barney-gumble").await;
    both.says("was working when he left");
    both.says("had been dead all week");

    // ── ③ the verb that DOES replace, and says so ───────────────────────────
    //
    // The postcondition is computed per write, so the edit verb's line is a
    // different sentence. A build that answered both with one constant would
    // be promising, on this call, something it did not do.
    let address = s
        .fact("person:barney-gumble", "thought the lease was paid in June")
        .await;
    let corrected = s
        .call(
            "update_fact",
            json!({
                "address": address,
                "content": "the lease was NOT paid in June — confirmed",
            }),
        )
        .await;
    let replaced = corrected.json()["postcondition"]
        .as_str()
        .expect("an edit states what now stands")
        .to_string();
    assert!(
        replaced.contains(&address),
        "the edit's line names the record it rewrote: {replaced}",
    );
    assert_ne!(
        replaced, standing,
        "the two verbs answer with one constant, so the line promises on an edit what only an \
         append can keep",
    );

    // ── ④ a value the store did not keep as it was sent ─────────────────────
    //
    // A check-in merges the schedule jojobot computed into the caller's own
    // record, so the record it builds is a derivation whatever the caller
    // declared. **The substitution is correct and the silence was not**: an
    // agent that has met one unannounced substitution has to price every other
    // write at its worst case.
    s.add("thing:jukebox", "The Jukebox").await;
    s.add_under("thing:jukebox", "rhythm:descale", "Descale")
        .await;
    s.call(
        "capture",
        json!({
            "subject": "rhythm:descale",
            "content": "set the loop up",
            "date": "2026-08-01",
            "fields": {
                "name": "Descale",
                "last_check_in": "2026-08-01",
                "cadence_days": "7",
                "counts_from": "2026-08-01",
                "advances_from": "check_in_date",
            },
        }),
    )
    .await;

    let checked_in = s
        .call(
            "capture",
            json!({
                "subject": "rhythm:descale",
                "content": "did it this morning",
                "provenance": "testimony",
                "check_in": "ran",
                "date": "2026-08-10",
            }),
        )
        .await;
    let receipt = checked_in.json();
    let named = &receipt["delta"][0];
    assert_eq!(named["field"], "provenance", "{receipt}");
    assert_eq!(named["sent"], "testimony", "{receipt}");
    assert_eq!(
        named["stored"], "inference",
        "the caller's own value was replaced and the receipt has to name it: {receipt}",
    );

    // ⭐ **Silent when nothing differs.** The same verb, keeping everything it
    // was sent, carries no delta — a line that printed on every write is one a
    // reader learns to skip, and it would be gone from view on the write that
    // needed it.
    let kept = s
        .call(
            "capture",
            json!({
                "subject": "person:barney-gumble",
                "content": "says the jukebox sounds better now",
                "provenance": "testimony",
                "date": "2026-08-11",
            }),
        )
        .await;
    assert!(
        kept.json()["delta"].is_null(),
        "nothing was replaced and the receipt says so by saying nothing: {}",
        kept.json(),
    );

    story.finish().await;
}
