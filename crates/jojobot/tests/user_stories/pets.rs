//! "The dog is Bart's. So is the cat. Which of his pets is the old one?"
//!
//! Three questions that a store full of records could not answer until a
//! declaration made a key into a link.
//!
//! **The link is the point.** Every pet record already carried the owner's
//! handle, and it was a string that looked like a handle: nothing walked it,
//! because nothing had said it was anything. Declaring `owner` to hold a
//! reference is what turns it into a relation — one relation, `owner`, walked
//! outbound to what a record points at and inbound to the records pointing
//! back. It is not a sixth edge shape and nobody declared an inverse.
//!
//! **What the inbound walk computes is key-scoped.** It reaches everything
//! naming Bart through `owner`, the bicycle included — and that is the right
//! answer to what it was asked. Narrowing it to "his PETS" is a second
//! question asked beside the walk: which of the things it reached FIT the type,
//! meaning carry every key it names. The bike answers `pet` and never fits it.
//!
//! **The ordering is the second half.** `born` holds a date, and saying so is
//! what makes "before" a question this store can be asked. A key with no
//! declaration behind it has equality and nothing more, which is why the last
//! beat here asks for an ordering nobody licensed and is turned back.
//!
//! What declaring buys is REACH. Nothing is gated by it: the same records were
//! findable by the keys they carry before any of this, and still are.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_declared_reference_key_answers_the_questions_about_the_pets() {
    let story = Story::begin("bot:otto").await;

    // ── the records, written with nobody's type in mind ─────────────────────
    let s = story.session().await;
    s.add("person:bart", "Bart").await;
    s.add("pet:santas-little-helper", "Santa's Little Helper")
        .await;
    s.add("pet:snowball", "Snowball").await;

    s.event_with(
        "pet:santas-little-helper",
        "the greyhound, came home from the track",
        json!({ "born": "2019-04-15", "weight": "27", "owner": "person:bart" }),
        &[],
    )
    .await;
    s.event_with(
        "pet:snowball",
        "the cat",
        json!({ "born": "2024-11-02", "weight": "4", "owner": "person:bart" }),
        &[],
    )
    .await;

    // The negative every answer below rests on: a thing of Bart's that is no
    // pet. Without it, "the walk reached the pets" would be indistinguishable
    // from "the walk reached everything".
    s.add("thing:floor-pump", "The Floor Pump").await;
    s.event_with(
        "thing:floor-pump",
        "reseated the hose",
        json!({ "fitted": "2026-02-01" }),
        &[],
    )
    .await;

    // And the awkward one, which is here because it is what a real store looks
    // like: a record that is no pet and carries the same key anyway.
    s.add("thing:red-bike", "The Red Bike").await;
    s.event_with(
        "thing:red-bike",
        "needs new brake pads",
        json!({ "owner": "person:bart" }),
        &[],
    )
    .await;

    // ── the declaration, which is the whole of what changes ─────────────────
    s.call(
        "declare_type",
        json!({
            "name": "pet",
            "fields": [
                { "key": "name", "holds": "text", "required": true },
                { "key": "born", "holds": "date", "required": true },
                { "key": "weight", "holds": "number", "required": true },
                { "key": "owner", "holds": "reference", "required": true },
            ],
        }),
    )
    .await
    .says("\"name\":\"pet\"");

    // ── forward · from the pet, out to whoever owns it ──────────────────────
    //
    // The key IS the relation's name. No direction is passed because outbound
    // is the default: the key off this record, to what it points at.
    let owner = s
        .shape(
            "who the greyhound belongs to",
            json!({
                "subject": "pet:santas-little-helper",
                "facts": false,
                "follow": { "relation": "owner" },
            }),
        )
        .await;
    owner.says("person:bart");
    owner.says("\"relation\":\"owner\"");

    // ── inbound · everything pointing at the person through that key ────────
    //
    // The same name, walked the other way. Nobody declared an inverse: there is
    // one relation, `owner`, and the direction picks the question.
    let pointing_at_bart = s
        .shape(
            "everything that names Bart as its owner",
            json!({
                "subject": "person:bart",
                "facts": false,
                "follow": { "relation": "owner", "direction": "in" },
            }),
        )
        .await;
    pointing_at_bart.says("pet:santas-little-helper");
    pointing_at_bart.says("pet:snowball");
    // The pump's record carries no `owner` at all, so nothing reaches it. That
    // is what makes the hits above mean something.
    pointing_at_bart.never_says("thing:floor-pump");

    // …and the bike, whose repair record names the same key. **This is
    // correct.** The walk was asked for everything pointing here through
    // `owner`, and the bike does.
    pointing_at_bart.says("thing:red-bike");

    // ── the type-scoped has-many · "Bart's PETS", in one walk ───────────────
    //
    // **What narrows the walk is FITTING the type, not answering it.** The bike
    // answers `pet` — its repair record carries `owner`, which is one of the
    // type's keys — and answering is what a partial match is. Fitting is
    // carrying all of them, and the bike never will.
    //
    // The pets do, and neither of their records does it alone: the name came
    // from one sitting and the rest from another. That is what makes this a
    // question about the THINGS rather than about the rows.
    for (pet, name) in [
        ("pet:santas-little-helper", "Santa's Little Helper"),
        ("pet:snowball", "Snowball"),
    ] {
        s.event_with(pet, "what we call it", json!({ "name": name }), &[])
            .await;
    }
    let his_pets_walked = s
        .shape(
            "Bart's pets, walked",
            json!({
                "subject": "person:bart",
                "facts": false,
                "follow": { "relation": "owner", "direction": "in", "fits_type": "pet" },
            }),
        )
        .await;
    his_pets_walked.says("pet:santas-little-helper");
    his_pets_walked.says("pet:snowball");
    // The pair this turns on, asserted both ways: the bike IS in the unnarrowed
    // walk above and OUT of this one. A negative on its own would pass on an
    // empty answer.
    his_pets_walked.never_says("thing:red-bike");
    // …and the bike is not silently gone: the walk reached it and did not keep
    // it, which is an edge nobody followed.
    his_pets_walked.says("unwalked");

    // The name `pet.owner` stays gone. It spelled the narrowing as part of the
    // relation, which said the key belonged to one type; the key is one key and
    // the narrowing is a separate question asked beside it.
    s.refused(
        "recall",
        json!({
            "subject": "person:bart",
            "follow": { "relation": "pet.owner" },
        }),
    )
    .await
    .says("owner");

    // ── and the question that DOES work today ───────────────────────────────
    //
    // Not a walk: a selection. Entities of kind `pet` whose record names Bart —
    // a kind and a key filter, both of which already existed. It answers "his
    // pets" exactly, and it is the shape to reach for until the walk can.
    let his_pets = s
        .shape(
            "Bart's pets, by kind and key",
            json!({
                "kind": "pet",
                "fields": [{ "key": "owner", "value": "person:bart" }],
            }),
        )
        .await;
    his_pets.says("pet:santas-little-helper");
    his_pets.says("pet:snowball");
    // The pair the gap turns on, asserted both ways rather than as one absence:
    // the bike is IN the key-scoped walk above and OUT of this kind-scoped
    // selection. A negative on its own would pass on an empty answer.
    his_pets.never_says("thing:red-bike");

    // ── the filtered walk · the acceptance case, in one call ────────────────
    //
    // "His pets born before a date" was two questions before this: which are
    // his pets, then which of those is old. The filter rides on the walk.
    let older = s
        .shape(
            "Bart's pets born before 2020",
            json!({
                "subject": "person:bart",
                "follow": {
                    "relation": "owner",
                    "direction": "in",
                    "keeping": [{ "key": "born", "compare": "before", "value": "2020-01-01" }],
                },
            }),
        )
        .await;
    older.says("pet:santas-little-helper");
    older.never_says("pet:snowball");
    // The pet it did not keep is an edge nobody followed, and the answer says
    // so rather than leaving an empty list to be read as "there are no others".
    older.says("unwalked");

    // The number half of the same rule, on the roots rather than on a walk:
    // `weight` was declared to hold a number, so it can be compared as one.
    s.shape(
        "the pets under ten",
        json!({ "fields": [{ "key": "weight", "compare": "less", "value": "10" }] }),
    )
    .await
    .says("pet:snowball")
    .never_says("pet:santas-little-helper");

    // ── and what a declaration does NOT license ─────────────────────────────
    //
    // `repair` is nobody's type, so its keys have equality and nothing else.
    // Asking for an ordering over one comes back blocked rather than quietly
    // answering the equality question, which a caller would read as an answer.
    s.refused(
        "recall",
        json!({ "fields": [{ "key": "fitted", "compare": "after", "value": "2026-01-01" }] }),
    )
    .await
    .says("declar");

    // …and the equality that needs no declaration still works on the very same
    // undeclared key, so the refusal above is about the ordering and not about
    // undeclared records.
    s.shape(
        "the thing whose owner is recorded, by the value itself",
        json!({ "kind": "thing", "fields": [{ "key": "owner", "value": "person:bart" }] }),
    )
    .await
    .says("thing:red-bike");

    // ── a relation nobody declared says so, and says what there is ──────────
    s.refused(
        "recall",
        json!({
            "subject": "person:bart",
            "follow": { "relation": "vet.patient" },
        }),
    )
    .await
    .says("owner");

    s.wrap("named the link the pet records already had, and walked it both ways")
        .await;
    story.finish().await;
}
