//! "I want to move abroad — help me figure out where to go."
//!
//! Follows a real conversation: an open question, some constraints, research,
//! a shortlist, then the cascade of things that have to get done. Months of it.
//!
//! `// GAP —` marks a call that cannot be made today. Those blocks are the roadmap.

use super::dsl::Story;

#[tokio::test]
async fn moving_abroad() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · the open question, and what he's optimising for ─────────
    let s = story.session().await;

    s.add("person:tulio", "Tulio").await;
    s.add("place:springfield", "Springfield").await;
    s.fact("person:tulio", "lives in Springfield, wants to move abroad")
        .await;

    // The constraints. These are testimony — he said them — and they're what
    // every later recommendation has to be checked against.
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

    // GAP — there is no entity for the operator, so in the real thing these
    // three have no subject at all. A system modelling a life has no node for
    // the person whose life it is, and every preference needs one.

    // Research. Inference, not testimony, and it reads back marked as such.
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

    // ── session 2 · "add that one to the shortlist too" ──────────────────────
    let s = story.session().await;

    s.add("place:north-haverbrook", "North Haverbrook").await;
    s.guess(
        "place:north-haverbrook",
        "coastal, mild winters, smaller scene than the capital",
    )
    .await;

    // GAP — the single most common move in the whole conversation, and there is
    // nowhere for it. These two are CANDIDATES under evaluation, one probably
    // ahead. The graph can say what is true of each and cannot say that either
    // is being considered, ranked, or ruled out.
    // s.shortlist("project:atlas", &["place:capital-city", "place:north-haverbrook"]).await;

    // GAP — no place-to-place containment. `location` points a fact at a place;
    // a city cannot be IN a country, so nothing reaches these two through Far Country.
    // s.fact_at("place:capital-city", "is a city of", "location", "place:far-country").await;

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

    // Everything he listed goes in as prose, because prose is all there is.
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

    // GAP — every line above is a TASK and none of them is one. Recorded as a
    // fact, "the photo has to be taken first" still reads as true after it is
    // taken; rewriting it in place then destroys the record that it was ever
    // outstanding. Facts are current truth by design and a task is a thing whose
    // state changes and whose history matters.
    // s.task("project:atlas", "take the visa photo").before("book the appointment").await;

    // GAP — the appointment date has nowhere to go. A fact's date is when the
    // claim became true, not when a thing happens; putting January the first
    // there redefines that field for every other fact in the system.
    // s.task("project:atlas", "embassy appointment").due("2027-01-01").await;

    // GAP — the visa, the housing, the shipping and the money are CHILDREN of
    // the move. Parentage is not reachable from the surface, so they sit flat as
    // six sentences on one node instead of being zoomable.
    // s.add_under("project:atlas", "project", "atlas-visa", "Visa").await;

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

    // GAP — `pet` is a kind by his ruling and is not built. Typed as a `thing` the
    // model says something false about a member of the household, and the vaccines,
    // the import permit and the crate booking have nowhere honest to live.
    // s.add("pet:snowball", "Snowball").await;
    // s.fact("pet:snowball", "vaccines up to date; import permit still to apply for").await;

    s.add("event:departure-flight", "Departure").await;
    s.fact(
        "event:departure-flight",
        "one-way, booked once the visa lands",
    )
    .await;

    // GAP — an event here is an entity, not a dated occurrence. No date on it, no
    // type carrying the fields a flight has, no way to say who is on it.
    // s.event_typed("flight", "2027-02-09", &[("with", "person:bodoque")]).await;

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

    // The real assertion: cold, weeks later, it finds the file without being told.
    s.find("visa").await.says("project:atlas");
    s.find("container").await.says("org:springfield-movers");
    s.recall("project:atlas").await.says("embassy");
    s.list("place").await.says("place:north-haverbrook");

    // GAP — the question he actually asks at this point. Nothing can answer
    // "what is still open", because nothing here has a state.
    // s.open_under("project:atlas").says("visa photo");

    // GAP — the appointment moved twice. Retraction is decided and not built, so
    // the record keeps every wrong date with nothing saying which one held.
    // s.retract("project:atlas#f3", "appointment was rebooked").await;

    s.wrap("still in progress").await;

    story.finish().await;
}
