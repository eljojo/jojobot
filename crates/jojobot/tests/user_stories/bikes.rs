//! "Keep track of my bikes — what they are, what's been done to them, how much
//! I ride them, and what's still under warranty."
//!
//! A thing with a long life rather than a project with an end, so what it
//! probes is accumulation: the same measurement year after year, parts
//! replaced under it, and the questions asked months later.
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn keeping_track_of_bikes() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · what they are ───────────────────────────────────────────
    let s = story.session().await;

    s.add("thing:gravel-bike", "Gravel Bike").await;
    s.fact("thing:gravel-bike", "ridden most weeks").await;

    s.fact_on("thing:gravel-bike", "purchased new", "2024-04-11")
        .await;
    s.fact(
        "thing:gravel-bike",
        "frame warranty runs five years from purchase",
    )
    .await;

    // That date went into the only date field a plain claim has, which means
    // when the claim became known. The two ARE separable: an occurrence goes
    // under a key of its own on a typed record, where its meaning is the key
    // name rather than the sentence beside it, and the service in session 2
    // goes in that way. Nothing forces it, so a claim written this way still
    // holds one date doing both jobs.

    // Purchase plus five years is arithmetic, and the arithmetic is the
    // session's — but its ANSWER goes in as a value under a key rather than as
    // another sentence, so the date the question turns on is somewhere a
    // question can reach it.
    s.event_with(
        "thing:gravel-bike",
        "frame warranty",
        json!({"expires": "2029-04-11"}),
        &[],
    )
    .await;
    // **When the cover runs out is a question about the BIKE, so the read that
    // answers it asks for the bike and not for everything ever said about it.**
    // A thing comes back as its records' fields folded into one row — one value
    // per key — and that row is the answer here. The records are behind it and
    // the answer says how many, so nothing is hidden and nothing is shipped
    // unasked.
    let bike = s
        .shape(
            "what the bike is, without every claim ever made about it",
            json!({ "subject": "thing:gravel-bike", "facts": false }),
        )
        .await;
    bike.says("\"expires\":\"2029-04-11\"");
    bike.says("records are behind these fields");
    // The sentence that needed a whole read is not in this answer, which is the
    // point of asking this way rather than the old one.
    bike.never_says("frame warranty runs five years from purchase");

    // …and the sentence is still there for the read that wants it, so the dense
    // answer left it out rather than losing it.
    s.recall("thing:gravel-bike")
        .await
        .says("frame warranty runs five years from purchase")
        .says("2029-04-11");

    s.add("thing:road-bike", "Road Bike").await;
    s.fact(
        "thing:road-bike",
        "hanging in the basement, unridden for two years",
    )
    .await;
    // The other side of the question below: a bike whose cover has already run
    // out. Without one, "what is still under warranty" would come back with
    // everything and look like an answer.
    s.event_with(
        "thing:road-bike",
        "frame warranty",
        json!({"expires": "2025-06-30"}),
        &[],
    )
    .await;
    // The claim stays one crisp line, and the nuance that would ruin it as a
    // claim rides beside it instead of being folded into the sentence.
    s.call(
        "capture",
        json!({
            "subject": "thing:road-bike",
            "content": "needs tyres before it can be sold",
            "details": "the rear rim is worn too, so tyres alone may not be enough",
            "provenance": "testimony",
        }),
    )
    .await;
    s.recall("thing:road-bike")
        .await
        .says("\"content\":\"needs tyres before it can be sold\"")
        .says("the rear rim is worn too");

    // Both of those are STATES rather than descriptions, and a state has
    // somewhere to go that is not a sentence: it rides a record under a key of
    // its own, where a question can ask for it by value. The two claims above
    // went in as prose, which is why neither answers the read below.
    let for_sale = s
        .event_with(
            "thing:road-bike",
            "listed on the club noticeboard",
            json!({"state": "for sale"}),
            &[],
        )
        .await;
    let selling = s
        .shape(
            "the things that are up for sale",
            json!({ "fields": [{ "key": "state", "value": "for sale" }] }),
        )
        .await;
    selling.says("thing:road-bike");
    // The bike that is not for sale, in the same answer that just proved the
    // read is not empty.
    selling.never_says("thing:gravel-bike");

    // GAP — and nothing moves it. A record is current truth rewritten in place,
    // so the day the bike sells the key is overwritten and what it said before
    // is gone: "sold in March" and "for sale since March" read the same
    // afterwards, and no read asks what a state used to be. What the key buys
    // is the question, not the passage of the thing through it.
    //   s.moved(&for_sale, "sold", on: "2027-03-02").await;
    s.correct_fields(&for_sale, json!({"state": "sold"}), &[])
        .await;
    s.recall("thing:road-bike")
        .await
        .claim(&for_sale)
        .says("sold")
        .never_says("for sale");

    s.wrap("both bikes recorded").await;

    // ── session 2 · the shop, and the work it did ───────────────────────────
    let s = story.session().await;

    s.add("org:springfield-cyclery", "Springfield Cyclery")
        .await;
    s.add("person:milhouse", "Milhouse").await;
    s.fact_about(
        "person:milhouse",
        "the mechanic who actually knows the bike",
        "membership",
        "org:springfield-cyclery",
    )
    .await;
    s.fact(
        "thing:gravel-bike",
        "chain and cassette both replaced last spring at the cyclery",
    )
    .await;

    // Who works where IS walkable, so the shop's people come back in one call
    // without knowing their names first.
    s.through("membership", "org:springfield-cyclery", "person")
        .await
        .says("person:milhouse");

    // A service goes down as fields on a record: what was done and when, as
    // values, with `refs` naming who did it. "What has been done to this bike"
    // is one read of its record.
    let service = s
        .event_with(
            "thing:gravel-bike",
            "annual service",
            json!({"done_on": "2026-04-18", "work": "chain, cables, bearings"}),
            &["person:milhouse"],
        )
        .await;
    s.recall("thing:gravel-bike")
        .await
        .claim(&service)
        .says("2026-04-18")
        .says("person:milhouse");

    // The shop calls back: the bearings were not touched, and nobody wrote
    // down the day after all. The record is edited where it stands — the key
    // that was wrong is rewritten, the key that was guessed is taken off, and
    // the keys nobody mentioned stay as they were.
    s.correct_fields(&service, json!({"work": "chain, cables"}), &["done_on"])
        .await;
    // **The old value is named as gone**, not merely the new one as present:
    // "chain, cables" is a substring of "chain, cables, bearings", so the
    // positive half alone passes on a build where nothing was rewritten.
    s.recall("thing:gravel-bike")
        .await
        .claim(&service)
        .says("chain, cables")
        .says("person:milhouse")
        .never_says("bearings")
        .never_says("2026-04-18");

    // **The day nobody wrote is off the record and not gone from it.** Every
    // write of a key is kept, so the value that was there and the moment it was
    // taken off are both on the key's history — each write naming the record it
    // arrived in, that record's date and its status. A record read plainly says
    // what the service is now; this says what the service has been.
    let dates = s
        .shape(
            "every write of the service date",
            json!({ "subject": "thing:gravel-bike", "history": "done_on" }),
        )
        .await;
    dates
        .says("\"key\":\"done_on\"")
        .says("\"count\":2")
        .says(&format!("\"record\":\"{service}\""))
        .says("\"status\":\"active\"")
        .says("2026-04-18")
        // The clearing write says so in its own right rather than by a null the
        // reader has to know the meaning of.
        .says("\"cleared\":true");

    // GAP — and no read orders the RECORDS or takes the newest one. A key's
    // writes come back in order, which is the history above; "when did I last
    // service it" is a question about the records, and it comes back as every
    // service ever recorded with the session picking the latest date out by
    // reading them. Ordering is how an answer comes back rather than a question
    // of its own, so it arrives as an argument on the read and the tripwire
    // watches for both halves of it.
    //   s.shape("the newest service",
    //           json!({"subject": "thing:gravel-bike", "order": "expires", "newest": 1})).await;
    s.has_no_argument("recall", "order", &["fields", "facts"])
        .await;
    s.has_no_argument("recall", "newest", &["fields", "facts"])
        .await;

    // GAP — the chain is a PART of the bike, not a fact about it. Parentage is
    // not reachable, so it cannot be its own thing with its own history, and
    // "how many km on the chain since I fitted it" has nothing to hang on.
    //   s.add_under("thing:gravel-bike", "thing:bike-chain", "Chain").await;
    //
    // Asking for it anyway is refused, and the refusal names the argument.
    // The alternative is worse than the gap: a flat entity created and success
    // reported, which leaves a caller unable to tell "parentage is not on the
    // surface yet" from "I set it and it worked".
    s.refused(
        "add_entity",
        json!({
            "kind": "thing", "handle": "bike-chain", "name": "Chain",
            "source": "user-named", "parent": "thing:gravel-bike",
        }),
    )
    .await
    .says("parent")
    // A blocked ANSWER with a way forward, not a schema error thrown back at
    // the client: `wrote` is the field only the refusal carries, and a
    // deserializer failing would never reach it.
    .says("\"wrote\":false");
    s.list("thing").await.never_says("thing:bike-chain");

    // The same creation without it lands, so the refusal is about the argument
    // and not about the entity. The chain goes in as its own thing and lands
    // flat: nothing on the created record says what it is part of.
    s.add("thing:bike-chain", "Chain").await;
    s.list("thing")
        .await
        .says("thing:bike-chain")
        .never_says("parent");

    s.wrap("service history, such as it is").await;

    // ── session 3 · the numbers, year after year ────────────────────────────
    let s = story.session().await;

    // The same measurement at three sittings, a year apart. It goes under a
    // key rather than into a sentence, which is what makes the three of them
    // one set instead of three remarks that happen to mention kilometres.
    let mut tallies = Vec::new();
    for (year, km) in [("2024", "2600"), ("2025", "3800"), ("2026", "4100")] {
        tallies.push(
            s.event_with(
                "thing:gravel-bike",
                &format!("the year's tally for {year}"),
                json!({"km": km, "year": year}),
                &[],
            )
            .await,
        );
    }
    // The other bike, ridden none of it. Without it "the tallies came back"
    // and "everything came back" are the same answer.
    s.event_with(
        "thing:road-bike",
        "the year's tally for 2026",
        json!({"km": "0", "year": "2026"}),
        &[],
    )
    .await;

    // **The two questions one key answers, and the operator asks both.** How
    // far has this bike been ridden — one value, the newest write, folded from
    // every record about the bike. It is on the page he opens himself, where
    // the year each tally was written for sits beside it.
    let page = story.page("thing:gravel-bike").await;
    let fields = page.section("fields");
    fields.says("km").says("4100");
    // The two tallies before it are not the answer to that question, and the
    // fold says so by leaving them out.
    fields.never_says("2600").never_says("3800");

    // **And the key itself is the way to the other question.** He does not
    // know a query string and should not have to: what the fold shows is a
    // link, so the page he is on carries the way to the page behind it. The
    // address is read off that link rather than assembled here, which is the
    // difference between following the page and rehearsing it.
    let opened = story.follow(&fields.link_to("km")).await;

    // Every tally he ever wrote, oldest first, each with the record that
    // carried it. **Read out of the writes table alone** — the current value
    // is on the same page in the fold above, so a search of the whole page
    // finds 4,100 there and calls it a history.
    let writes = opened.section("history");
    writes
        .says("2600")
        .says("3800")
        .says("4100")
        .says(&tallies[0])
        .says("active");
    let each_year = writes.raw();
    assert!(
        each_year.find("2600") < each_year.find("3800")
            && each_year.find("3800") < each_year.find("4100"),
        "the writes are listed oldest first, or the page cannot say which way \
         the numbers went: {each_year}"
    );

    // …and the fold on that same page still answers the first question. Both
    // halves in one read: the history is there AND it has not replaced what
    // the key holds now.
    let still = opened.section("fields");
    still.says("4100");
    still.never_says("2600").never_says("3800");

    // …and how much each year, which is the SAME key asked as every time it
    // was written: oldest first, each write carrying the record it arrived in
    // and that record's date, and the count is the answer to how many years
    // there are.
    let ridden = s
        .shape(
            "every tally ever written for the bike",
            json!({ "subject": "thing:gravel-bike", "history": "km" }),
        )
        .await;
    ridden
        .says("\"key\":\"km\"")
        .says("\"count\":3")
        .says(&format!("\"record\":\"{}\"", tallies[0]));
    // **Read out of the history itself, and asserted as an ORDER.** Every one
    // of these numbers is also on the records above, so a search of the whole
    // answer for them passes on a build that hands the writes back in any
    // arrangement at all — which is the difference between three values being
    // present and a reader being able to tell which way they went.
    let answered: serde_json::Value =
        serde_json::from_str(ridden.raw()).expect("the answer is json");
    let each_year: Vec<&str> = answered["objects"][0]["history"]["writes"]
        .as_array()
        .expect("the writes behind the key")
        .iter()
        .map(|write| write["value"].as_str().expect("a write carries its value"))
        .collect();
    assert_eq!(
        each_year,
        ["2600", "3800", "4100"],
        "the writes come back oldest first: {}",
        ridden.raw()
    );

    // And the bike that was ridden none of it keeps its own history, so the
    // key is answered per thing rather than across the store.
    s.shape(
        "every tally ever written for the other bike",
        json!({ "subject": "thing:road-bike", "history": "km" }),
    )
    .await
    .says("\"count\":1")
    .never_says("4100");

    // The ordinary read is untouched: a call that names no key carries no
    // history at all, so the session that wants what the bike is now pays for
    // none of this.
    s.recall("thing:gravel-bike")
        .await
        .says("the year's tally for 2026")
        .never_says("\"history\"");

    s.wrap("tallies in, and what each year came to").await;

    // ── session 4 · months later, the questions actually asked ──────────────
    let s = story.session().await;

    s.find("warranty")
        .await
        .says("thing:gravel-bike")
        .says("thing:road-bike");
    s.find("cassette").await.says("thing:gravel-bike");

    // The title question — and the search above is not it. The word is on both
    // bikes and it cannot say which cover has run out.
    //
    // Declaring `warranty` is what makes the stored date orderable, and it is
    // retroactive — the two records were written three sessions ago by a
    // client that had never heard of the type.
    s.call(
        "declare_type",
        json!({
            "name": "warranty",
            "fields": [{ "key": "expires", "holds": "date" }],
        }),
    )
    .await
    .says("\"name\":\"warranty\"");

    let covered = s
        .shape(
            "what is still under warranty",
            json!({ "fields": [{ "key": "expires", "compare": "after", "value": "2026-08-01" }] }),
        )
        .await;
    covered.says("thing:gravel-bike");
    covered.never_says("thing:road-bike");

    s.recall("thing:road-bike").await.says("basement");
    s.list("thing").await.says("thing:gravel-bike");

    // Custody is a fact: the pump is the entity, "loaned to" is the fact, and
    // it points at the person. A fact is current truth rewritten in place,
    // which is right — where a thing is now has no business accumulating.
    s.add("thing:floor-pump", "Floor Pump").await;
    s.fact_about(
        "thing:floor-pump",
        "loaned out, still not back",
        "connection",
        "person:milhouse",
    )
    .await;
    s.find("loaned").await.says("thing:floor-pump");
    s.recall("thing:floor-pump").await.says("person:milhouse");

    // The shape is `connection`, which says a link is there and that how it
    // relates was not recorded. That is the honest shape for a loan and it is
    // no name for one — and the traffic goes both ways, which is what makes
    // the difference bite: his torque wrench is here, pointed at him by the
    // very same shape, and it is the opposite arrangement.
    s.add("thing:torque-wrench", "Torque Wrench").await;
    s.fact_about(
        "thing:torque-wrench",
        "his, borrowed for the bottom bracket and not given back yet",
        "connection",
        "person:milhouse",
    )
    .await;
    let linked = s.through("connection", "person:milhouse", "thing").await;
    linked.says("thing:floor-pump");
    linked.says("thing:torque-wrench");

    // So "what have I lent out, and to whom" needs the link NAMED, and a key
    // is where a name goes: `loaned_to` holds his handle, and declaring it to
    // hold a reference is what turns the key into a relation the query can
    // travel.
    //
    // It goes on a record of its own rather than on the claim above, so the
    // lending is one thing and what the pump is stays another beside it.
    s.event_with(
        "thing:floor-pump",
        "lent out at the spring service",
        json!({"loaned_to": "person:milhouse"}),
        &[],
    )
    .await;
    s.call(
        "declare_type",
        json!({
            "name": "loan",
            "fields": [{ "key": "loaned_to", "holds": "reference" }],
        }),
    )
    .await
    .says("\"name\":\"loan\"");

    // Walked inbound from him: what he has of mine, which is not what I have
    // of his. The wrench is in the untyped walk above and out of this one, and
    // that pair is the whole difference between a link and a named link.
    let lent = s
        .shape(
            "what he has of mine",
            json!({
                "subject": "person:milhouse",
                "facts": false,
                "follow": { "relation": "loaned_to", "direction": "in" },
            }),
        )
        .await;
    lent.says("thing:floor-pump");
    lent.never_says("thing:torque-wrench");

    s.wrap("still riding").await;

    // ── session 5 · one of them is sold ─────────────────────────────────────
    let s = story.session().await;

    s.fact("thing:road-bike", "sold — tyres thrown in, gone in March")
        .await;

    // Everything ever recorded about it is still current truth about a bike
    // that is no longer here, and still comes back in an ordinary read.
    s.recall("thing:road-bike")
        .await
        .says("sold")
        .says("hanging in the basement")
        .says("needs tyres before it can be sold");
    s.list("thing").await.says("thing:road-bike");

    // GAP — an entity has no end. Nothing says this one has left the
    // operator's life, so it goes on answering "what do I own", and its facts
    // go on reading as present tense. The alternatives available today are
    // both wrong: rewrite each claim into the past, which destroys the record,
    // or leave them, which is what happened here.
    //   s.closed("thing:road-bike", "sold in March").await;
    s.has_no_verb("close_entity", &["update_entity", "list_entities"])
        .await;

    s.wrap("one sold, and the record cannot tell").await;

    story.finish().await;
}
