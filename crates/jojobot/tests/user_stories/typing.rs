//! "I have been writing down services for months. Can I get the ones with a
//! cost on them?"
//!
//! The records came first and nobody had a type in mind when they were
//! written. **That is the case this exists to prove**: a type declared today
//! reaches things described before it, because a thing answers a type by the
//! keys its records carry and was never asked what it was declared to be. If
//! declaring had to come first, every record already in the store would be out
//! of reach for ever, which is exactly the store somebody has.
//!
//! **And the question is asked of the THING.** What a bike is gets written
//! down a piece at a time, so its `serviced` and its `cost` can come from
//! different sittings and still make it a serviced thing between them.
//!
//! The things are also messy, the way things are: one is missing a key, one
//! has a date somebody typed in words. Both come back — a query that returned
//! only the tidy ones would hide the things worth finding, and a caller
//! looking for work to do is looking for precisely the untidy ones.
//!
//! **And the title is the other question.** "The ones with a cost on them"
//! keeps only the things carrying every key, so the story asks both: the
//! tolerant question for what is worth looking at, and the strict one for what
//! actually is a service.

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

    // Whole: both keys, and nothing on the record says "service" at all. There
    // is no name for a class of record to give, so keys are the only thing a
    // type can be answered by.
    s.event_with(
        "thing:gravel-bike",
        "new chain and a full clean",
        json!({ "serviced": "2026-03-14", "cost": "40" }),
        &[],
    )
    .await;

    // Partial: somebody wrote down when, and never what it cost.
    s.event_with(
        "thing:road-bike",
        "brake bleed, invoice never came",
        json!({ "serviced": "2026-04-02" }),
        &[],
    )
    .await;

    // Messy: the key is there and the value is a phrase, not a date.
    s.event_with(
        "thing:floor-pump",
        "reseated the hose, no charge",
        json!({ "serviced": "some time in may", "cost": "0" }),
        &[],
    )
    .await;

    // And one that is none of this: a record with keys that have nothing to do
    // with a service, on a thing that HAS been serviced. It is what makes the
    // unit legible — the outing does not stop the bike being a serviced thing,
    // and it does not make it one either.
    s.event_with(
        "thing:gravel-bike",
        "rode it to the coast",
        json!({ "distance": "80" }),
        &[],
    )
    .await;

    // And the negative every answer below rests on: a thing nobody has ever
    // serviced. Without it, "the type found the serviced things" would be
    // indistinguishable from "the type found everything".
    s.add("thing:torque-wrench", "The Torque Wrench").await;
    s.event_with(
        "thing:torque-wrench",
        "borrowed twice, never serviced",
        json!({ "distance": "3" }),
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
    //
    // **The answer is the THINGS.** A type describes what a thing is, and the
    // records are how it came to be known — so a caller asking "which of mine
    // are services" is asking about the bike, not about the row somebody typed
    // it into.
    let found = s
        .call("search", json!({ "answers_type": "service", "limit": 50 }))
        .await;

    // The positive everything else rests on: a thing described months before
    // the type existed, under nobody's type, comes back whole.
    found.says("thing:gravel-bike");
    found.says("\"complete\":true");

    // The partial one is here too, saying what it lacks by name — not hidden,
    // and not reported as a count the caller has to interpret.
    found.says("thing:road-bike");
    found.says("\"lacking\":[\"cost\"]");

    // The messy one is here, flagged rather than dropped: the reader sees the
    // value that is wrong and can go and fix it.
    found.says("thing:floor-pump");
    found.says("\"value\":\"some time in may\"");

    // …and the negative, in the same answer that just proved it is not empty:
    // a thing sharing no key with the type is not a weak match.
    found.never_says("thing:torque-wrench");

    // ── the same type, asked strictly ───────────────────────────────────────
    //
    // **The question in the title, and the answer above is not it.** "The ones
    // with a cost on them" keeps only the things with no gaps, and the tolerant
    // read hands back the brake bleed as well. Which of the two questions is
    // being asked is the caller's to choose, and both are one call.
    let whole = s
        .call("search", json!({ "fits_type": "service", "limit": 50 }))
        .await;
    whole.says("thing:gravel-bike");
    // The messy one fits. A key holding a value the type did not describe is
    // still a key the thing carries, and this question is about gaps — so the
    // strict answer reports the bad value rather than dropping the thing.
    whole.says("thing:floor-pump");
    whole.says("\"value\":\"some time in may\"");
    // **The pair this turns on, asserted both ways.** The brake bleed IS in the
    // tolerant answer above and OUT of this one; a negative on its own would
    // pass on an empty answer.
    whole.never_says("thing:road-bike");
    whole.never_says("thing:torque-wrench");

    // Asked both ways in one call, nothing picks for the caller: the two keep
    // different things, so the answer says so and runs neither.
    s.refused(
        "search",
        json!({ "answers_type": "service", "fits_type": "service" }),
    )
    .await
    .says("fits_type");

    // ── the same type, asked as a question about THINGS ─────────────────────
    //
    // **A THING answers a type, not one of its rows.** The bike's `serviced`
    // and its `cost` could have been written on different days by different
    // sessions; what makes the bike a serviced thing is that between them its
    // records carry the keys.
    //
    // **So the answer is the fold, and the records are not asked for.** Which
    // sitting each key arrived in is not what "have I serviced it" is asking,
    // and reading every claim to work out a thing's keys is the long way round
    // a question the object now answers by itself.
    let serviced = s
        .shape(
            "the things that have been serviced",
            json!({ "answers_type": "service" }),
        )
        .await;
    serviced.says("thing:gravel-bike");
    // Both keys on one row, off two records written on different days. That is
    // the fold doing the work the type depends on.
    serviced.says("\"serviced\":\"2026-03-14\"");
    serviced.says("\"cost\":\"40\"");
    // …and the outing's key is on the same row, because the fold is over
    // everything the bike has, not over what the type asked about.
    serviced.says("\"distance\":\"80\"");
    serviced.says("\"complete\":true");
    // The claims are behind it and the answer says how many rather than leaving
    // a reader to wonder whether the bike has any.
    serviced.says("records are behind these fields");
    serviced.never_says("new chain and a full clean");
    // The same negative, in the same answer that just proved it is not empty.
    serviced.never_says("thing:torque-wrench");

    // And narrowed further by a value, which is the axis a type alone does not
    // have: the service that cost nothing.
    s.shape(
        "the things with a service that cost nothing",
        json!({ "fields": [{ "key": "cost", "value": "0" }], "facts": true }),
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

    // ── and the operator opens the page, which speaks no MCP ────────────────
    //
    // **The one surface he reads himself.** Everything above went through the
    // verbs a session calls; this is the window, and what a thing IS has to be
    // legible in it or the model is invisible to the person it is for.
    let page = story.page("thing:gravel-bike").await;
    // **The fold, asserted on the fold's own table.** `serviced` and `cost`
    // were written on one record and `distance` on another, and both are on the
    // page a second time under the records they came from — so a beat that
    // matched the whole page would pass with the folded table missing
    // altogether.
    let fields = page.section("fields");
    fields.says("serviced").says("2026-03-14");
    fields.says("distance").says("80");
    // What the fields add up to, in the type's own words. It is the one thing
    // on the page he could not work out for himself without holding every
    // declaration in his head.
    //
    // **Pinned to the row's own cells, because the type is named `service` and
    // the key it asks for is `serviced`.** The keys the thing holds are a
    // column of this same table, so a needle for the type's name anywhere in
    // the table is answered by the key — and a beat written that way says
    // nothing about the Type column, on a page where that column is empty.
    page.section("conforms")
        .says("<tr><td>service</td><td>whole</td>");

    story.finish().await;
}
