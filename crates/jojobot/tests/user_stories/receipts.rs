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
                "recorded_at": "2026-08-14",
            }),
        )
        .await;
    assert_eq!(first.json()["success"], true);
    assert!(
        first.json().get("postcondition").is_none(),
        "{}",
        first.json()
    );

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
                "recorded_at": "2026-08-14",
            }),
        )
        .await;
    assert_eq!(second.json()["success"], true);
    assert_ne!(first.json()["address"], second.json()["address"]);

    // …and the store agrees with the receipt, which is the half a line about
    // itself cannot prove.
    let both = s.recall("person:barney-gumble").await;
    both.says("was working when he left");
    both.says("had been dead all week");

    // ── ③ the verb that DOES replace, and says so ───────────────────────────
    //
    // A capture makes an address; an edit keeps the address it corrects.
    // The store read below proves the operation behind each receipt.
    let address = s
        .fact("person:barney-gumble", "thought the lease was paid in June")
        .await;
    let corrected = s
        .call(
            "update_fact",
            json!({
                "address": address,
                "content": "the lease was NOT paid in June — confirmed",
                "provenance": "testimony",
            }),
        )
        .await;
    assert_eq!(corrected.json()["success"], true);
    assert_eq!(corrected.json()["address"], address);

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
            "recorded_at": "2026-08-01",
            "sets": {
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
                "recorded_at": "2026-08-10",
            }),
        )
        .await;
    let receipt = checked_in.json();
    assert_eq!(receipt["derived"]["provenance"], "inference", "{receipt}");
    assert!(
        receipt.get("delta").is_none(),
        "the sent value is not echoed: {receipt}"
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
                "recorded_at": "2026-08-11",
            }),
        )
        .await;
    assert!(
        kept.json()["delta"].is_null(),
        "nothing was replaced and the receipt says so by saying nothing: {}",
        kept.json(),
    );

    // ── ⑤ the same courtesy on the verbs that were silent about it ──────────
    //
    // A bare handle is read as a person. `capture` announced that on its
    // subject and the entity verbs did not, so one conversion was documented on
    // one verb and invisible on two others — which is the whole defect one
    // costume over: a caller that has to diff its own call against the answer
    // learns to distrust every write.
    let named = s
        .call(
            "add_entity",
            json!({
                "kind": "thing", "handle": "record-crate", "name": "Record Crate",
                "source": "user-named", "parent": "barney-gumble",
            }),
        )
        .await;
    let receipt = named.json();
    assert!(receipt.get("parent").is_none(), "{receipt}");
    assert_eq!(
        receipt["derived"]["parent"], "person:barney-gumble",
        "the handle was read and the receipt has to say how: {receipt}",
    );
    // No reason, and that is honest: reading a bare handle as a person is what
    // the argument means, where a check-in's substitution has a cause behind it
    // worth a sentence.
    assert!(
        receipt.get("warnings").is_none(),
        "a difference with nothing to explain carries no explanation: {receipt}",
    );

    story.finish().await;
}

