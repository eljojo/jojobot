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

use serde_json::json;

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
    "list_sent with bodies",
    "read_mailbox, everything waiting",
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
    for i in 0..MESSAGES {
        mail.post_message(NewMessage {
            mailbox: MailboxName("otto".into()),
            body: filler(700, i),
            subject: Some(format!("report {i}")),
            sender: "bot:sigma".into(),
            sent_at: jiff::Timestamp::now(),
            in_reply_to: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
        })
        .await
        .expect("a message lands");
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
