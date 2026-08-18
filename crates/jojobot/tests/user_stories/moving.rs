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
    //   s.add_the_operator("The operator").await;
    s.list("person").await.never_says("\"operator\"");

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

    // GAP — the most common move in the whole conversation, and half of it is
    // served now. HOLDING the candidates is a key: a declared `candidate_for`
    // reference points each city at the move, and the walk back from the move
    // returns the pair, which is what "being considered" needs. The rest of
    // choosing is still missing. Nothing ranks them, and ruling one out is
    // recordable but not askable — every filter says which records to KEEP, so
    // "the candidates not ruled out" is a question about a record that is not
    // there.
    //   s.shortlist("project:atlas", &["place:capital-city", "place:north-haverbrook"]).await;
    s.has_no_verb("shortlist", &["capture", "search"]).await;

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

    // GAP — and the EDGE is only as good as the claim's wording. Nothing says
    // this shape means "inside" rather than "near" or "flies to", so a walk by
    // shape finds the pair and a reader still reads each sentence to learn
    // what the link was. Naming the link is no longer out of reach: a key
    // declared to hold a reference is a relation the query walks both ways, so
    // an `inside` key says what `location` cannot. The residual is the narrow
    // one — the name lives on a KEY of the record and never on the edge, and
    // the five shapes stay a closed vocabulary.
    //   s.fact_about("place:capital-city", "…", "inside", "place:far-country").await;
    s.has_no_verb("contains", &["capture", "search"]).await;

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
    s.refused(
        "capture",
        json!({
            "subject": "person:patana", "content": "handling the visa",
            "provenance": "testimony", "shape": "membership",
            "object": "project:atlas",
        }),
    )
    .await
    .says("membership");

    // Most of the list goes in as prose, which is what a session records when
    // it is taking dictation rather than filing.
    s.fact(
        "project:atlas",
        "visa: embassy appointments open on the first of January",
    )
    .await;
    s.fact("project:atlas", "flights: watch prices, set money aside")
        .await;

    // Two of them go in as records with keys, because they are the two the
    // operator asks after: what is still outstanding, and when it is due. A
    // key is where that goes — the state and the day are values a question can
    // reach, where the sentences above can only be searched for by wording.
    s.event_with(
        "project:atlas",
        "visa photo has to be taken before the appointment",
        json!({"state": "open"}),
        &[],
    )
    .await;
    s.event_with(
        "project:atlas",
        "embassy appointment",
        json!({"state": "open", "due": "2027-01-01"}),
        &[],
    )
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

    // GAP — every line above is a TASK and none of them is one. The two keyed
    // records hold a state and a day, which is what a task is made of, and
    // nothing moves either: taking the photo means a session rewrites `state`
    // in place, and the record that it was ever outstanding goes with the
    // rewrite. Nothing orders them, nothing knows the photo comes before the
    // appointment, and nothing is a task by construction — two sessions
    // filing the same work under `state` and `status` are filing two things.
    //   s.task("project:atlas", "take the visa photo").before("book the appointment").await;
    s.has_no_verb("task", &["capture", "update_fact"]).await;

    // …and the date the appointment is due for is on the record beside its
    // state, where a question about January reaches it. Declaring the type is
    // what makes the day comparable rather than a string that happens to sort.
    s.call(
        "declare_type",
        json!({
            "name": "commitment",
            "fields": [{ "key": "due", "holds": "date" }],
        }),
    )
    .await
    .says("\"name\":\"commitment\"");
    // **And the question is asked of the RECORD**, which is the whole of what
    // the gap above is about: every commitment here is a row on the one
    // project, so "what is due in January" is a question about the rows. Asked
    // of the thing it would be a question about the project, and the project
    // holds a `due` — so it would come back with all six of its rows, the
    // undated photo among them. On a build where each of these is its own
    // thing, this is the default question again.
    let january = s
        .shape(
            "what is due before the end of January",
            json!({
                "fields": [{
                    "key": "due", "compare": "before", "value": "2027-02-01", "scope": "record",
                }],
                "facts": true,
            }),
        )
        .await;
    january.says("embassy appointment");
    // The negative in the same answer: the photo is outstanding too and nobody
    // ever gave it a day, so a read about January must not reach it.
    january.never_says("visa photo");

    // GAP — the visa, the housing, the shipping and the money are CHILDREN of
    // the move. Parentage is not reachable from the surface, so they sit flat
    // as six sentences on one node instead of being zoomable.
    //   s.add_under("project:atlas", "project:atlas-visa", "Visa").await;
    s.list("project").await.never_says("parent");

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

    // A member of the household, filed as one. The bike above ships in the
    // container and the cat does not, and the model now says which is which
    // rather than calling both a possession.
    s.add("pet:snowball", "Snowball").await;
    s.fact("pet:snowball", "rabies shot done, boosters up to date")
        .await;
    s.fact(
        "pet:snowball",
        "import permit applied for, four weeks quoted",
    )
    .await;
    s.fact(
        "pet:snowball",
        "travel crate booked in the cabin, not the hold",
    )
    .await;
    // The three claims above had nowhere honest to live while a cat was a
    // thing, so the beat that proves the kind arrived is that they are on her
    // page and readable back off it.
    s.shape(
        "what has to happen before the cat can fly",
        json!({ "subject": "pet:snowball", "facts": true }),
    )
    .await
    .says("import permit")
    .says("travel crate");

    s.add("event:departure-flight", "Departure").await;
    s.fact(
        "event:departure-flight",
        "one-way, booked once the visa lands",
    )
    .await;

    // The occurrence is a record on that entity: the fields a flight has are
    // values, and `refs` says who is on it.
    let flight = s
        .event_with(
            "event:departure-flight",
            "the flight the family is booked on",
            json!({"departs_on": "2027-02-09", "one_way": "yes"}),
            &["person:tulio"],
        )
        .await;
    s.recall("event:departure-flight")
        .await
        .claim(&flight)
        .says("2027-02-09")
        .says("person:tulio");

    // The other thing on the calendar, and nobody gave it a day under a key —
    // which is what makes the read below mean anything.
    s.add("event:leaving-party", "The Leaving Party").await;
    s.fact("event:leaving-party", "the weekend before they fly")
        .await;

    // **"What is happening in February" reads the EVENTS, not their claims.**
    // The day is on a record and the thing's fields are its records folded, so
    // the date is on the event itself; declaring the type is what makes the
    // window comparable.
    s.call(
        "declare_type",
        json!({
            "name": "departure",
            "fields": [{ "key": "departs_on", "holds": "date" }],
        }),
    )
    .await
    .says("\"name\":\"departure\"");
    let february = s
        .shape(
            "what is happening in February",
            json!({
                "kind": "event",
                "facts": false,
                "fields": [
                    { "key": "departs_on", "compare": "after", "value": "2027-01-31" },
                    { "key": "departs_on", "compare": "before", "value": "2027-03-01" },
                ],
            }),
        )
        .await;
    february.says("event:departure-flight");
    february.says("\"departs_on\":\"2027-02-09\"");

    // GAP — and the answer is only the events somebody keyed. The party is as
    // real a thing in February as the flight and it carries no day under any
    // key, so nothing reaches it: an entity has no date of its own, and nothing
    // asks for one when an event is created. The question is one read now and
    // what it answers over is what a session remembered to file.
    //   s.happens_on("event:leaving-party", "2027-02-06").await;
    february.never_says("event:leaving-party");
    s.list("event").await.never_says("\"happens_on\"");

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

    // The question actually asked at this point, and the two records that were
    // filed with a state answer it: one read, and the sentences come back
    // filtered out rather than judged one by one.
    //
    // **Asked of the record, for the reason the January read is** — these are
    // rows on one project rather than things of their own, so what is open is a
    // question about the rows. It is the same gap, seen from the other side.
    let still_open = s
        .shape(
            "what is still open on the move",
            json!({
                "fields": [{ "key": "state", "value": "open", "scope": "record" }],
                "facts": true,
            }),
        )
        .await;
    still_open.says("visa photo");
    still_open.says("embassy appointment");
    // What the read cannot see, in the same answer: the four lines that went in
    // as sentences are as outstanding as the two above and carry no state, so
    // "what is still open" answers over what somebody thought to file that way.
    still_open.never_says("watch prices");

    // GAP — so the answer is only as good as the filing, and nothing makes the
    // filing happen. Every filter says which records to KEEP, so the four
    // stateless lines cannot be asked for BY their absence either: "what is
    // outstanding and nobody has said so" is a question about a key that is not
    // there.
    //   s.shape("the work carrying no state",
    //           json!({"fields": [{"key": "state", "missing": true}]})).await;
    s.has_no_argument("recall", "missing", &["fields", "key"])
        .await;

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
    s.recall("project:atlas")
        .await
        .claim(&first_date)
        .says("superseded")
        .never_says("superseded_by");

    s.wrap("still in progress").await;

    story.finish().await;
}
