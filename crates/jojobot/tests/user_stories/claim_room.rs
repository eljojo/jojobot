//! "Can I write down a claim nobody could read back?"
//!
//! A read serves a record whole or not at all, and an answer has a ceiling. A
//! record that cannot fit one answer even alone would be handed back over the
//! ceiling, so the write that would make it is refused instead, with the ways to
//! say it. Each case finds the largest write that lands by asking, so the limit
//! is whatever the served surface enforces and no number is written down here.

use serde_json::{Value, json};

use jojobot_domain::text::ANSWER_CEILING;

use super::dsl::{Session, Story};

/// A slug of ten letters that depends on nothing but the index, spread far
/// enough apart that no two screen as near-misses of each other.
fn word(i: usize) -> String {
    let mut state = (i as u64)
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (0..10)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (b'a' + ((state >> 33) % 26) as u8) as char
        })
        .collect()
}

/// Text of exactly `len` characters.
fn text(len: usize) -> String {
    let mut out: String = "kiln ".repeat(len / 5 + 1).chars().take(len).collect();
    // A trailing space would be trimmed on the way in and move the length.
    if out.ends_with(' ') {
        out.pop();
        out.push('k');
    }
    out
}

fn refused(answer: &Value) -> bool {
    answer["status"] == "blocked"
}

/// The first size in `from..to` that `lands` refuses, found by bisection. `from`
/// must land and `to` must not, and the answer is the pair that proves it.
async fn first_refused<F, Fut>(mut from: usize, mut to: usize, lands: F) -> usize
where
    F: Fn(usize) -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    assert!(lands(from).await, "{from} characters should land");
    assert!(!lands(to).await, "{to} characters should be refused");
    while to - from > 1 {
        let mid = (from + to) / 2;
        if lands(mid).await {
            from = mid;
        } else {
            to = mid;
        }
    }
    to
}

/// A thing nobody has written about, made for one attempt.
async fn fresh_thing(otto: &Session, i: usize) -> String {
    let handle = format!("thing:{}", word(i));
    otto.add(&handle, &format!("Spot {}", word(i))).await;
    handle
}

/// **A claim too big to be read whole is refused with nothing written, and the
/// largest one that lands is read back whole under the ceiling.** The details of
/// a capture are the knob: one character more than the limit is refused and the
/// limit itself is accepted and recalled in one answer.
#[tokio::test]
async fn a_capture_too_big_to_be_read_back_whole_is_refused_and_the_largest_that_lands_is_not() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    let probes = std::cell::Cell::new(0usize);
    let capturing = |len: usize| {
        let otto = &otto;
        let probes = &probes;
        async move {
            probes.set(probes.get() + 1);
            let subject = fresh_thing(otto, probes.get()).await;
            let answer = otto
                .attempt(
                    "capture",
                    json!({
                        "subject": subject, "content": "a note", "details": text(len),
                        "provenance": "testimony",
                    }),
                )
                .await;
            !refused(&answer)
        }
    };
    let over = first_refused(100, ANSWER_CEILING + 5_000, capturing).await;
    assert!(
        (20_000..ANSWER_CEILING).contains(&over),
        "the limit is the ceiling less the envelope, not a number of its own: {over}"
    );

    // One over is refused: the word says the call must change, the refusal
    // names the ways to say it, and nothing was written.
    let subject = fresh_thing(&otto, 10_001).await;
    let refusal = otto
        .refused(
            "capture",
            json!({
                "subject": subject, "content": "a note", "details": text(over),
                "provenance": "testimony",
            }),
        )
        .await;
    assert_eq!(refusal.json()["fix_by"], "change", "{}", refusal.raw());
    assert_eq!(refusal.json()["wrote"], false, "{}", refusal.raw());
    refusal.says("split").says("pointer");
    let held = otto
        .call("recall", json!({"subject": subject, "facts": true}))
        .await;
    assert_eq!(
        held.json()["objects"][0]["facts"].as_array().map(Vec::len),
        Some(0),
        "nothing was written: {}",
        held.raw()
    );

    // The limit itself lands, and a read of the thing serves it whole in one
    // answer that fits.
    let subject = fresh_thing(&otto, 10_002).await;
    otto.call(
        "capture",
        json!({
            "subject": subject, "content": "a note", "details": text(over - 1),
            "provenance": "testimony",
        }),
    )
    .await;
    let (read, size) = otto
        .answer_and_size("recall", json!({"subject": subject, "facts": true}))
        .await;
    assert!(size <= ANSWER_CEILING, "the read fits: {size}");
    let facts = read["objects"][0]["facts"].as_array().expect("the facts");
    assert_eq!(facts.len(), 1, "{read}");
    assert_eq!(
        facts[0]["details"].as_str().map(str::len),
        Some(over - 1),
        "the claim came back whole"
    );
    assert!(read.get("not_shown").is_none(), "{read}");
}

