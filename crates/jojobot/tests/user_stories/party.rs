//! "I want to throw a birthday party. Help me work out who comes, what to
//! cook, and where."
//!
//! What it probes is what a graph should be best at: a set of people whose
//! state changes, a walk from an event to its guests to what they eat, and one
//! person reachable only through another.
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn throwing_a_birthday_party() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · the idea ────────────────────────────────────────────────
    let s = story.session().await;

    s.add("person:bodoque", "Bodoque").await;
    s.add("event:birthday-party", "Bodoque's Birthday").await;
    s.fact_about(
        "event:birthday-party",
        "a birthday party for them, ten or twelve people",
        "about",
        "person:bodoque",
    )
    .await;

    s.add("place:moes", "Moe's").await;
    s.fact(
        "event:birthday-party",
        "either Moe's or my place; not decided",
    )
    .await;
    s.fact(
        "event:birthday-party",
        "asking people whether the 14th or the 21st works",
    )
    .await;

    // GAP — two venues and two dates, both under evaluation, and choosing is
    // what planning mostly is. Holding the candidates is served: a key
    // declared to hold a reference points each venue at the party, and the
    // walk back from the party returns them as a set. Ranking them is not, and
    // neither is asking which are still in play — ruling one out is
    // recordable, and every filter says which records to KEEP, so the ones
    // carrying no such record cannot be asked for.
    //   s.shortlist("the venue", &["place:moes"]).await;
    s.has_no_verb("shortlist", &["capture", "search"]).await;

    // The day is not missing either, though the party has not got one here. A
    // claim's own date is still when it became known, and the day a thing
    // happens on goes under a key of its own on a typed record, where a
    // declared date is comparable. What carries no date is the ENTITY, and the
    // moving story marks that.

    s.wrap("party sketched").await;

    // ── session 2 · the guests, and what they need ──────────────────────────
    let s = story.session().await;

    s.add("person:patana", "Patana").await;
    // Everybody says the nickname and nobody says the name on the record, so
    // the record carries both. An alias is another label on one person, not a
    // second person to keep in step.
    s.call(
        "add_entity",
        json!({
            "kind": "person", "handle": "barney-gumble", "name": "Barney",
            "aliases": ["Barn"], "source": "user-named",
        }),
    )
    .await;
    s.add("person:ned-flanders", "Ned").await;

    // The name the operator actually says finds them.
    s.find("Barn").await.says("person:barney-gumble");

    for guest in [
        "person:patana",
        "person:barney-gumble",
        "person:ned-flanders",
    ] {
        s.fact_about(
            guest,
            "invited to the party",
            "attendance",
            "event:birthday-party",
        )
        .await;
    }

    let patana_eats = s.fact("person:patana", "vegetarian").await;
    s.fact("person:barney-gumble", "does not drink").await;
    s.fact("person:ned-flanders", "bringing a partner").await;

    // The partner is a PERSON and the link to them is drawable: `connection`
    // takes any kind, so person-to-person is reachable.
    s.add("person:maude", "Maude").await;
    s.fact_about(
        "person:ned-flanders",
        "bringing their partner",
        "connection",
        "person:maude",
    )
    .await;
    s.through("connection", "person:maude", "person")
        .await
        .says("person:ned-flanders");

    // GAP — and the SHAPE says only that a link exists. The five are a closed
    // vocabulary, `relation` is not one of them, and every person-to-person
    // edge is therefore `connection`: partner, colleague and "does not get
    // along with" all draw the same one, so a walk by shape returns them
    // together.
    //   s.fact_about("person:ned-flanders", "their partner", "relation", "person:maude").await;
    s.refused(
        "capture",
        json!({
            "subject": "person:ned-flanders", "content": "their partner",
            "provenance": "testimony", "shape": "relation",
            "object": "person:maude",
        }),
    )
    .await
    .says("relation");
    // Telling them apart AT ALL is not what is missing. A key does that:
    // a declared `partner: reference` holding Maude's handle is a relation the
    // query walks in both directions, the same mechanism the pets story walks
    // `owner` by, and a person is as good a thing to point at as any other. So
    // the residual gap is the narrow one — the name lives on a KEY of the
    // record and never on the edge, and the two are separate instruments.
    // `connection` goes on meaning "how these two relate was not recorded",
    // whatever keys the record carrying it also holds.

    s.wrap("invitations out").await;

    // ── session 3 · the replies trickle in ──────────────────────────────────
    let s = story.session().await;

    // A reply is worth a KEY rather than only a sentence. The edge says these
    // two stand in an attendance relation and nothing more — it does not claim
    // anybody is there, any more than `location` claims somebody is still at a
    // place — so which way the reply went goes on the record as `answer`,
    // where a later question can ask for it.
    //
    // Not coming is information as load-bearing as coming, so it gets the same
    // edge and the same key.
    for (guest, said, answer) in [
        ("person:patana", "coming to the party", "yes"),
        (
            "person:barney-gumble",
            "cannot make it, away that weekend",
            "no",
        ),
    ] {
        s.call(
            "capture",
            json!({
                "subject": guest, "content": said, "provenance": "testimony",
                "shape": "attendance", "object": "event:birthday-party",
                "fields": {"answer": answer},
            }),
        )
        .await;
    }

    s.find("cannot make").await.says("person:barney-gumble");
    s.recall("person:barney-gumble")
        .await
        .says("away that weekend");

    // Asking the party for its guests returns everyone who relates to it, in
    // one call — the graph doing its job.
    let guests = s
        .through("attendance", "event:birthday-party", "person")
        .await;
    guests
        .says("person:patana")
        .says("person:barney-gumble")
        .says("person:ned-flanders");

    // And who actually said yes is its own question now, rather than a read
    // per guest and a judgement per read. The key carries the reply, so the
    // one who is coming and the one who is not stop coming back
    // indistinguishable.
    s.shape(
        "the guests who said yes",
        json!({"fields": [{"key": "answer", "value": "yes"}]}),
    )
    .await
    .says("person:patana")
    .never_says("person:barney-gumble");

    s.shape(
        "the guests who said no",
        json!({"fields": [{"key": "answer", "value": "no"}]}),
    )
    .await
    .says("person:barney-gumble")
    .never_says("person:patana");

    // GAP — but the one who has NOT answered is still not askable. Every
    // filter here says which records to keep, and "the guests carrying no
    // reply at all" is a question about a record that does not exist. It is a
    // filter, so it arrives as an argument on the read that filters, which is
    // what the tripwire watches.
    //   s.shape("the guests who have not replied",
    //           json!({"fields": [{"key": "answer", "missing": true}]})).await;
    s.has_no_argument("recall", "missing", &["fields", "key"])
        .await;

    s.wrap("two replies in, one outstanding").await;

    // ── session 4 · what to cook ────────────────────────────────────────────
    let s = story.session().await;

    // The venue went in under the short name everybody says. Giving it its
    // proper one edits the same entity rather than standing a second one
    // beside it: the handle says which entity to edit, and the label is what
    // changes.
    s.call(
        "update_entity",
        json!({"handle": "place:moes", "name": "Moe's Tavern"}),
    )
    .await
    .says("Moe's Tavern");
    s.list("place")
        .await
        .says("Moe's Tavern")
        .never_says("\"name\":\"Moe's\"");

    // The multi-hop, taken the long way first: who is attending, then what
    // each of them eats. Two calls, and the session holds the answer to the
    // first while it asks the second.
    s.through("attendance", "event:birthday-party", "person")
        .await
        .says("person:patana");
    s.recall("person:patana").await.says("vegetarian");
    s.find("vegetarian").await.says("person:patana");

    // **And then in ONE question.** "What do my guests eat" is the party, the
    // edges drawn at it, and each guest's own page — a walk that returns the
    // shape rather than a list the session has to walk itself.
    let table = s
        .shape(
            "the party, its guests, and what each of them eats",
            json!({
                "subject": "event:birthday-party",
                "facts": true,
                "follow": {"shape": "attendance", "direction": "in"},
            }),
        )
        .await;
    table
        .says("person:patana")
        .says("vegetarian")
        .says("does not drink")
        .says("bringing a partner");

    // The shape is the answer: the guests hang off the party rather than
    // arriving beside it, so the session reads who eats what without holding
    // anything.
    table.claim(&patana_eats).says("vegetarian");

    // ── somebody drops out, and the guest list has to show it ──────────────
    //
    // A guest who said yes and then did not come is the ordinary case, and it
    // is the one where a walk used to mislead: the attendance claim is taken
    // back, and the walk still reached the guest by an edge that looked exactly
    // like everybody else's. A session answering "who is coming" off the walk
    // read a withdrawn yes as a standing one.
    //
    // The claim is not hidden — hiding it would make somebody who dropped out
    // and somebody who was never asked the same answer — so the walk carries
    // both, and says which is which.
    let dropped = s
        .fact_about(
            "person:maude",
            "coming to the party",
            "attendance",
            "event:birthday-party",
        )
        .await;
    let guests = json!({
        "subject": "event:birthday-party",
        "follow": {"shape": "attendance", "direction": "in"},
    });
    s.shape("who is coming to the party", guests.clone())
        .await
        .says("person:maude")
        .never_says("retracted");
    s.retract(&dropped, "cannot make it after all — away that weekend")
        .await;
    let after = s.shape("who is coming to the party", guests.clone()).await;
    after.says("person:maude").says("retracted");
    // ── and the guest who was never there at all ────────────────────────────
    //
    // 🚨 **A different act, and the surface says so at the moment of the
    // write.** The party is over and somebody says a guest was never at it.
    // That is not a claim that changed — the past does not change — it is a
    // record that was never true, which is what `retract` says and keeps the
    // account of why. A rewrite is the ordinary verb and stays available; the
    // receipt names the other path rather than refusing.
    s.add("person:nelson", "Nelson").await;
    let mistaken = s
        .fact_about(
            "person:nelson",
            "was at the party",
            "attendance",
            "event:birthday-party",
        )
        .await;
    s.correct_reading_the_receipt(&mistaken, "was NOT at the party after all")
        .await
        .says("retract")
        // ⚠️ **And it says what the rewrite did not destroy**, which is the
        // reason a session had for writing nothing at all when it met a claim
        // it disagreed with.
        .says("history_record");

    // An ordinary correction, on the same store, carries neither line: a
    // receipt that always says it is one nobody reads.
    s.correct_reading_the_receipt(&patana_eats, "eats fish as well")
        .await
        .never_says("retract");

    // The guest whose yes still stands is still reached, so the retraction took
    // one link and not the walk. **Which link carries the marker is not
    // something a whole-answer assertion can say** — that is pinned per link in
    // the verb's own suite; here the pair that means something is the same read
    // before and after.
    after.says("person:patana");

    // ── and one guest was never coming at all ──────────────────────────────
    //
    // Different from dropping out. Ralph was written down as attending on a
    // misreading, so the claim is corrected to say the opposite — and a
    // correction stays active, because what was recorded was wrong rather than
    // something that should never have been recorded.
    //
    // Rewriting the sentence is not enough on its own: the attendance edge
    // stands behind it, and the guest list goes on counting him. The edge comes
    // off in the same edit.
    s.add("person:ralph", "Ralph").await;
    let misread = s
        .fact_about(
            "person:ralph",
            "coming to the party",
            "attendance",
            "event:birthday-party",
        )
        .await;
    s.shape("who is coming to the party", guests.clone())
        .await
        .says("person:ralph");
    s.correct_clearing_the_edge(&misread, "is not coming — that was somebody else")
        .await;
    s.shape("who is coming to the party", guests)
        .await
        .never_says("person:ralph")
        // The half that says the edit reached one claim and not the guest list:
        // everybody else is still on it.
        .says("person:patana");

    // ⚠️ **One claim's edge, not the person's.** Barney draws the same edge
    // from several claims, so clearing one of them would leave him on the list
    // — correctly. Unlinking somebody who was written down more than once is as
    // many edits as there are claims, and the guest above is on the list once.

    // The practical half, and the one actually worried about. It is not a
    // verb: "do I have enough chairs" is arithmetic over two things already
    // recorded, and the arithmetic is the session's.
    s.add("thing:folding-chairs", "Folding Chairs").await;
    s.fact(
        "thing:folding-chairs",
        "six of them, stacked in the basement",
    )
    .await;
    s.find("chairs").await.says("thing:folding-chairs");

    // GAP — but neither side of that sum is a number HERE. "Six of them" is
    // prose and so is "ten or twelve people", so the session gets two
    // sentences and parses quantities out of English it wrote itself. A count
    // does not have to be prose: a key declared to hold a number is compared
    // as one, so "the chairs, if there are fewer than ten" is a read. What no
    // read does is the sum — nothing adds, counts or totals, so the arithmetic
    // stays the session's whichever way the two went in.
    //   s.fact_keyed("thing:folding-chairs", "count", "6").await;
    s.has_no_verb("count_of", &["capture", "search"]).await;

    s.wrap("menu still open").await;

    story.finish().await;
}
