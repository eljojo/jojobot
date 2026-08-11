//! "The dog is Bart's. So is the cat. Which of his pets is the old one?"
//!
//! Three questions that a store full of records could not answer until a
//! declaration made a key into a link.
//!
//! **The link is the point.** Every pet record already carried the owner's
//! handle, and it was a string that looked like a handle: nothing walked it,
//! because nothing had said it was anything. Declaring `owner` to hold a
//! reference is what turns it into a relation — and the reverse of that
//! relation, `pet.owner`, is the has-many nobody wrote down. It is not a sixth
//! edge shape and nobody declared an inverse.
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
        "pet",
        json!({ "born": "2019-04-15", "weight": "27", "owner": "person:bart" }),
        &[],
    )
    .await;
    s.event_with(
        "pet:snowball",
        "the cat",
        "pet",
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
        "repair",
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
        "repair",
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
                { "key": "name", "holds": "text" },
                { "key": "born", "holds": "date" },
                { "key": "weight", "holds": "number" },
                { "key": "owner", "holds": "reference" },
            ],
        }),
    )
    .await
    .says("\"name\":\"pet\"");

    // ── forward · from the pet, out to whoever owns it ──────────────────────
    //
    // The key's own name is the relation, and no direction is passed: the name
    // is what says which way it goes.
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

    // ── reverse · from the person, back to every pet — the has-many ─────────
    //
    // Nobody declared this direction. `pet.owner` is derived from the type and
    // the key, and it is qualified by the type because one type may declare two
    // reference keys onto the same kind.
    let pets = s
        .shape(
            "everything of Bart's that is a pet",
            json!({
                "subject": "person:bart",
                "facts": false,
                "follow": { "relation": "pet.owner" },
            }),
        )
        .await;
    pets.says("pet:santas-little-helper");
    pets.says("pet:snowball");
    // The pump's record shares no key with `pet`, so it answers the type not at
    // all and the walk does not reach it. That is what makes the two hits above
    // mean something.
    pets.never_says("thing:floor-pump");

    // GAP — the bike comes back, and it is not a pet.
    //
    // Its repair record carries `owner` and nothing else of `pet`, and a record
    // holding ONE of a type's keys answers that type: `search` with
    // `answers_type: pet` returns this same record today. So the reverse
    // relation agrees with the rest of the store rather than disagreeing with
    // it, and the price is that a key shared between two types puts foreign
    // records in the has-many.
    //
    // Narrowing it is a decision about what `type.key` means, not a defect to
    // patch here: a stricter rule would make one verb say a record answers a
    // type while another says it does not, about the same record.
    pets.says("thing:red-bike");

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
                    "relation": "pet.owner",
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
    .says("pet.owner");

    s.wrap("named the link the pet records already had, and walked it both ways")
        .await;
    story.finish().await;
}