/// The details length at which `capture` first refuses, found fresh each time.
async fn capture_limit(otto: &Session) -> usize {
    let probes = std::cell::Cell::new(50_000usize);
    first_refused(100, ANSWER_CEILING + 5_000, |len| {
        probes.set(probes.get() + 1);
        let probe = probes.get();
        async move {
            let subject = fresh_thing(otto, probe).await;
            let answer = otto
                .attempt(
                    "capture",
                    json!({
                        "subject": subject, "content": "a note", "details": text(len),
                        "provenance": "testimony",
                    }),
                )
                .await;
            !refused(&answer)
        }
    })
    .await
}

/// **The limit is where a read stops fitting, not a number of its own.** A
/// record written under the guard, straight into the store, comes back from a
/// read over the ceiling a little past the limit and under it at the limit. The
/// guard keeps back what a read keeps back (the status bar, the pointer to the
/// next part) and room for what a read adds on its first answer and for the
/// widest stamp and revision note, so it is stricter than the bytes of a read by
/// that much and no more.
#[tokio::test]
async fn the_limit_for_a_claim_is_where_a_read_of_it_stops_fitting() {
    use jojobot_domain::memory::{EntityId, EntityKind, Guarded, NewEntity, NewFact};

    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    let limit = capture_limit(&otto).await;

    // A record of `len` details characters written under the guard, then read.
    let served = |len: usize| {
        let (story, otto) = (&story, &otto);
        async move {
            let memory = story.memory_store();
            let id = EntityId::new(EntityKind::THING, word(60_000 + len));
            let added = memory
                .add_entity(NewEntity::new(
                    id.clone(),
                    format!("Spot {}", word(60_000 + len)),
                    "user-named",
                ))
                .await;
            assert!(matches!(added, Ok(Guarded::Written(_))), "{added:?}");
            let today: jiff::civil::Date = "2026-10-01".parse().expect("a day");
            let mut fact = NewFact::about(id.clone(), "a note", today);
            fact.details = Some(text(len));
            let written = memory.capture(fact).await;
            assert!(matches!(written, Ok(Guarded::Written(_))), "{written:?}");
            otto.answer_and_size("recall", json!({"subject": id.as_str(), "facts": true}))
                .await
                .1
        }
    };
    let (fits, past, at) = (
        served(limit - 1).await,
        served(limit + 3_000).await,
        served(limit).await,
    );
    assert!(
        fits <= ANSWER_CEILING,
        "the largest record the guard lets through is read back under the ceiling: {fits}"
    );
    assert!(
        past > ANSWER_CEILING,
        "a record 3,000 characters past the limit is not: the guard is not looser than a read: \
         {past}"
    );
    assert!(
        at + 3_000 > ANSWER_CEILING,
        "the guard keeps back no more than 3,000 characters for what a read adds: {at}"
    );
}