/// **A receipt says which keys reached the thing and which stayed on the
/// record.** Six keys describe their own claim and never fold onto the thing, so
/// a receipt that said every key a record carries had "moved" told a bot that
/// starred a rule that the star was now the bot's. The data was right and the
/// sentence was wrong. The receipt now names the keys that moved to the subject
/// apart from the keys that stayed, each in its own place.
#[tokio::test]
async fn a_receipt_says_a_mark_stayed_on_the_record_and_an_ordinary_key_moved_to_the_thing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;

    let both = s
        .call(
            "capture",
            json!({
                "subject": "bot:omega", "content": "keep the kettle descaled",
                "provenance": "testimony",
                "fields": {"starred": "true"}, "sets": {"one_liner": "keeps the kettle descaled"},
            }),
        )
        .await
        .json();
    assert!(both.get("postcondition").is_none(), "{both}");
    assert_eq!(both["reached_the_thing"], json!(["one_liner"]), "{both}");
    assert_eq!(both["stayed_on_the_record"], json!(["starred"]), "{both}");

    // A capture carrying only a mark moves nothing, and says nothing moved.
    let only_a_mark = s
        .call(
            "capture",
            json!({
                "subject": "bot:omega", "content": "keep the floor swept",
                "provenance": "testimony", "fields": {"starred": "true"},
            }),
        )
        .await
        .json();
    assert_eq!(only_a_mark["stayed_on_the_record"], json!(["starred"]));
    assert_eq!(only_a_mark["reached_the_thing"], json!([]));

    // A capture carrying only an ordinary key says it moved, as it always did.
    let only_ordinary = s
        .call(
            "capture",
            json!({
                "subject": "bot:omega", "content": "the kettle is a blue one",
                "provenance": "testimony", "sets": {"colour": "blue"},
            }),
        )
        .await
        .json();
    assert_eq!(only_ordinary["reached_the_thing"], json!(["colour"]));

    // The same key sent as the claim's own field stays on the claim, and the
    // receipt lists it there and not among the keys that reached the bot.
    let as_own_field = s
        .call(
            "capture",
            json!({
                "subject": "bot:omega", "content": "the kettle is a green one",
                "provenance": "testimony", "fields": {"colour": "green"},
            }),
        )
        .await
        .json();
    assert_eq!(
        as_own_field["stayed_on_the_record"],
        json!(["colour"]),
        "{as_own_field}"
    );
    assert_eq!(
        as_own_field["reached_the_thing"],
        json!([]),
        "{as_own_field}"
    );

    s.wrap("wrote a mark and an ordinary key").await;
    story.finish().await;
}

/// **Taking a record back says which keys the thing lost.** Archiving a record
/// and retracting one both take its writes off the thing. A key the record set
/// alone is gone, a key an earlier record also set goes back to that record's
/// value, and the receipt names the first kind in `left_the_thing`, so a bot
/// does not have to read the thing before and after to learn what it cost.
#[tokio::test]
async fn taking_a_record_back_says_which_keys_the_thing_lost() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:the-fern", "The fern").await;
    let set = |content: &str, sets: serde_json::Value| {
        let call = json!({
            "subject": "thing:the-fern", "content": content,
            "provenance": "testimony", "sets": sets,
        });
        let s = &s;
        async move { s.call("capture", call).await.json() }
    };
    set("measured", json!({"height": "short", "leaves": "few"})).await;
    let second = set("measured again", json!({"height": "tall", "pot": "clay"})).await;

    let archived = s
        .call(
            "update_fact",
            json!({"address": second["address"], "status": "archived"}),
        )
        .await
        .json();
    assert_eq!(archived["left_the_thing"], json!(["pot"]), "{archived}");
    let thing = s.recall("thing:the-fern").await.json()["objects"][0].clone();
    assert_eq!(thing["fields"]["height"], "short", "{thing}");
    assert!(thing["fields"]["pot"].is_null(), "{thing}");

    let third = set("repotted", json!({"pot": "glass"})).await;
    let retracted = s
        .call(
            "retract",
            json!({"address": third["address"], "reason": "that was another plant"}),
        )
        .await
        .json();
    assert_eq!(retracted["left_the_thing"], json!(["pot"]), "{retracted}");

    story.finish().await;
}

