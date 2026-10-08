//! "How big is an answer allowed to get?"
//!
//! The client renders about 30,000 characters of an answer inline and nothing
//! past roughly 50,000. One ceiling, 28,000 characters, is the most any answer
//! is meant to carry, and a list fills against it in its own order rather than
//! being cut after the fact.
//!
//! The fixture here is hostile on purpose: a world big enough that the widest
//! call of each read verb would pass the ceiling if nothing stopped it. Each
//! case asks through the served surface, so what is measured is the answer a
//! client receives.

use serde_json::{Value, json};

use jojobot_domain::text::ANSWER_CEILING;

use super::dsl::{Session, Story};

/// How many entities the world holds, how many facts mention the marker, how
/// many facts sit on one subject and how many messages wait in one box.
const ENTITIES: usize = 120;
const MARKER_FACTS: usize = 300;
const FACTS_ON_ONE_SUBJECT: usize = 60;
const MESSAGES: usize = 50;
const PLACES: usize = 10;
/// How many runs otto has had, each with a focus near the longest a run keeps,
/// so `list_runs` at its widest has more to list than one answer holds.
const RUNS: usize = 90;

/// The two days an aged world is written on: every place is described whole in
/// the first, and one of its keys is rewritten in the second.
const AUGUST: &str = "2026-08-03";
const OCTOBER: &str = "2026-10-08";

/// The verbs whose widest call still passes the ceiling, each named for the
/// call that was measured. A later slice that makes one fit removes its line
/// here, and a verb that starts passing the ceiling without being added fails
/// the case that compares the two.
///
/// `list_runs` has no fit yet: with runs enough to stretch it, its widest call
/// passes the ceiling. Written down here until the slice that fits it.
const STILL_OVER: &[&str] = &["list_runs"];