/// **An edit that makes a record too big to be read back whole is refused, the
/// record stays as it was, and the largest edit that lands is read back whole.**
#[tokio::test]
async fn an_edit_that_makes_a_claim_too_big_to_read_is_refused_and_leaves_it_as_it_was() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    let subject = fresh_thing(&otto, 20_000).await;
    let address = otto.fact(&subject, "a note").await;
    let over = first_refused(100, ANSWER_CEILING + 5_000, |len| {
        let (otto, address) = (&otto, &address);
        async move {
            let answer = otto
                .attempt(
                    "update_fact",
                    json!({"address": address, "details": text(len)}),
                )
                .await;
            !refused(&answer)
        }
    })
    .await;
    assert_eq!(
        over,
        capture_limit(&otto).await,
        "a capture and an edit are held to one limit"
    );

    // One over is refused and the claim keeps what it held at the limit.
    let refusal = otto
        .refused(
            "update_fact",
            json!({"address": address, "details": text(over)}),
        )
        .await;
    assert_eq!(refusal.json()["fix_by"], "change", "{}", refusal.raw());
    refusal.says("split").says("pointer");
    let (read, size) = otto
        .answer_and_size("recall", json!({"subject": subject, "facts": true}))
        .await;
    assert!(size <= ANSWER_CEILING, "{size}");
    let facts = read["objects"][0]["facts"].as_array().expect("the facts");
    assert_eq!(
        facts[0]["details"].as_str().map(str::len),
        Some(over - 1),
        "the refused edit changed nothing: {read}"
    );

    // And an edit that does not grow it lands even when a record is over: here
    // the claim is archived, which says nothing new.
    otto.call(
        "update_fact",
        json!({"address": address, "status": "archived", "details": text(over - 1)}),
    )
    .await;
}

/// **A creation whose first claim cannot be read back whole makes nothing.**
/// `sets` on `add_entity` is written as the thing's first claim and folds onto
/// the thing, so it is held to the same limit and the refusal leaves no thing
/// behind.
#[tokio::test]
async fn a_creation_whose_first_claim_is_too_big_to_read_back_makes_nothing() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    let probes = std::cell::Cell::new(30_000usize);
    let creating = |len: usize| {
        let (otto, probes) = (&otto, &probes);
        async move {
            probes.set(probes.get() + 1);
            let slug = word(probes.get());
            let answer = otto
                .attempt(
                    "add_entity",
                    json!({
                        "kind": "thing", "handle": slug, "name": format!("Spot {slug}"),
                        "source": "user-named", "sets": {"note": text(len)},
                    }),
                )
                .await;
            !refused(&answer)
        }
    };
    let over = first_refused(100, ANSWER_CEILING + 5_000, creating).await;
    assert!(
        (10_000..ANSWER_CEILING).contains(&over),
        "a value set on the thing counts once on the claim and once on the thing: {over}"
    );

    let slug = word(40_000);
    let refusal = otto
        .refused(
            "add_entity",
            json!({
                "kind": "thing", "handle": slug, "name": format!("Spot {slug}"),
                "source": "user-named", "sets": {"note": text(over)},
            }),
        )
        .await;
    assert_eq!(refusal.json()["fix_by"], "change", "{}", refusal.raw());
    otto.refused("recall", json!({"subject": format!("thing:{slug}")}))
        .await;

    let slug = word(40_001);
    otto.call(
        "add_entity",
        json!({
            "kind": "thing", "handle": slug, "name": format!("Spot {slug}"),
            "source": "user-named", "sets": {"note": text(over - 1)},
        }),
    )
    .await;
    let (read, size) = otto
        .answer_and_size(
            "recall",
            json!({"subject": format!("thing:{slug}"), "facts": true}),
        )
        .await;
    assert!(size <= ANSWER_CEILING, "{size}");
    assert_eq!(
        read["objects"][0]["fields"]["note"].as_str().map(str::len),
        Some(over - 1),
        "{read}"
    );

    // The same value set by an edit of an existing thing is held to a limit of
    // its own (the generated claim words differ), and a refusal changes nothing.
    let probes = std::cell::Cell::new(41_000usize);
    let editing_over = first_refused(100, ANSWER_CEILING, |len| {
        let (otto, probes) = (&otto, &probes);
        async move {
            probes.set(probes.get() + 1);
            let existing = fresh_thing(otto, probes.get()).await;
            let answer = otto
                .attempt(
                    "update_entity",
                    json!({"handle": existing, "sets": {"note": text(len)}}),
                )
                .await;
            !refused(&answer)
        }
    })
    .await;
    let existing = fresh_thing(&otto, 42_000).await;
    let refusal = otto
        .refused(
            "update_entity",
            json!({"handle": existing, "sets": {"note": text(editing_over)}}),
        )
        .await;
    assert_eq!(refusal.json()["fix_by"], "change", "{}", refusal.raw());
    let bare = otto
        .call("recall", json!({"subject": existing, "facts": true}))
        .await;
    assert_eq!(
        bare.json()["objects"][0]["facts"].as_array().map(Vec::len),
        Some(0),
        "the refused edit wrote no claim: {}",
        bare.raw()
    );
    otto.call(
        "update_entity",
        json!({"handle": existing, "sets": {"note": text(editing_over - 1)}}),
    )
    .await;
}