/// **An archive with keys beside it reaches nothing.** A record written archived
/// is off the thing, so keys sent in `sets` with the archive stay on the record:
/// the receipt names none of them under `reached_the_thing`, names them under
/// `stayed_on_the_record`, and the thing does not hold them. A key the thing held
/// from that record is named under `left_the_thing`. The paired positive is an
/// ordinary edit with `sets`, whose key reaches the thing and is named there.
#[tokio::test]
async fn an_archive_with_keys_beside_it_reaches_nothing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thread:milhouse-moves", "Milhouse moves").await;
    s.add("person:homer", "Homer").await;
    s.add("person:lisa", "Lisa").await;
    let capture = |content: &str| {
        let call = json!({
            "subject": "thread:milhouse-moves", "content": content,
            "provenance": "testimony",
        });
        let s = &s;
        async move { s.call("capture", call).await.json() }
    };
    let held = |key: &'static str| {
        let s = &s;
        async move {
            s.call(
                "recall",
                json!({"subject": "thread:milhouse-moves", "keys": [key]}),
            )
            .await
            .json()["objects"][0]["fields"][key]
                .clone()
        }
    };

    // ── the paired positive: an ordinary edit's key reaches the thing ───────
    let first = capture("moved to a new street").await;
    let edited = s
        .call(
            "update_fact",
            json!({"address": first["address"], "sets": {"reviewed_by": "person:homer"}}),
        )
        .await
        .json();
    assert_eq!(
        edited["reached_the_thing"],
        json!(["reviewed_by"]),
        "{edited}"
    );
    assert_eq!(held("reviewed_by").await, "person:homer");

    // ── an archive with a key the thing never held: nothing reaches ─────────
    let second = capture("moved back").await;
    let archived = s
        .call(
            "update_fact",
            json!({"address": second["address"], "status": "archived",
                   "details": "it did not happen",
                   "sets": {"checked_by": "person:lisa"}}),
        )
        .await
        .json();
    assert_eq!(archived["reached_the_thing"], json!([]), "{archived}");
    // **The reason is a field of the record itself**, written from the details
    // of the archive, so it is a key that stayed on the record beside the one the
    // caller sent; the receipt's changed block names it too.
    assert_eq!(
        archived["stayed_on_the_record"],
        json!(["archive_reason", "checked_by"]),
        "{archived}"
    );
    assert!(
        archived["changed"].to_string().contains("archive_reason"),
        "{archived}"
    );
    assert!(held("archive_reason").await.is_null());
    let record = s
        .call(
            "recall",
            json!({"subject": "thread:milhouse-moves", "facts": true, "status": "archived"}),
        )
        .await;
    assert!(
        record.raw().contains("it did not happen") && record.raw().contains("archive_reason"),
        "the record kept its reason beside the archive: {}",
        record.raw()
    );
    assert!(held("checked_by").await.is_null());

    // ── an archive of the record that set a key: the key left the thing ─────
    let archived_first = s
        .call(
            "update_fact",
            json!({"address": first["address"], "status": "archived",
                   "details": "the move was a rumour",
                   "sets": {"reviewed_by": "person:lisa"}}),
        )
        .await
        .json();
    assert_eq!(
        archived_first["reached_the_thing"],
        json!([]),
        "{archived_first}"
    );
    assert_eq!(
        archived_first["left_the_thing"],
        json!(["reviewed_by"]),
        "{archived_first}"
    );
    assert!(held("reviewed_by").await.is_null());

    // ── an own field: advice to resend in `sets` only while the record stands
    let third = capture("moved twice").await;
    let own_on_active = s
        .call(
            "update_fact",
            json!({"address": third["address"], "fields": {"mood": "calm"}}),
        )
        .await
        .json();
    assert!(
        own_on_active["a_key_stayed_off_the_thing"].is_string(),
        "{own_on_active}"
    );
    let own_on_archive = s
        .call(
            "update_fact",
            json!({"address": third["address"], "status": "archived",
                   "details": "not twice", "fields": {"tone": "flat"}}),
        )
        .await
        .json();
    assert!(
        own_on_archive.get("a_key_stayed_off_the_thing").is_none(),
        "an archived record reaches nothing, so sets is no route: {own_on_archive}"
    );
    assert_eq!(
        own_on_archive["reached_the_thing"],
        json!([]),
        "{own_on_archive}"
    );

    // ── a key the kind reads off the thing: the `sets` hint, only while it stands
    s.add("project:backyard-shed", "Backyard shed").await;
    s.call(
        "add_entity",
        json!({"kind": "work", "handle": "sigma", "name": "Sigma", "source": "user-named",
               "parent": "project:backyard-shed"}),
    )
    .await;
    let note = s
        .call(
            "capture",
            json!({"subject": "work:sigma", "content": "the roof", "provenance": "testimony"}),
        )
        .await
        .json();
    let hinted = s
        .call(
            "update_fact",
            json!({"address": note["address"], "fields": {"owner": "person:homer"}}),
        )
        .await
        .json();
    assert!(hinted["hint"].is_string(), "{hinted}");
    let archived_hint = s
        .call(
            "update_fact",
            json!({"address": note["address"], "status": "archived",
                   "details": "not the roof", "fields": {"owner": "person:lisa"}}),
        )
        .await
        .json();
    assert!(
        archived_hint.get("hint").is_none(),
        "an archived record reaches nothing, so sets is no route: {archived_hint}"
    );

    story.finish().await;
}
