//! "I have been writing down services for months. Can I get the ones with a
//! cost on them?"
//!
//! The records came first and nobody had a type in mind when they were
//! written. **That is the case this exists to prove**: a type declared today
//! reaches records written before it, because a record answers a type by the
//! keys it carries and was never asked what it was declared to be. If
//! declaring had to come first, every record already in the store would be out
//! of reach for ever, which is exactly the store somebody has.
//!
//! The records are also messy, the way records are: one is missing a key, one
//! has a date somebody typed in words. Both come back — a query that returned
//! only the tidy ones would hide the records worth finding, and a caller
//! looking for work to do is looking for precisely the untidy ones.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_type_declared_today_finds_records_written_before_it() {
    let story = Story::begin("bot:otto").await;

    // ── over the months · services recorded, with no type in mind ───────────
    let s = story.session().await;
    s.add("thing:gravel-bike", "The Gravel Bike").await;
    s.add("thing:road-bike", "The Road Bike").await;
    s.add("thing:floor-pump", "The Floor Pump").await;

    // Whole: both keys, and it calls itself something else entirely. Nothing
    // on this record says "service" in the way a type would.
    s.event_with(
        "thing:gravel-bike",
        "new chain and a full clean",
        "workshop-visit",
        json!({ "serviced": "2026-03-14", "cost": "40" }),
        &[],
    )
    .await;

    // Partial: somebody wrote down when, and never what it cost.
    s.event_with(
        "thing:road-bike",
        "brake bleed, invoice never came",
        "workshop-visit",
        json!({ "serviced": "2026-04-02" }),
        &[],
    )
    .await;

    // Messy: the key is there and the value is a phrase, not a date.
    s.event_with(
        "thing:floor-pump",
        "reseated the hose, no charge",
        "workshop-visit",
        json!({ "serviced": "some time in may", "cost": "0" }),
        &[],
    )
    .await;

    // And one that is none of this: an event with keys that have nothing to do
    // with a service. It is what stops "it matched" from meaning nothing.
    s.event_with(
        "thing:gravel-bike",
        "rode it to the coast",
        "outing",
        json!({ "distance": "80" }),
        &[],
    )
    .await;

    // ── today · the type is named for the first time ────────────────────────
    let declared = s
        .call(
            "declare_type",
            json!({
                "name": "service",
                "fields": [
                    { "key": "serviced", "holds": "date" },
                    { "key": "cost", "holds": "number" },
                ],
            }),
        )
        .await;
    declared.says("\"name\":\"service\"");
    // The keys come back in the order they were declared, which is the order
    // every answer below names them in.
    declared.says("\"key\":\"serviced\"");
    declared.says("\"key\":\"cost\"");

    // ── and it reaches everything already written ───────────────────────────
    let found = s
        .call("search", json!({ "answers_type": "service", "limit": 50 }))
        .await;

    // The positive everything else rests on: a record written months before
    // the type existed, filed under a different name, comes back.
    found.says("new chain and a full clean");
    found.says("\"complete\":true");

    // The partial one is here too, saying what it lacks by name — not hidden,
    // and not reported as a count the caller has to interpret.
    found.says("brake bleed, invoice never came");
    found.says("\"lacking\":[\"cost\"]");

    // The messy one is here, flagged rather than dropped: the reader sees the
    // value that is wrong and can go and fix it.
    found.says("reseated the hose, no charge");
    found.says("\"value\":\"some time in may\"");

    // …and the negative, in the same answer that just proved it is not empty:
    // a record sharing no key with the type is not a weak match.
    found.never_says("rode it to the coast");

    // ── the same type, asked as a question about THINGS ─────────────────────
    //
    // Search answers with the records. The other question is "which of my
    // things have been serviced at all", and that is about the objects rather
    // than about their rows — so it goes to the query that returns objects,
    // and each one arrives holding only the records that answered.
    let serviced = s
        .shape(
            "the things carrying a service record",
            json!({ "answers_type": "service" }),
        )
        .await;
    serviced.says("thing:gravel-bike");
    serviced.says("new chain and a full clean");
    // The negative in the same answer: the outing shares no key with the type,
    // so it is not one of the records that answered, even though it sits on
    // the object that came back.
    serviced.never_says("rode it to the coast");

    // And narrowed further by a value, which is the axis a type alone does not
    // have: the service that cost nothing.
    s.shape(
        "the things with a service that cost nothing",
        json!({ "fields": [{ "key": "cost", "value": "0" }] }),
    )
    .await
    .says("reseated the hose, no charge")
    .never_says("new chain and a full clean");

    // ── a name nobody declared says so, and says what there is ──────────────
    let missing = s
        .refused("search", json!({ "answers_type": "warranty", "limit": 50 }))
        .await;
    // It names the types that do exist, so the next call is reachable from the
    // refusal rather than from somewhere else.
    missing.says("service");

    s.wrap("named the shape the services already had, and found them")
        .await;
    story.finish().await;
}
