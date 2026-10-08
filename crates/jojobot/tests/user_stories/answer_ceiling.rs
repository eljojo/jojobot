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

/// The verbs whose widest call still passes the ceiling, each named for the
/// call that was measured. A later slice that makes one fit removes its line
/// here, and a verb that starts passing the ceiling without being added fails
/// the case that compares the two.
const STILL_OVER: &[&str] = &[
    "recall one subject with its facts",
    "recall every thing of a kind",
];

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
    use jojobot_domain::mailbox::{MailboxName, Mailboxes, NewMessage};
    use jojobot_domain::memory::{EntityId, EntityKind, NewEntity, NewFact};

    let first = Story::begin("bot:otto").await;
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
                "user-named",
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
    let heavy = handles[ENTITIES - 1].clone();
    for i in 0..FACTS_ON_ONE_SUBJECT {
        memory
            .capture(NewFact::about(
                heavy.clone(),
                format!("long record {i}: {}", filler(780, i)),
                today,
            ))
            .await
            .expect("a fact lands");
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

    let story = first.restarted().await;
    let otto = story.as_bot("bot:otto").await;
    let sigma = story.as_bot("bot:sigma").await;
    World {
        _story: story,
        otto,
        sigma,
        heavy: heavy.as_str().to_string(),
        marker_addresses,
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
