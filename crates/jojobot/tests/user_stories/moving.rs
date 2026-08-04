//! "I want to move abroad — help me figure out where to go, and then help me
//! actually do it."
//!
//! An open question, some constraints, research, a shortlist, then the cascade
//! of things that have to get done. Months of it.
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn moving_abroad() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · the open question, and what it is optimising for ────────
    let s = story.session().await;

    s.add("person:tulio", "Tulio").await;
    s.add("place:springfield", "Springfield").await;
    s.fact("person:tulio", "lives in Springfield, wants to move abroad")
        .await;

    // The constraints: testimony, and what every later recommendation has to
    // be checked against.
    s.fact(
        "person:tulio",
        "can work as a waiter, a teacher, or a programmer",
    )
    .await;
    s.fact(
        "person:tulio",
        "wants a city with an electronic music scene, and culture generally",
    )
    .await;
    s.fact("person:tulio", "likes the beach; good weather is a plus")
        .await;

    // GAP — there is no entity for the operator, so outside this story those
    // three claims have no subject at all. A system modelling a life has no
    // node for the person whose life it is, and every preference needs one.

    // Research: inference, not testimony, and it reads back marked as such.
    s.add("place:far-country", "Far Country").await;
    s.add("place:capital-city", "Capital City").await;
    s.guess(
        "place:capital-city",
        "large electronic scene, several venues, weekly nights",
    )
    .await;
    s.guess("place:capital-city", "no coast; winters are hard")
        .await;

    s.journal("gathered the constraints, started looking at cities")
        .await;
    s.wrap("constraints captured, one city looked at").await;

    // ── session 2 · "add that one to the shortlist too" ─────────────────────
    let s = story.session().await;

    s.add("place:north-haverbrook", "North Haverbrook").await;
    s.guess(
        "place:north-haverbrook",
        "coastal, mild winters, smaller scene than the capital",
    )
    .await;

    // GAP — the most common move in the whole conversation, and there is
    // nowhere for it. These two are candidates under evaluation, one probably
    // ahead. The graph can say what is true of each and cannot say either is
    // being considered, ranked, or ruled out.
    //   s.shortlist("project:atlas", &["place:capital-city", "place:north-haverbrook"]).await;

    // A place DOES sit inside another: `location` constrains what an edge
    // points AT, never what it points from, so a city in a country is an
    // ordinary edge and both cities are reachable through Far Country in one
    // walk.
    for city in ["place:capital-city", "place:north-haverbrook"] {
        s.fact_about(
            city,
            "a city of Far Country",
            "location",
            "place:far-country",
        )
        .await;
    }
    s.through("location", "place:far-country", "place")
        .await
        .says("place:capital-city")
        .says("place:north-haverbrook");

    // GAP — and the containment is only as good as the claim's wording.
    // Nothing says this edge means "inside" rather than "near" or "flies
    // to", so a walk finds the pair and a reader still has to read each
    // sentence to learn what the link was.

    s.wrap("two cities on the table").await;

    // ── session 3 · the work that actually has to happen ────────────────────
    let s = story.session().await;

    s.add("project:atlas", "The Move").await;
    s.fact(
        "project:atlas",
        "moving to Far Country; city not settled yet",
    )
    .await;

    s.add("person:patana", "Patana").await;
    s.fact(
        "person:patana",
        "immigration lawyer, handling the visa file",
    )
    .await;
    s.add("org:globex", "Globex").await;
    s.fact("org:globex", "hiring programmers, would sponsor")
        .await;

    // GAP — the lawyer's part in the move cannot be an edge. `membership`
    // points at an org, so a person's role IN A PROJECT has no shape: who is
    // doing what on this move is prose, and "who is involved" is a word search
    // rather than a walk. Compare the same question about a company, which is
    // one call.
    //   s.fact_about("person:patana", "handling the visa", "role", "project:atlas").await;

    // Everything listed goes in as prose, because prose is all there is.
    s.fact(
        "project:atlas",
        "visa: embassy appointments open on the first of January",
    )
    .await;
    s.fact(
        "project:atlas",
        "visa photo has to be taken before the appointment",
    )
    .await;
    s.fact("project:atlas", "flights: watch prices, set money aside")
        .await;
    s.fact("project:atlas", "decide what ships and what gets sold")
        .await;
    s.fact(
        "project:atlas",
        "somewhere short-term for the first month, then find permanent",
    )
    .await;
    s.fact(
        "project:atlas",
        "check what medical paperwork is needed, and the city hall side",
    )
    .await;

    // GAP — every line above is a TASK and none of them is one. As a fact,
    // "the photo has to be taken first" still reads as true after it is taken,
    // and rewriting it in place then destroys the record that it was ever
    // outstanding. Facts are current truth by design; a task is a thing whose
    // state changes and whose history matters.
    //   s.task("project:atlas", "take the visa photo").before("book the appointment").await;

    // GAP — and the appointment date has nowhere to go. Putting January the
    // first in a claim's date field redefines that field for every other claim
    // in the system.
    //   s.task("project:atlas", "embassy appointment").due("2027-01-01").await;

    // GAP — the visa, the housing, the shipping and the money are CHILDREN of
    // the move. Parentage is not reachable from the surface, so they sit flat
    // as six sentences on one node instead of being zoomable.
    //   s.add_under("project:atlas", "project:atlas-visa", "Visa").await;

    s.wrap("the list exists; nothing is done").await;

    // ── session 4 · shipping a life ─────────────────────────────────────────
    let s = story.session().await;

    s.add("org:springfield-movers", "Springfield Movers").await;
    s.fact(
        "org:springfield-movers",
        "quoted for a container, insured to replacement value",
    )
    .await;

    s.add("thing:red-bike", "Red Bike").await;
    s.fact("thing:red-bike", "ships in the container, boxed")
        .await;

    // GAP — `pet` is a decided kind and is not built. Typed as a `thing`, the
    // model says something false about a member of the household, and the
    // vaccines, the import permit and the crate booking have nowhere honest to
    // live.
    //   s.add("pet:snowball", "Snowball").await;

    s.add("event:departure-flight", "Departure").await;
    s.fact(
        "event:departure-flight",
        "one-way, booked once the visa lands",
    )
    .await;

    // The occurrence is a typed event on that entity: the fields a flight has
    // are values, and `refs` says who is on it.
    let flight = s
        .event_with(
            "event:departure-flight",
            "the flight the family is booked on",
            "flight",
            json!({"departs_on": "2027-02-09", "one_way": "yes"}),
            &["person:tulio"],
        )
        .await;
    s.recall("event:departure-flight")
        .await
        .claim(&flight)
        .says("2027-02-09")
        .says("person:tulio");

    // GAP — and the entity itself still has no date. `event:departure-flight`
    // is a node whose occurrence lives on a claim about it, so "what is
    // happening in February" has to read every event's claims rather than the
    // events.
    //   s.happens_on("event:departure-flight", "2027-02-09").await;

    s.wrap("movers quoted, flight sketched").await;

    // ── session 5 · a session that was not there for any of it ──────────────
    let s = story.session().await;

    s.add("person:bodoque", "Bodoque").await;
    s.fact_about(
        "person:bodoque",
        "already lives out there, offered a spare room for the first weeks",
        "location",
        "place:capital-city",
    )
    .await;

    // Cold, weeks later, it finds the file without being told.
    s.find("visa").await.says("project:atlas");
    s.find("container").await.says("org:springfield-movers");
    s.recall("project:atlas").await.says("embassy");
    s.list("place").await.says("place:north-haverbrook");

    // And who is already in the destination city is one walk, not a search
    // through wording.
    s.through("location", "place:capital-city", "person")
        .await
        .says("person:bodoque");

    // GAP — the question actually asked at this point. Nothing can answer
    // "what is still open", because nothing here has a state.
    //   s.open_under("project:atlas").says("visa photo").await;

    // The appointment moved, and the claim that held before it moved is put
    // past rather than rewritten or taken back: it was true in its day, so it
    // stays on the record and stops coming back as current truth.
    let first_date = s
        .fact("project:atlas", "embassy appointment is on the first")
        .await;
    s.supersede(&first_date).await;
    s.fact(
        "project:atlas",
        "embassy appointment was rebooked to the ninth",
    )
    .await;
    // It stays on the record, marked as moved past — and the front door stops
    // offering it as current truth, which is the difference between putting a
    // claim past and deleting it.
    s.recall("project:atlas")
        .await
        .claim(&first_date)
        .says("superseded");
    s.find("embassy appointment")
        .await
        .says("rebooked to the ninth")
        .never_says("appointment is on the first");

    // GAP — and nothing says WHICH claim replaced it. A superseded claim knows
    // it was moved past and not what moved past it, so a reader reconstructing
    // the sequence matches the wording by hand.
    //   s.superseded_by("project:atlas#f3", &rebooked).await;

    s.wrap("still in progress").await;

    story.finish().await;
}