/// `count` aliases of 150 characters that no other thing shares.
fn aliases(seed: usize, count: usize) -> Vec<String> {
    (0..count)
        .map(|i| {
            format!("{} ", word(seed * 1_000 + i))
                .repeat(14)
                .trim()
                .to_string()
        })
        .collect()
}

/// **A thing whose aliases could not be read back whole is not made, and an
/// edit that sets them is refused.** Every read of a thing serves its aliases,
/// so a list that does not fit one answer leaves room for no record on it.
#[tokio::test]
async fn aliases_that_could_not_be_read_back_whole_are_refused_on_creation_and_on_edit() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    let probes = std::cell::Cell::new(70_000usize);
    let creating = |count: usize| {
        let (otto, probes) = (&otto, &probes);
        async move {
            probes.set(probes.get() + 1);
            let slug = word(probes.get());
            otto.attempt(
                "add_entity",
                json!({
                    "kind": "thing", "handle": slug, "name": format!("Spot {slug}"),
                    "source": "user-named", "aliases": aliases(probes.get(), count),
                }),
            )
            .await
        }
    };
    // Every accepted list stays in the store and is screened against by the
    // next, so the limit is found from the numbers two refusals carry rather
    // than by bisection: each alias costs the same, so size is a line in the
    // count and the room says where it ends.
    let (wide, narrow) = (creating(300).await, creating(200).await);
    assert!(refused(&wide) && refused(&narrow), "{wide} {narrow}");
    let number =
        |body: &Value, key: &str| body[key].as_u64().expect("the refusal says its size") as usize;
    let per_alias = (number(&wide, "size") - number(&narrow, "size")) / 100;
    let base = number(&narrow, "size") - per_alias * 200;
    let mut over = (number(&wide, "room") - base) / per_alias + 1;
    // The estimate is read off two lines; the limit itself is asked for.
    while refused(&creating(over - 1).await) {
        over -= 1;
    }
    assert!(
        refused(&creating(over).await),
        "{over} aliases should be refused"
    );
    assert!(
        (100..250).contains(&over),
        "about 150 characters each under 28,000: {over}"
    );

    let slug = word(80_000);
    let refusal = otto
        .refused(
            "add_entity",
            json!({
                "kind": "thing", "handle": slug, "name": format!("Spot {slug}"),
                "source": "user-named", "aliases": aliases(80_000, over),
            }),
        )
        .await;
    assert_eq!(refusal.json()["fix_by"], "change", "{}", refusal.raw());
    refusal.says("aliases");
    otto.refused("recall", json!({"subject": format!("thing:{slug}")}))
        .await;

    // One alias fewer lands, and the thing is read back whole.
    let handle = format!("thing:{}", word(80_001));
    otto.call(
        "add_entity",
        json!({
            "kind": "thing", "handle": word(80_001), "name": format!("Spot {}", word(80_001)),
            "source": "user-named", "aliases": aliases(80_001, over - 1),
        }),
    )
    .await;
    let (read, size) = otto
        .answer_and_size("recall", json!({"subject": handle, "facts": true}))
        .await;
    assert!(size <= ANSWER_CEILING, "{size}");
    assert_eq!(
        read["objects"][0]["alternateName"].as_array().map(Vec::len),
        Some(over - 1),
        "{read}"
    );

    // An edit that would send the same list is held to the same limit and
    // changes nothing when it is refused.
    let refusal = otto
        .refused(
            "update_entity",
            json!({"handle": handle, "aliases": aliases(80_002, over)}),
        )
        .await;
    assert_eq!(refusal.json()["fix_by"], "change", "{}", refusal.raw());
    let again = otto
        .call("recall", json!({"subject": handle, "facts": true}))
        .await;
    assert_eq!(
        again.json()["objects"][0]["alternateName"]
            .as_array()
            .map(Vec::len),
        Some(over - 1),
        "the refused edit changed nothing"
    );
}