/// A slug of ten letters that depends on nothing but the index, spread far
/// enough apart that no two screen as near-misses of each other.
fn slug(i: usize) -> String {
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

/// A sentence of about `len` characters.
fn filler(len: usize, seed: usize) -> String {
    let words = [
        "kiln",
        "relined",
        "stack",
        "ledger",
        "invoice",
        "shipment",
        "manifest",
        "crate",
        "counted",
        "reconciled",
        "against",
        "the",
        "flue",
        "draft",
        "burner",
        "glaze",
    ];
    let mut out = String::new();
    let mut at = seed;
    while out.len() < len {
        out.push_str(words[at % words.len()]);
        out.push(' ');
        at = at.wrapping_mul(7).wrapping_add(3);
    }
    out
}

/// The world, and the two sessions the measuring needs.
struct World {
    _story: Story,
    otto: Session,
    sigma: Session,
    heavy: String,
    /// The address of every fact that mentions the marker, read off the store
    /// as each was written, so the walk of a search has an oracle that is not
    /// the search.
    marker_addresses: Vec<String>,
    /// The address of every fact on the heavy place, and the handle of every
    /// place, as the store reported them.
    heavy_addresses: Vec<String>,
    place_ids: Vec<String>,
    thing_ids: Vec<String>,
    /// The id and body of every message sent to otto, as the store reported
    /// them when each was posted.
    messages: Vec<(String, String)>,
    /// The same for the box sigma owns, which sigma's own post collects.
    sigma_messages: Vec<(String, String)>,
}

/// **A world too big to write through the served surface**, seeded in the
/// stores under the server and then served by a second server whose index is
/// built from them in one pass. What is measured afterwards is still the answer
/// a client receives from the served surface.
async fn hostile_world() -> World {
    world(false).await
}

/// **The same world with the ten places written on two days.** Each place holds
/// a colour and a size from August, and in October its colour is rewritten, so
/// every place has one key older than the thing. The server is served on the
/// October day, which is the frame the stamps are read as days in.
async fn aged_hostile_world() -> World {
    world(true).await
}

async fn world(aged: bool) -> World {
    use jojobot_domain::mailbox::{MailboxName, Mailboxes, NewMessage};
    use jojobot_domain::memory::{EntityId, EntityKind, NewEntity, NewFact};

    let (first, clock) = if aged {
        let (story, hand) = Story::begin_on_a_store_clock_that_moves("bot:otto", AUGUST).await;
        (story, Some(hand))
    } else {
        (Story::begin("bot:otto").await, None)
    };
    first.session().await.add("bot:sigma", "Sigma").await;

    let memory = first.memory_store();
    let today: jiff::civil::Date = "2026-10-01".parse().expect("a day");
    let mut handles = Vec::new();
    for i in 0..ENTITIES {
        let id = EntityId::new(EntityKind::THING, slug(i));
        memory
            .add_entity(NewEntity::new(
                id.clone(),
                format!("Gizmo {}", slug(i)),
                // **Where the thing came from, at the longest a source may be.** The write guard screens
                // names and aliases against every thing already held, which
                // is what makes a thing costly to add, and it does not screen
                // this. Entities that each carry a sentence reach the ceiling
                // with the things the world already has.
                filler(190, i),
            ))
            .await
            .expect("an entity lands");
        handles.push(id);
    }
    let mut marker_addresses = Vec::new();
    for i in 0..MARKER_FACTS {
        let written = memory
            .capture(NewFact::about(
                handles[i % 40].clone(),
                format!("marker observation {i}: {}", filler(180, i)),
                today,
            ))
            .await
            .expect("a fact lands");
        let jojobot_domain::memory::Guarded::Written(fact) = written else {
            panic!("a fact about a thing that exists lands");
        };
        marker_addresses.push(fact.address().to_string());
    }
    // The places: a small kind with many facts apiece, one of them heavy. A kind
    // recall costs more than the answer grows, so the kind that carries the
    // facts is kept to ten.
    let mut place_ids: Vec<String> = Vec::new();
    let mut heavy_addresses: Vec<String> = Vec::new();
    let mut heavy = None;
    for p in 0..PLACES {
        let id = EntityId::new(EntityKind::PLACE, slug(10_000 + p));
        memory
            .add_entity(NewEntity::new(
                id.clone(),
                format!("Spot {}", slug(10_000 + p)),
                "user-named",
            ))
            .await
            .expect("a place lands");
        let (facts, length) = if p == 0 {
            (FACTS_ON_ONE_SUBJECT, 780)
        } else {
            (8, 500)
        };
        for i in 0..facts {
            let mut record = NewFact::about(
                id.clone(),
                format!("long record {i}: {}", filler(length, i + p)),
                today,
            );
            if aged && i == 0 {
                record.fields = [("colour", "green"), ("size", "large")]
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
            }
            let written = memory.capture(record).await.expect("a fact lands");
            let jojobot_domain::memory::Guarded::Written(fact) = written else {
                panic!("a fact about a place that exists lands");
            };
            if p == 0 {
                heavy_addresses.push(fact.address().to_string());
            }
        }
        if p == 0 {
            heavy = Some(id.clone());
        }
        place_ids.push(id.as_str().to_string());
    }
    let heavy = heavy.expect("a heavy place");
    if let Some(hand) = &clock {
        hand.stating(OCTOBER.parse().expect("a day"));
        for id in &place_ids {
            let mut repainted =
                NewFact::about(EntityId(id.clone()), "repainted".to_string(), today);
            repainted.fields = [("colour".to_string(), "blue".to_string())]
                .into_iter()
                .collect();
            memory.capture(repainted).await.expect("a fact lands");
        }
    }
    let mail = first.mail_store();
    let mut messages = Vec::new();
    let mut sigma_messages = Vec::new();
    for (to, from, seed, kept) in [
        ("otto", "bot:sigma", 0, &mut messages),
        ("sigma", "bot:otto", 1000, &mut sigma_messages),
    ] {
        for i in 0..MESSAGES {
            let body = filler(700, seed + i);
            let posted = mail
                .post_message(NewMessage {
                    mailbox: MailboxName(to.into()),
                    body: body.clone(),
                    subject: Some(format!("report {i}")),
                    sender: from.into(),
                    sent_at: jiff::Timestamp::now(),
                    in_reply_to: None,
                    sender_mail_waiting_at_send: None,
                    posted_by_session: None,
                })
                .await
                .expect("a message lands");
            let jojobot_domain::mailbox::Guarded::Written(message) = posted else {
                panic!("a message to a box that exists lands");
            };
            kept.push((message.id.as_str().to_string(), body));
        }
    }

    // Otto's runs: each opened beside the others and given a journal entry,
    // which is what makes a run exist, with a focus near the longest kept.
    for i in 0..RUNS {
        let run = first.as_bot("bot:otto").await;
        run.call(
            "journal",
            json!({"entry": format!("sitting {i}"), "focus": filler(190, 3000 + i)}),
        )
        .await;
    }

    let story = if aged {
        first.restarted_pretending_it_is(OCTOBER).await
    } else {
        first.restarted().await
    };
    let otto = story.as_bot("bot:otto").await;
    let sigma = story.as_bot("bot:sigma").await;
    World {
        _story: story,
        otto,
        sigma,
        heavy: heavy.as_str().to_string(),
        marker_addresses,
        heavy_addresses,
        place_ids,
        thing_ids: handles.iter().map(|id| id.as_str().to_string()).collect(),
        messages,
        sigma_messages,
    }
}

/// Every read-shaped verb at its widest, with the characters it answered.
async fn widest_calls(world: &World) -> Vec<(&'static str, usize)> {
    let o = &world.otto;
    let mut measured = Vec::new();
    measured.push((
        "search at limit 1000",
        o.answer_size("search", json!({"query": "marker", "limit": 1000}))
            .await,
    ));
    measured.push((
        "recall one subject with its facts",
        o.answer_size("recall", json!({"subject": world.heavy, "facts": true}))
            .await,
    ));
    measured.push((
        "recall every thing of a kind",
        o.answer_size("recall", json!({"kind": "thing"})).await,
    ));
    measured.push((
        "recall every place with its facts",
        o.answer_size("recall", json!({"kind": "place", "facts": true}))
            .await,
    ));
    measured.push((
        "list_entities",
        o.answer_size("list_entities", json!({})).await,
    ));
    measured.push((
        "list_sent with bodies",
        world
            .sigma
            .answer_size("list_sent", json!({"include_bodies": true, "limit": 1000}))
            .await,
    ));
    // Sigma's post collects sigma's own full box, so otto's box stays whole for
    // the read below.
    measured.push((
        "post_message collecting a full box",
        world
            .sigma
            .answer_size(
                "post_message",
                json!({"to": "otto", "body": "reporting in", "subject": "done"}),
            )
            .await,
    ));
    measured.push((
        "read_mailbox, everything waiting",
        o.answer_size("read_mailbox", json!({})).await,
    ));
    measured.push((
        "list_runs",
        o.answer_size("list_runs", json!({"limit": 1000})).await,
    ));
    measured
}

/// **The widest call of each read verb against the one ceiling.** Prints the
/// table, so a run with `--nocapture` is the measurement, and asserts the two
/// things a ceiling is: the verbs this slice has made fit do fit, and the set
/// still over it is exactly the set written down in `STILL_OVER`.
#[tokio::test]
async fn the_widest_call_of_each_read_verb_against_the_one_ceiling() {
    let world = hostile_world().await;
    let measured = widest_calls(&world).await;

    eprintln!("ceiling {ANSWER_CEILING}");
    for (call, size) in &measured {
        let verdict = if *size > ANSWER_CEILING {
            "OVER"
        } else {
            "fits"
        };
        eprintln!("{size:>8}  {verdict:4}  {call}");
    }

    let over: Vec<&str> = measured
        .iter()
        .filter(|(_, size)| *size > ANSWER_CEILING)
        .map(|(call, _)| *call)
        .collect();
    assert_eq!(
        over, STILL_OVER,
        "the calls over the ceiling are not the calls written down as still over it"
    );
}

/// **Search at its widest fits under the ceiling, and walking on from where it
/// stops returns every hit exactly once.** Three hundred facts mention the
/// marker, far more than fit in one answer. Each part, status bar included,
/// is under the ceiling; the offset each names reads the next; and the parts
/// together are exactly the facts that were written, which the store handed
/// back as each was written and which search did not produce.
#[tokio::test]
async fn search_at_its_widest_fits_and_walking_on_returns_every_hit_exactly_once() {
    let world = hostile_world().await;
    let mut seen: Vec<String> = Vec::new();
    let mut offset = 0u64;
    let mut parts = 0;
    loop {
        let part = world
            .otto
            .call(
                "search",
                json!({"query": "marker", "limit": 1000, "offset": offset}),
            )
            .await;
        parts += 1;
        assert!(parts < 100, "the walk never ends");
        assert!(
            part.size() <= ANSWER_CEILING,
            "part {parts} is {} characters",
            part.size()
        );
        let body = part.json();
        for hit in body["results"].as_array().expect("a list of results") {
            assert_eq!(hit["hit"], "fact", "only facts mention the marker: {hit}");
            seen.push(hit["address"].as_str().expect("an address").to_string());
        }
        match body["not_shown"]["offset"].as_u64() {
            Some(next) => offset = next,
            None => break,
        }
    }
    assert!(parts > 2, "the facts took several parts: {parts}");
    let mut walked = seen.clone();
    walked.sort();
    walked.dedup();
    assert_eq!(walked.len(), seen.len(), "no hit came back twice");
    let mut written = world.marker_addresses.clone();
    written.sort();
    assert_eq!(
        walked, written,
        "every fact that was written, and nothing else"
    );
}

/// **`limit` is a page and `offset` is where the next page starts.** Three
/// hundred facts read forty at a time come back as pages that each hold forty
/// (the last holds the rest), and together they are exactly the facts that were
/// written, each once. The real index answers, not a double.
#[tokio::test]
async fn pages_of_forty_walk_the_whole_ranking_once() {
    let world = hostile_world().await;
    let mut seen: Vec<String> = Vec::new();
    let mut sizes: Vec<usize> = Vec::new();
    let mut offset = 0u64;
    loop {
        let page = world
            .otto
            .call(
                "search",
                json!({"query": "marker", "limit": 40, "offset": offset}),
            )
            .await;
        assert!(page.size() <= ANSWER_CEILING, "{}", page.size());
        let body = page.json();
        let hits = body["results"].as_array().expect("a list of results");
        if hits.is_empty() {
            assert!(body["past_the_end"].is_string(), "{body}");
            break;
        }
        sizes.push(hits.len());
        offset += hits.len() as u64;
        seen.extend(
            hits.iter()
                .map(|h| h["address"].as_str().expect("an address").to_string()),
        );
        assert!(sizes.len() < 50, "the walk never ends");
    }
    assert!(
        sizes.iter().all(|n| *n <= 40),
        "a page is at most forty: {sizes:?}"
    );
    assert!(
        sizes.len() >= 8,
        "three hundred facts need eight pages: {sizes:?}"
    );
    let mut walked = seen.clone();
    walked.sort();
    walked.dedup();
    assert_eq!(walked.len(), seen.len(), "no hit came back twice");
    let mut written = world.marker_addresses.clone();
    written.sort();
    assert_eq!(
        walked, written,
        "every fact that was written, and nothing else"
    );
}

/// **A box far past the ceiling is still delivered whole in count and under the
/// ceiling in size.** Every waiting message is taken, which the box counts
/// confirm from the other side. The answer carries the oldest bodies whole and
/// then leaves bodies out, each flagged, and names by id the messages that did
/// not fit even as an envelope. Nothing is lost: every message is accounted for
/// exactly once in the answer, and every body that was left out is whole when
/// asked for by id.
#[tokio::test]
async fn a_box_past_the_ceiling_is_taken_whole_and_the_answer_stays_under_it() {
    let world = hostile_world().await;
    let delivery = world.otto.call("read_mailbox", json!({})).await;
    assert!(delivery.size() <= ANSWER_CEILING, "{}", delivery.size());
    let body = delivery.json();

    let messages = body["messages"].as_array().expect("a list of messages");
    let whole: Vec<&Value> = messages.iter().filter(|m| m["body"].is_string()).collect();
    let flagged: Vec<&Value> = messages
        .iter()
        .filter(|m| m["body_elided"] == true)
        .collect();
    assert!(!whole.is_empty(), "the oldest bodies come whole");
    assert_eq!(
        whole.len() + flagged.len(),
        messages.len(),
        "a message is either whole or flagged, never both and never neither"
    );
    for left_out in &flagged {
        assert!(left_out["body"].is_null(), "{left_out}");
        assert!(left_out["body_bytes"].as_u64().is_some(), "{left_out}");
    }
    let named: Vec<String> = body["not_shown"]["ids"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .map(|id| id.as_str().expect("an id").to_string())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        flagged.len() + named.len() > 0,
        "fifty messages of seven hundred characters do not fit whole"
    );
    assert_eq!(
        body["not_shown"]["count"].as_u64().unwrap_or(0) as usize,
        named.len(),
        "{body}"
    );

    // Every message is in the answer exactly once, as itself or as an id.
    let mut accounted: Vec<String> = messages
        .iter()
        .map(|m| m["id"].as_str().expect("an id").to_string())
        .chain(named.iter().cloned())
        .collect();
    accounted.sort();
    let mut posted: Vec<String> = world.messages.iter().map(|(id, _)| id.clone()).collect();
    posted.sort();
    assert_eq!(accounted, posted, "no message lost, none twice");

    // Taken: the box counts say so from the other side.
    let counted = world
        .otto
        .call("read_mailbox", json!({"counts_only": true}))
        .await
        .json();
    assert_eq!(counted["counts"]["new"], 0, "{counted}");
    assert_eq!(counted["counts"]["read"], MESSAGES, "{counted}");

    // Reachable: each body that was left out is whole by id.
    for (id, expected) in &world.messages {
        let left_out = flagged.iter().any(|m| m["id"] == *id) || named.contains(id);
        if !left_out {
            continue;
        }
        let read = world
            .otto
            .call("read_message", json!({"message_id": id}))
            .await
            .json();
        // The store keeps a body without the trailing space the fixture ends on.
        assert_eq!(read["body"], expected.trim_end(), "{id}");
    }
}

/// **A post that collects a box past the ceiling stays under it and takes
/// every message.** The delivery that rides on a post is the same delivery a
/// read makes, fitted in the room the post's own receipt leaves. Each message
/// is carried or named exactly once, the box counts say all were taken, and
/// every body left out is whole when asked for by id.
#[tokio::test]
async fn a_post_that_collects_a_box_past_the_ceiling_stays_under_it_and_takes_every_message() {
    let world = hostile_world().await;
    let posted = world
        .sigma
        .call(
            "post_message",
            json!({"to": "otto", "body": "reporting in", "subject": "done"}),
        )
        .await;
    assert!(posted.size() <= ANSWER_CEILING, "{}", posted.size());
    let body = posted.json();
    let mail = &body["your_mail"];

    let messages = mail["messages"].as_array().expect("a list of messages");
    let flagged: Vec<&Value> = messages
        .iter()
        .filter(|m| m["body_elided"] == true)
        .collect();
    let named: Vec<String> = mail["not_shown"]["ids"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .map(|id| id.as_str().expect("an id").to_string())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        flagged.len() + named.len() > 0,
        "fifty messages of seven hundred characters do not fit whole beside a receipt: {mail}"
    );

    let mut accounted: Vec<String> = messages
        .iter()
        .map(|m| m["id"].as_str().expect("an id").to_string())
        .chain(named.iter().cloned())
        .collect();
    accounted.sort();
    let mut waiting: Vec<String> = world
        .sigma_messages
        .iter()
        .map(|(id, _)| id.clone())
        .collect();
    waiting.sort();
    assert_eq!(accounted, waiting, "no message lost, none twice");

    let counted = world
        .sigma
        .call("read_mailbox", json!({"counts_only": true}))
        .await
        .json();
    assert_eq!(counted["counts"]["new"], 0, "{counted}");
    assert_eq!(counted["counts"]["read"], MESSAGES, "{counted}");

    for (id, expected) in &world.sigma_messages {
        let left_out = flagged.iter().any(|m| m["id"] == *id) || named.contains(id);
        if !left_out {
            continue;
        }
        let read = world
            .sigma
            .call("read_message", json!({"message_id": id}))
            .await
            .json();
        assert_eq!(read["body"], expected.trim_end(), "{id}");
    }
}

/// **The next read names what was taken and not shown.** The messages the first
/// read left out are owed, so they come back named as leftovers, and none of
/// them is delivered a second time.
#[tokio::test]
async fn a_second_read_names_every_message_the_first_took() {
    let world = hostile_world().await;
    world.otto.call("read_mailbox", json!({})).await;
    let again = world.otto.call("read_mailbox", json!({})).await.json();
    assert_eq!(again["count"], 0, "nothing is fresh: {again}");
    assert_eq!(again["leftovers"]["count"], MESSAGES, "{again}");
}

/// **A walk of `list_entities` returns each entity once under the ceiling.**
/// The things, each with a sentence of where it came from, and the people and
/// bots beside them are past the ceiling whole. Each part, status bar included, is under it, the offset each
/// part names reads the next, and the parts together are exactly the entities
/// the store lists as browsable, which the store's own port answers and the
/// verb did not produce.
#[tokio::test]
async fn a_walk_of_list_entities_returns_each_entity_once_under_the_ceiling() {
    let world = hostile_world().await;
    let mut seen: Vec<String> = Vec::new();
    let mut offset = 0u64;
    let mut parts = 0;
    loop {
        let part = world
            .otto
            .call("list_entities", json!({"offset": offset}))
            .await;
        parts += 1;
        assert!(parts < 100, "the walk never ends");
        assert!(
            part.size() <= ANSWER_CEILING,
            "part {parts} is {} characters",
            part.size()
        );
        let body = part.json();
        let listed = body["entities"].as_array().expect("a list of entities");
        assert_eq!(body["count"], listed.len(), "{body}");
        for entity in listed {
            seen.push(entity["id"].as_str().expect("an id").to_string());
        }
        match body["not_shown"]["offset"].as_u64() {
            Some(next) => offset = next,
            None => break,
        }
    }
    assert!(parts > 1, "the entities took several parts: {parts}");
    let mut walked = seen.clone();
    walked.sort();
    walked.dedup();
    assert_eq!(walked.len(), seen.len(), "no entity came back twice");
    let mut stored: Vec<String> = world
        ._story
        .memory_store()
        .list_entities(None)
        .await
        .expect("the store lists its entities")
        .into_iter()
        .filter(jojobot_domain::memory::Entity::browsable)
        .map(|entity| entity.id.as_str().to_string())
        .collect();
    stored.sort();
    assert_eq!(
        walked, stored,
        "every entity the store holds, and nothing else"
    );
}

/// **An offset past the last entity says so** and carries nothing, so a caller
/// that overshot is told rather than shown an empty inventory.
#[tokio::test]
async fn an_offset_past_the_last_entity_says_it_is_past_the_end() {
    let world = hostile_world().await;
    let body = world
        .otto
        .call("list_entities", json!({"offset": 100_000}))
        .await
        .json();
    assert!(body["past_the_end"].is_string(), "{body}");
    assert_eq!(body["entities"].as_array().map(Vec::len), Some(0), "{body}");
    assert!(body.get("not_shown").is_none(), "{body}");
}

/// **A walk of `list_sent` returns each message once.** Fifty messages with
/// their bodies are far past the ceiling. Each part is under it, the offset
/// each part names reads the next, and the parts together are exactly the
/// messages that were sent.
#[tokio::test]
async fn a_walk_of_list_sent_returns_each_message_once_under_the_ceiling() {
    let world = hostile_world().await;
    let mut seen: Vec<String> = Vec::new();
    let mut offset = 0u64;
    let mut parts = 0;
    let mut left_out_by_the_last: Option<u64> = None;
    loop {
        let part = world
            .sigma
            .call(
                "list_sent",
                json!({"include_bodies": true, "limit": 1000, "offset": offset}),
            )
            .await;
        parts += 1;
        assert!(parts < 100, "the walk never ends");
        assert!(
            part.size() <= ANSWER_CEILING,
            "part {parts}: {}",
            part.size()
        );
        let body = part.json();
        let ids: Vec<String> = body["messages"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|m| m["id"].as_str().expect("an id").to_string())
            .collect();
        assert!(!ids.is_empty(), "a part carries at least one message");
        if let Some(promised) = left_out_by_the_last {
            let from_here = ids.len() as u64 + body["not_shown"]["count"].as_u64().unwrap_or(0);
            assert_eq!(
                promised, from_here,
                "what was left out is what the rest return"
            );
        }
        seen.extend(ids);
        match body["not_shown"]["offset"].as_u64() {
            Some(next) => {
                left_out_by_the_last = body["not_shown"]["count"].as_u64();
                offset = next;
            }
            None => break,
        }
    }
    assert!(parts > 1, "the messages took several parts: {parts}");
    let mut walked = seen.clone();
    walked.sort();
    walked.dedup();
    assert_eq!(walked.len(), seen.len(), "no message came back twice");
    let mut sent: Vec<String> = world.messages.iter().map(|(id, _)| id.clone()).collect();
    sent.sort();
    assert_eq!(
        walked, sent,
        "every message that was sent, and nothing else"
    );
}

/// **One subject with far more facts than fit comes back in parts that each fit
/// the ceiling, and walking on returns every fact once.** The heavy place holds
/// sixty long records, far past the ceiling. The offset names how many of its
/// facts have been read, the count each part states is what the later parts
/// return, and the facts together are exactly those the store reported.
#[tokio::test]
async fn a_subject_past_the_ceiling_is_read_in_parts_that_return_each_fact_once() {
    let world = hostile_world().await;
    let mut seen: Vec<String> = Vec::new();
    let mut offset = 0u64;
    let mut parts = 0;
    let mut left_out_by_the_last: Option<u64> = None;
    loop {
        let part = world
            .otto
            .call(
                "recall",
                json!({"subject": world.heavy, "facts": true, "offset": offset}),
            )
            .await;
        parts += 1;
        assert!(parts < 100, "the walk never ends");
        assert!(
            part.size() <= ANSWER_CEILING,
            "part {parts}: {}",
            part.size()
        );
        let body = part.json();
        assert_eq!(body["count"], 1, "one subject is one object: {body}");
        let addresses: Vec<String> = body["objects"][0]["facts"]
            .as_array()
            .expect("the facts")
            .iter()
            .map(|f| f["address"].as_str().expect("an address").to_string())
            .collect();
        assert!(!addresses.is_empty(), "a part carries at least one fact");
        if let Some(promised) = left_out_by_the_last {
            let from_here =
                addresses.len() as u64 + body["not_shown"]["count"].as_u64().unwrap_or(0);
            assert_eq!(
                promised, from_here,
                "what was left out is what the rest return"
            );
        }
        seen.extend(addresses);
        match body["not_shown"]["offset"].as_u64() {
            Some(next) => {
                left_out_by_the_last = body["not_shown"]["count"].as_u64();
                offset = next;
            }
            None => break,
        }
    }
    assert!(parts > 1, "the facts took several parts: {parts}");
    let mut walked = seen.clone();
    walked.sort();
    walked.dedup();
    assert_eq!(walked.len(), seen.len(), "no fact came back twice");
    let mut written = world.heavy_addresses.clone();
    written.sort();
    assert_eq!(
        walked, written,
        "every fact that was written, and nothing else"
    );
}

/// **A kind past the ceiling is read in parts, and walking on returns every
/// thing once.** The places carry facts and one of them alone is past the
/// ceiling, so its facts are cut where it stands and the answer names the call
/// that reads them; the other places come whole in the parts around it. The
/// objects of all the parts are exactly the places that were written.
#[tokio::test]
async fn a_kind_past_the_ceiling_is_read_in_parts_that_return_each_thing_once() {
    let world = hostile_world().await;
    let mut seen: Vec<String> = Vec::new();
    let mut cut: Vec<String> = Vec::new();
    let mut offset = 0u64;
    let mut parts = 0;
    let mut left_out_by_the_last: Option<u64> = None;
    loop {
        let part = world
            .otto
            .call(
                "recall",
                json!({"kind": "place", "facts": true, "offset": offset}),
            )
            .await;
        parts += 1;
        assert!(parts < 100, "the walk never ends");
        assert!(
            part.size() <= ANSWER_CEILING,
            "part {parts}: {}",
            part.size()
        );
        let body = part.json();
        let objects = body["objects"].as_array().expect("objects");
        assert!(!objects.is_empty(), "a part carries at least one object");
        assert_eq!(body["count"], objects.len(), "{body}");
        for object in objects {
            seen.push(object["id"].as_str().expect("an id").to_string());
            if object["facts_not_shown"].is_object() {
                cut.push(object["id"].as_str().expect("an id").to_string());
                // Every fact is accounted for: carried, or counted as left out.
                let carried = object["facts"].as_array().expect("facts").len();
                let left_out = object["facts_not_shown"]["count"]
                    .as_u64()
                    .expect("a count");
                assert_eq!(
                    carried as u64 + left_out,
                    FACTS_ON_ONE_SUBJECT as u64,
                    "{object}"
                );
                // …and the answer names the call that reads them: this place alone.
                assert!(
                    object["facts_not_shown"]["how_to_proceed"]
                        .as_str()
                        .expect("a way on")
                        .contains(&world.heavy),
                    "{object}"
                );
            }
        }
        if let Some(promised) = left_out_by_the_last {
            let from_here = objects.len() as u64 + body["not_shown"]["count"].as_u64().unwrap_or(0);
            assert_eq!(
                promised, from_here,
                "what was left out is what the rest return"
            );
        }
        match body["not_shown"]["offset"].as_u64() {
            Some(next) => {
                left_out_by_the_last = body["not_shown"]["count"].as_u64();
                offset = next;
            }
            None => break,
        }
    }
    assert!(parts > 1, "the places took several parts: {parts}");
    let mut walked = seen.clone();
    walked.sort();
    walked.dedup();
    assert_eq!(walked.len(), seen.len(), "no place came back twice");
    let mut written = world.place_ids.clone();
    written.sort();
    assert_eq!(
        walked, written,
        "every place that was written, and nothing else"
    );
    assert_eq!(
        cut,
        vec![world.heavy.clone()],
        "only the heavy place had its facts cut, and said so"
    );
}

/// **A kind whose objects carry no facts is filled the same way.** A hundred and
/// twenty things are past the ceiling by their fields alone, so the parts are
/// whole objects, the count each states is what the later parts return, and the
/// things together are exactly those that were written.
#[tokio::test]
async fn a_kind_without_facts_past_the_ceiling_is_read_in_parts_too() {
    let world = hostile_world().await;
    let mut seen: Vec<String> = Vec::new();
    let mut offset = 0u64;
    let mut parts = 0;
    let mut left_out_by_the_last: Option<u64> = None;
    loop {
        let part = world
            .otto
            .call("recall", json!({"kind": "thing", "offset": offset}))
            .await;
        parts += 1;
        assert!(parts < 100, "the walk never ends");
        assert!(
            part.size() <= ANSWER_CEILING,
            "part {parts}: {}",
            part.size()
        );
        let body = part.json();
        let objects = body["objects"].as_array().expect("objects");
        assert!(!objects.is_empty(), "a part carries at least one object");
        if let Some(promised) = left_out_by_the_last {
            let from_here = objects.len() as u64 + body["not_shown"]["count"].as_u64().unwrap_or(0);
            assert_eq!(
                promised, from_here,
                "what was left out is what the rest return"
            );
        }
        seen.extend(
            objects
                .iter()
                .map(|o| o["id"].as_str().expect("an id").to_string()),
        );
        match body["not_shown"]["offset"].as_u64() {
            Some(next) => {
                left_out_by_the_last = body["not_shown"]["count"].as_u64();
                offset = next;
            }
            None => break,
        }
    }
    assert!(parts > 1, "the things took several parts: {parts}");
    let mut walked = seen.clone();
    walked.sort();
    walked.dedup();
    assert_eq!(walked.len(), seen.len(), "no thing came back twice");
    let mut written = world.thing_ids.clone();
    written.sort();
    assert_eq!(
        walked, written,
        "every thing that was written, and nothing else"
    );
}

/// **A kind of places whose keys are different ages is read in parts that all
/// fit, and every place says how old it is.** Ten places are written on two
/// days: colour and size in August, colour again in October. Every object of
/// every part carries the October day it was last learned about and the August
/// day of its size, which is the one key older than the thing, and every part,
/// those fields included, is under the ceiling. The places walked are exactly
/// the places written, so the days were not bought by dropping an object.
#[tokio::test]
async fn a_kind_of_places_of_different_ages_says_each_ones_age_under_the_ceiling() {
    let world = aged_hostile_world().await;
    let mut seen: Vec<String> = Vec::new();
    let mut offset = 0u64;
    let mut parts = 0;
    loop {
        let part = world
            .otto
            .call(
                "recall",
                json!({"kind": "place", "facts": true, "offset": offset}),
            )
            .await;
        parts += 1;
        assert!(parts < 100, "the walk never ends");
        assert!(
            part.size() <= ANSWER_CEILING,
            "part {parts}: {}",
            part.size()
        );
        let body = part.json();
        for object in body["objects"].as_array().expect("objects") {
            assert_eq!(object["as_of"], OCTOBER, "{object}");
            assert_eq!(
                object["fields_as_of"],
                json!({"size": AUGUST}),
                "only the size is older than the place: {object}"
            );
            seen.push(object["id"].as_str().expect("an id").to_string());
        }
        match body["not_shown"]["offset"].as_u64() {
            Some(next) => offset = next,
            None => break,
        }
    }
    assert!(parts > 1, "the places took several parts: {parts}");
    seen.sort();
    let mut written = world.place_ids.clone();
    written.sort();
    assert_eq!(seen, written, "every place that was written, once");
}

/// **One fact larger than the ceiling is served whole, and the pointer that
/// sends a caller to it does not promise otherwise.** A place whose one fact is
/// past the ceiling cannot fit in a part of a kind recall, so its facts are cut
/// and `facts_not_shown` names the call that reads them. That call returns the
/// fact whole, over the ceiling, because a fact is never cut mid-record. The
/// pointer says so; a sentence promising parts that fit would be false for
/// exactly the fact that triggers it.
#[tokio::test]
async fn a_fact_larger_than_the_ceiling_is_served_whole_and_the_pointer_says_so() {
    use jojobot_domain::memory::{EntityId, EntityKind, Guarded, NewEntity, NewFact};

    let first = Story::begin("bot:otto").await;
    let memory = first.memory_store();
    let today: jiff::civil::Date = "2026-10-01".parse().expect("a day");
    let huge = filler(ANSWER_CEILING + 5_000, 3);
    let mut written_len = 0;
    let mut place_ids: Vec<String> = Vec::new();
    for (p, size) in [(0usize, huge.len()), (1, 200)] {
        let id = EntityId::new(EntityKind::PLACE, slug(20_000 + p));
        let added = memory
            .add_entity(NewEntity::new(
                id.clone(),
                format!("Spot {}", slug(20_000 + p)),
                "user-named",
            ))
            .await;
        assert!(
            matches!(added, Ok(Guarded::Written(_))),
            "a place lands: {added:?}"
        );
        let mut fact = NewFact::about(id.clone(), format!("record {p}"), today);
        fact.details = Some(filler(size, p));
        let outcome = memory.capture(fact).await;
        let Ok(Guarded::Written(fact)) = outcome else {
            panic!("a fact about a place that exists lands: {outcome:?}");
        };
        if p == 0 {
            written_len = fact.details.as_deref().map_or(0, str::len);
        }
        place_ids.push(id.as_str().to_string());
    }
    assert!(
        written_len > ANSWER_CEILING,
        "the store kept the fact whole: {written_len}"
    );
    let story = first.restarted().await;
    let otto = story.as_bot("bot:otto").await;
    let heavy = place_ids[0].clone();

    // The kind recall cuts the heavy place's facts where it stands and says where
    // to read them.
    let mut pointer: Option<Value> = None;
    let mut offset = 0u64;
    for _ in 0..5 {
        let part = otto
            .call(
                "recall",
                json!({"kind": "place", "facts": true, "offset": offset}),
            )
            .await;
        assert!(part.size() <= ANSWER_CEILING, "{}", part.size());
        let body = part.json();
        for object in body["objects"].as_array().expect("objects") {
            if object["facts_not_shown"].is_object() {
                assert_eq!(object["id"], heavy.as_str(), "{object}");
                pointer = Some(object["facts_not_shown"].clone());
            }
        }
        match body["not_shown"]["offset"].as_u64() {
            Some(next) => offset = next,
            None => break,
        }
    }
    let pointer = pointer.expect("the heavy place had its facts cut and said so");

    // Read alone, the one fact comes back whole and over the ceiling.
    let (body, size) = otto
        .answer_and_size("recall", json!({"subject": heavy, "facts": true}))
        .await;
    assert!(
        size > ANSWER_CEILING,
        "a fact is never cut mid-record: {size}"
    );
    let facts = body["objects"][0]["facts"].as_array().expect("the facts");
    assert_eq!(facts.len(), 1, "one fact, not cut to none: {size}");
    assert_eq!(
        facts[0]["details"].as_str().map(str::len),
        Some(written_len),
        "the fact came back whole"
    );
    assert!(body.get("not_shown").is_none(), "nothing left to read on");

    // And the pointer says it: this is the one word the old sentence lacked.
    let said = pointer["how_to_proceed"].as_str().expect("a way on");
    assert!(said.contains(&heavy), "it names the place: {said}");
    assert!(
        said.contains("whole"),
        "it says the fact comes back whole: {said}"
    );
}
