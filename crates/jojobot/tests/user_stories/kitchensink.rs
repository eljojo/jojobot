//! Everything the surface serves that no other story needed.
//!
//! Every other file here is somebody trying to get something done, and what it
//! exercises is whatever that took. This one is not a persona and does not
//! pretend to be: it exists so the remainder is not zero-covered. A capability
//! with no natural home is still a capability, and a feature no story touches
//! is a feature nobody would notice losing.
//!
//! It is a thin walk on purpose. It is **not** an assertion-free one: a beat
//! that only proves a call did not error has covered nothing, so each one below
//! says what came back and, where the wrong answer would be an empty one, says
//! what did not come back beside it.
//!
//! When a beat here starts belonging somewhere — when a real persona needs it —
//! move it there. This file shrinking is the good outcome.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn nothing_on_the_surface_goes_unexercised() {
    let story = Story::begin("bot:otto").await;

    // ── an entity carrying the fields nobody's story needed ─────────────────
    let s = story.session().await;

    // `crm` cross-links the entity to wherever the task layer addresses it, and
    // `boot` marks it as part of the core an assistant loads every session
    // rather than something fetched when the conversation reaches for it.
    s.call(
        "add_entity",
        json!({
            "kind": "bot", "handle": "epsilon", "name": "Epsilon",
            "source": "user-named", "crm": "card-4711", "boot": "always",
        }),
    )
    .await
    .says("card-4711")
    .says("\"boot\":\"always\"");

    // The default is the other one, so the field above is a choice rather than
    // the only thing the column can say.
    s.add("person:homer", "Homer").await;
    s.list("person").await.says("\"boot\":\"on-demand\"");

    // ── a near miss, and the token that lifts exactly it ────────────────────

    // A new handle whose tokens sit inside one that exists is refused with the
    // candidates, and nothing is written.
    let refusal = s
        .refused(
            "add_entity",
            json!({
                "kind": "person", "handle": "homer-simpson", "name": "Homer Simpson",
                "source": "user-named",
            }),
        )
        .await;
    refusal.says("person:homer");
    s.list("person")
        .await
        .says("person:homer")
        .never_says("person:homer-simpson");

    // The refusal mints a token, and sending it back is how a caller says it
    // read the candidates and judged them a different person.
    let token = refusal.advised("override_token");
    s.call(
        "add_entity",
        json!({
            "kind": "person", "handle": "homer-simpson", "name": "Homer Simpson",
            "source": "user-named", "override_token": token,
        }),
    )
    .await
    .says("person:homer-simpson");
    s.list("person").await.says("person:homer-simpson");

    // ── an entity edited on the axes a rename does not touch ────────────────

    // `crm` and `source` move without going near the label, so no gate is
    // involved: the guard screens what an entity will be CALLED.
    s.call(
        "update_entity",
        json!({"handle": "person:homer", "crm": "card-8150", "source": "crm-card"}),
    )
    .await
    .says("card-8150")
    .says("\"source\":\"crm-card\"");
    s.list("person").await.says("card-8150");

    // An edit replaces the alias list whole, and a name added that way finds
    // the person exactly as one given at creation does.
    s.call(
        "update_entity",
        json!({"handle": "person:homer", "aliases": ["Dad"]}),
    )
    .await
    .says("Dad");
    s.find("Dad").await.says("person:homer");

    // A rename INTO a name something else already answers to meets the same
    // gate a creation does, and the same token lifts it.
    let relabel = s
        .refused(
            "update_entity",
            json!({"handle": "person:homer-simpson", "name": "Homer"}),
        )
        .await;
    relabel.says("person:homer");
    s.call(
        "update_entity",
        json!({
            "handle": "person:homer-simpson", "name": "Homer",
            "override_token": relabel.advised("override_token"),
        }),
    )
    .await
    .says("person:homer-simpson");

    s.wrap("the entity fields, and the token that gets past a near miss")
        .await;

    // ── a claim's provenance, moved after the fact ──────────────────────────
    let s = story.session().await;

    // Demotion is free: taking back the claim that somebody said a thing needs
    // nobody's confirmation, because it removes an authority rather than
    // adding one. Promotion is the gated direction, and `unsure` covers it.
    let told = s.fact("person:homer", "walks to work").await;
    s.call(
        "update_fact",
        json!({"address": &told, "provenance": "inference"}),
    )
    .await
    .says("\"provenance\":\"inference\"");
    s.recall("person:homer")
        .await
        .claim(&told)
        .says("\"provenance\":\"inference\"")
        .never_says("\"provenance\":\"testimony\"");

    // Details move on an existing claim the way content does, and an empty
    // string is what clears them — which is why they are not merely omitted.
    s.call(
        "update_fact",
        json!({"address": &told, "details": "he says it takes twenty minutes"}),
    )
    .await
    .says("twenty minutes");
    s.recall("person:homer")
        .await
        .claim(&told)
        .says("twenty minutes");
    s.call("update_fact", json!({"address": &told, "details": ""}))
        .await;
    s.recall("person:homer")
        .await
        .claim(&told)
        .says("\"content\":\"walks to work\"")
        .never_says("twenty minutes");

    // ── the search filters no story asked in ────────────────────────────────

    // `subject` narrows to one entity's claims without a word to match on, so
    // the question is structural rather than textual.
    let elsewhere = s.fact("person:homer-simpson", "walks to work too").await;
    s.call("search", json!({"subject": "person:homer"}))
        .await
        .says(&told)
        .never_says(&elsewhere);

    // `status` reaches what a default search deliberately leaves out.
    let moved_past = s
        .fact("person:homer", "worked nights until the spring")
        .await;
    s.supersede(&moved_past).await;
    s.call("search", json!({"status": "superseded"}))
        .await
        .says(&moved_past)
        .never_says(&told);

    // `limit` sizes the answer, and `count` is what came back rather than what
    // matched — this surface has no truncation marker at all, so a caller who
    // wants to know there was more asks a narrower question. (`not_shown`
    // belongs to `list_sent`, not here.)
    s.call("search", json!({"query": "walks to work", "limit": 1}))
        .await
        .says("\"count\":1");

    // ── an argument this surface does not have, at either level ─────────────

    // The positive both refusals rest on: a well-formed `edge` walks the graph
    // and comes back with the member. Without it, "the call was refused" would
    // read the same on a build where every edge is refused.
    s.add("org:springfield-cyclery", "Springfield Cyclery")
        .await;
    s.fact_about(
        "person:homer",
        "joined in the spring",
        "membership",
        "org:springfield-cyclery",
    )
    .await;
    s.through("membership", "org:springfield-cyclery", "person")
        .await
        .says("person:homer");

    // An argument the verb does not implement is refused by name, and nothing
    // runs.
    s.refused("search", json!({"query": "cyclery", "weight": 3}))
        .await
        .says("weight");

    // …and so is one inside a sub-object, named by the path that says which
    // level it sat at. A caller told only `weight` would go looking at the
    // wrong one.
    s.refused(
        "search",
        json!({
            "edge": {
                "shape": "membership", "object": "org:springfield-cyclery", "weight": 3,
            },
        }),
    )
    .await
    .says("edge.weight");

    // ── how far a walk went, and where it stopped ───────────────────────────
    //
    // Homer belongs to the club and the club is somewhere, so an outbound walk
    // of two hops has somewhere to go twice. One hop reaches the club; two
    // reaches the town it is in.
    s.add("place:shelbyville", "Shelbyville").await;
    s.fact_about(
        "org:springfield-cyclery",
        "the workshop is over there",
        "location",
        "place:shelbyville",
    )
    .await;

    let one = s
        .shape(
            "what Homer is connected to",
            json!({"subject": "person:homer", "follow": {"direction": "out"}, "facts": false}),
        )
        .await;
    one.says("org:springfield-cyclery")
        .never_says("place:shelbyville");

    // **And the club says its own edges were not followed**, rather than
    // coming back with an empty `connected` that a reader would take for "the
    // club is connected to nothing". The note names the way to the rest.
    one.says("\"unwalked\"").says("deeper follow");

    s.shape(
        "what Homer is connected to, and what that is connected to",
        json!({
            "subject": "person:homer",
            "follow": {"direction": "out", "depth": 2},
            "facts": false,
        }),
    )
    .await
    .says("org:springfield-cyclery")
    .says("place:shelbyville");

    s.wrap("provenance moved, and the three filters that need no words")
        .await;

    // ── where a sender's mail got to ────────────────────────────────────────
    let s = story.session().await;

    s.post("epsilon", "First", "One for epsilon.").await;
    s.post("assistant", "Second", "One for the default identity.")
        .await;

    // Every box this sender has posted into, and no delivery taken.
    s.call("list_sent", json!({}))
        .await
        .says("First")
        .says("Second");

    // `to` narrows to one colleague.
    s.call("list_sent", json!({"to": "epsilon"}))
        .await
        .says("First")
        .never_says("Second");

    // `sender` says WHOSE outgoing mail to read, matched exactly against the
    // handle on each message. It has to name a bot that is NOT this one to
    // prove anything — omitting it already answers with your own — and reading
    // another sender's is allowed, because where a message got to is not
    // private to whoever wrote it.
    let other = story.as_bot("bot:epsilon").await;
    other
        .post("assistant", "Third", "One from another sender.")
        .await;
    other
        .wrap("posted once, so there is somebody else's outbox")
        .await;
    s.call("list_sent", json!({"sender": "bot:epsilon"}))
        .await
        .says("Third")
        .never_says("First")
        .never_says("Second");

    // `limit` sizes it the way it sizes a search, newest first.
    s.call("list_sent", json!({"limit": 1}))
        .await
        .says("Second")
        .never_says("First");

    // `include_bodies` ships the text back. It is off by default because the
    // sender wrote it, so the useful answer is where it got to.
    s.call("list_sent", json!({}))
        .await
        .says("\"body_elided\":true")
        .says("\"body\":null");
    s.call("list_sent", json!({"include_bodies": true}))
        .await
        .says("\"body\":\"One for epsilon.\"");

    s.wrap("the sender's own view of its outgoing mail").await;

    // ── the door, asked for something that is not there ─────────────────────
    //
    // **These are the answers a cold session meets when it guesses wrong**, and
    // each of them is the difference between a surface that teaches and one
    // that breaks. They have no natural persona: nobody sets out to name a bot
    // that does not exist. They are here because the alternative is that the
    // only end-to-end coverage of them is a playbook a person has to pay to
    // run.

    // A boot that names nobody orients and starts nothing. The answer carries
    // the world; what it must NOT carry is a handle, because a caller with no
    // identity has nothing to write with.
    let (oriented, no_handle) = story.call("start_here", json!({})).await;
    oriented.says("\"snapshot\"").says("\"skills\"");
    assert!(
        no_handle.is_none(),
        "an anonymous boot handed back a session handle: {}",
        oriented.raw(),
    );

    let s = story.session().await;

    // A name that is no bot is an answer with the roster on it, not an error
    // and not an identity. Both halves: the refusal names a bot that does
    // exist, and the listing afterwards does not carry the name that was asked
    // for.
    s.refused("start_here", json!({"bot": "zzz-nobody"}))
        .await
        .says("bot:otto");
    s.list("bot").await.never_says("zzz-nobody");

    // A skill that is no skill is refused by the same door, naming one that is
    // real — so a session that guessed a procedure's name learns the ones it
    // could have asked for.
    s.refused("start_here", json!({"skill": "zzz-not-a-skill"}))
        .await
        .says("rhythms");

    // A search that narrows nothing is refused rather than answered with
    // everything or with a cheerful empty list, and the refusal names what was
    // missing.
    s.refused("search", json!({})).await.says("query");

    // ── the run that has been told, and the verbs that are not there ────────

    // A wrapped run is the last word: its story is told, and an entry after it
    // is refused rather than appended somewhere nobody reads. The positive it
    // rests on is the write before it, which the same session made into the
    // same run.
    s.journal("about to stop, and the record says so").await;
    s.call("wrap_session", json!({"story": "said what happened"}))
        .await
        .says("said what happened");
    s.refused("journal", json!({"entry": "one more, after the end"}))
        .await;

    let s = story.session().await;

    // **Nothing over this surface deletes.** Named as a class rather than as
    // three guesses: a session that believes it might be able to remove
    // something will eventually try, and what it meets has to be the absence of
    // the whole family.
    s.has_no_verb_containing("delete", &["add_entity", "retract"])
        .await;
    s.has_no_verb_containing("remove", &["update_fact", "mark_processed"])
        .await;

    // And no verb opens a mailbox. A box arrives with the bot that owns it, in
    // the one act — so the way to a new box is a new identity, and standing one
    // up to file a note is not a move anybody makes on their own.
    for asked in ["create_mailbox", "open_mailbox", "add_mailbox"] {
        s.has_no_verb(asked, &["post_message", "read_mailbox"])
            .await;
    }

    s.wrap("what the surface answers when it is asked for what it does not have")
        .await;

    story.finish().await;
}