/// **A merge whose account could not be read back whole moves nothing.** The
/// reason a caller gives is written as the account on the survivor, so it is
/// held to the same limit as any claim, and the duplicate stays what it was.
#[tokio::test]
async fn a_merge_whose_account_is_too_big_to_read_back_moves_nothing() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    let probes = std::cell::Cell::new(90_000usize);
    let merging = |len: usize| {
        let (otto, probes) = (&otto, &probes);
        async move {
            probes.set(probes.get() + 2);
            let duplicate = fresh_thing(otto, probes.get()).await;
            let survivor = fresh_thing(otto, probes.get() + 1).await;
            let answer = otto
                .attempt(
                    "merge_entities",
                    json!({"duplicate": duplicate, "survivor": survivor, "reason": text(len)}),
                )
                .await;
            !refused(&answer)
        }
    };
    let over = first_refused(10, ANSWER_CEILING + 5_000, merging).await;

    let duplicate = fresh_thing(&otto, 95_000).await;
    let survivor = fresh_thing(&otto, 95_001).await;
    let refusal = otto
        .refused(
            "merge_entities",
            json!({"duplicate": duplicate, "survivor": survivor, "reason": text(over)}),
        )
        .await;
    assert_eq!(refusal.json()["fix_by"], "change", "{}", refusal.raw());
    let still = otto
        .call("recall", json!({"subject": duplicate, "facts": true}))
        .await;
    assert_ne!(
        still.json()["objects"][0]["status"],
        "merged",
        "the duplicate was not folded: {}",
        still.raw()
    );

    let duplicate = fresh_thing(&otto, 95_002).await;
    let survivor = fresh_thing(&otto, 95_003).await;
    otto.call(
        "merge_entities",
        json!({"duplicate": duplicate, "survivor": survivor, "reason": text(over - 1)}),
    )
    .await;
    let (read, size) = otto
        .answer_and_size("recall", json!({"subject": survivor, "facts": true}))
        .await;
    assert!(size <= ANSWER_CEILING, "{size}");
    assert_eq!(
        read["objects"][0]["facts"][0]["content"]
            .as_str()
            .map(str::len),
        Some(over - 1),
        "{read}"
    );
}

/// **A thing whose own row is over one answer is never a dead end.** Such a
/// row can sit in a store from before the guard. A new claim on it is refused
/// for the right cause: the refusal says the thing's own row is what is over,
/// not the claim being written, and names the routes that shrink the row. An
/// edit that shrinks the row lands, and after it a new claim lands.
#[tokio::test]
async fn a_thing_whose_own_row_is_over_names_the_row_and_the_way_out_of_it() {
    use jojobot_domain::memory::{EntityId, EntityKind, Guarded, NewEntity};

    let first = Story::begin("bot:otto").await;
    let memory = first.memory_store();
    let id = EntityId::new(EntityKind::THING, word(120_000));
    let mut full = NewEntity::new(id.clone(), format!("Spot {}", word(120_000)), "user-named");
    full.aliases = aliases(120_000, 200);
    let added = memory.add_entity(full).await;
    assert!(matches!(added, Ok(Guarded::Written(_))), "{added:?}");
    let story = first.restarted().await;
    let otto = story.session().await;
    let handle = id.as_str().to_string();

    // A new claim is refused, and the answer says whose size it is.
    let refusal = otto
        .refused(
            "capture",
            json!({"subject": handle, "content": "a short note", "provenance": "testimony"}),
        )
        .await;
    assert_eq!(refusal.json()["fix_by"], "change", "{}", refusal.raw());
    assert_eq!(
        refusal.json()["what_is_over"],
        "the thing's name, aliases and keys",
        "{}",
        refusal.raw()
    );
    // The routes that shrink the row, and none that is about the claim.
    refusal.says("aliases").says("pointer");
    refusal.never_says("split it into claims");

    // A claim that is itself too big says it is the claim.
    let other = fresh_thing(&otto, 120_001).await;
    let big = otto
        .refused(
            "capture",
            json!({
                "subject": other, "content": "a note", "details": text(ANSWER_CEILING),
                "provenance": "testimony",
            }),
        )
        .await;
    assert_eq!(big.json()["what_is_over"], "the record", "{}", big.raw());

    // An edit that shrinks the row lands even while the row is still over, so a
    // row can be brought down in steps, and a claim is still refused for the row.
    // The receipt of such an edit fits one answer like every other: it does not
    // echo the aliases it leaves, and it says how to read them.
    let stepped = otto
        .call(
            "update_entity",
            json!({"handle": handle, "aliases": aliases(120_000, 190)}),
        )
        .await;
    let stepped = stepped.json();
    assert!(!refused(&stepped), "a smaller row is allowed: {stepped}");
    assert_eq!(stepped["id"], handle.as_str(), "{stepped}");
    assert_eq!(stepped["alternateName"], Value::Null, "{stepped}");
    assert_eq!(stepped["alternateName_elided"], true, "{stepped}");
    assert_eq!(stepped["alternateName_count"], 190, "{stepped}");
    assert!(
        stepped["how_to_read"]
            .as_str()
            .is_some_and(|how| how.contains("recall") && how.contains(handle.as_str())),
        "the receipt names the read that returns the aliases: {stepped}"
    );
    let still = otto
        .refused(
            "capture",
            json!({"subject": handle, "content": "a short note", "provenance": "testimony"}),
        )
        .await;
    assert_eq!(
        still.json()["what_is_over"],
        "the thing's name, aliases and keys",
        "{}",
        still.raw()
    );

    // An edit that brings the row under the limit lands, and its receipt still
    // carries the aliases whole: only a receipt that would not fit elides them.
    let brought_down = otto
        .call(
            "update_entity",
            json!({"handle": handle, "aliases": aliases(120_000, 20)}),
        )
        .await
        .json();
    assert_eq!(
        brought_down["alternateName"].as_array().map(Vec::len),
        Some(20),
        "{brought_down}"
    );
    assert!(
        brought_down.get("alternateName_elided").is_none(),
        "{brought_down}"
    );

    // And after it a new claim lands, and reads back.
    otto.call(
        "capture",
        json!({"subject": handle, "content": "a short note", "provenance": "testimony"}),
    )
    .await;
    let read = otto
        .call("recall", json!({"subject": handle, "facts": true}))
        .await;
    assert_eq!(
        read.json()["objects"][0]["facts"].as_array().map(Vec::len),
        Some(1),
        "{}",
        read.raw()
    );

    story.finish().await;
}

/// **The account a merge writes is measured beside the MERGED thing.** The
/// survivor takes the duplicate's names as aliases, so it is bigger once the
/// merge lands, and the account is read beside that bigger thing. A reason that
/// fits beside the survivor as it stands but not beside the merged one is
/// refused, and nothing moves.
#[tokio::test]
async fn a_merge_account_that_fits_beside_the_survivor_but_not_beside_the_merged_one_is_refused() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    let aliased = |seed: usize, count: usize, name: &str| {
        let (otto, name) = (&otto, name.to_string());
        async move {
            let handle = format!("thing:{}", word(seed));
            otto.call(
                "add_entity",
                json!({
                    "kind": "thing", "handle": word(seed), "name": name,
                    "source": "user-named", "aliases": aliases(seed, count),
                }),
            )
            .await;
            handle
        }
    };
    let reason = text(6_000);

    // The survivor as it stands has room for a record this big: a capture of the
    // same size lands.
    let survivor = aliased(130_000, 100, "Spot Keeper").await;
    otto.call(
        "capture",
        json!({
            "subject": survivor, "content": "a note", "details": reason,
            "provenance": "testimony",
        }),
    )
    .await;

    // The duplicate's forty aliases make the merged thing bigger, and its room
    // smaller than the account.
    let duplicate = aliased(130_001, 40, "Spot Dupe").await;
    let refusal = otto
        .refused(
            "merge_entities",
            json!({"duplicate": duplicate, "survivor": survivor, "reason": reason}),
        )
        .await;
    assert_eq!(refusal.json()["fix_by"], "change", "{}", refusal.raw());
    let still = otto
        .call("recall", json!({"subject": duplicate, "facts": true}))
        .await;
    assert_ne!(
        still.json()["objects"][0]["status"],
        "merged",
        "nothing moved: {}",
        still.raw()
    );

    // The same merge with a short reason lands, so the reason was what was over.
    otto.call(
        "merge_entities",
        json!({"duplicate": duplicate, "survivor": survivor, "reason": "the same keeper"}),
    )
    .await;

    story.finish().await;
}
