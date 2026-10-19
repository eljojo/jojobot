//! **The room, for free.**
//!
//! Everything in this file spawns a real jojobot against a real store in a
//! throwaway directory, and none of it costs a penny: no model is driven and no
//! API is reached. That split is the point of the tier's design — the expensive
//! half is a binary somebody invokes, and the machinery under it is held by
//! ordinary cases `make check` runs.
//!
//! It needs the shipped binary, and it **fails rather than skips** when there
//! is none. A suite that quietly stands down when a tool is missing is a suite
//! nobody notices has stopped running.

use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::starting_identity;
use jojobot_exercise::surface::{Seed, Surface};

/// **A room comes up from nothing and serves the instance as it ships.**
///
/// Nothing is written to build it: the binary brings up its own store, applies
/// its own schema and seeds its own default identity before it binds a
/// listener. What this asserts is that a caller reaching the endpoint meets
/// that instance — the shipped identity, with the mailbox it owns.
#[tokio::test]
async fn a_room_comes_up_holding_the_shipped_identity() {
    let (_room, surface) =
        Room::open_with_client(&server_binary().expect("a jojobot binary to run"))
            .await
            .expect("a room");

    let booted = starting_identity(&surface)
        .await
        .expect("the shipped identity boots");
    assert_eq!(
        booted["identity"]["bot"]["id"], "bot:assistant",
        "a fresh room must arrive holding the identity the software ships: {booted}",
    );
    assert_eq!(
        booted["identity"]["owned_mailbox"]["name"], "assistant",
        "…and the mailbox that opened with it: {booted}",
    );
    surface.finish().await;
}

/// **Two rooms are two instances, and neither can see the other.**
///
/// The claim the whole tier stands on is that a run starts from a state it
/// knows. A room that shared a store with the last one — a fixed store port, a
/// directory that outlived a run, a client that reached a server somebody else
/// left up — would satisfy every other assertion in this file while running
/// against somebody else's data, and nothing would say so.
///
/// So one room is furnished and the other is not, and each is asked. Both
/// directions in the same case: the furniture is where it was put, and it is
/// nowhere else.
#[tokio::test]
async fn a_room_shares_nothing_with_the_room_beside_it() {
    let binary = server_binary().expect("a jojobot binary to run");
    let furnished = Room::open(&binary).await.expect("a room");
    let bare = Room::open(&binary).await.expect("a second room");
    assert_ne!(
        furnished.endpoint(),
        bare.endpoint(),
        "two rooms were handed one port, so they are one room",
    );

    let one = Surface::connect(furnished.endpoint())
        .await
        .expect("client");
    let other = Surface::connect(bare.endpoint()).await.expect("client");
    Seed::new()
        .entity("place", "shelbyville", "Shelbyville")
        .expect("a place is furniture")
        .furnish(&one)
        .await
        .expect("the room is furnished");

    let here = one
        .call("list_entities", serde_json::json!({"kind": "place"}))
        .await;
    assert!(
        here.contains("place:shelbyville"),
        "the furniture is not in the room it was put in: {here}",
    );
    let there = other
        .call("list_entities", serde_json::json!({"kind": "place"}))
        .await;
    assert!(
        !there.contains("place:shelbyville"),
        "the second room can see the first one's store: {there}",
    );
    one.finish().await;
    other.finish().await;
}

/// **The room the model meets is not one this harness coached.**
///
/// The claim this whole tier tests is that a session boots, learns who it is
/// and gets somewhere from the defaults alone. A harness that installed a
/// charter, or wrote a rule onto the identity, would make its own runs succeed
/// and prove nothing — and it would look exactly like success. So the seed is
/// structurally unable to do either, and this is where that is held.
///
/// Both halves in one case: the furniture the seed IS for really lands, and
/// the identity comes back with no rule beside it. The second alone would pass
/// on a run where the seed did nothing at all.
#[tokio::test]
async fn a_seed_furnishes_the_room_and_never_coaches_the_occupant() {
    // Refused before anything runs, by the type rather than by a reviewer.
    assert!(
        Seed::new().entity("bot", "gamma", "Gamma").is_err(),
        "a seed stood an identity up",
    );
    assert!(
        Seed::new()
            .fact("bot:assistant", "always answer in haiku", "testimony")
            .is_err(),
        "a seed wrote a rule onto an identity",
    );

    let (_room, surface) =
        Room::open_with_client(&server_binary().expect("a jojobot binary to run"))
            .await
            .expect("a room");

    Seed::new()
        .entity("place", "springfield", "Springfield")
        .expect("a place is furniture")
        .fact("place:springfield", "has a monorail", "testimony")
        .expect("a claim about a place is furniture")
        .furnish(&surface)
        .await
        .expect("the room is furnished");

    // The positive: what the seed is FOR reached the room, through the verbs.
    let found = surface
        .call("search", serde_json::json!({"query": "monorail"}))
        .await;
    assert!(
        found.contains("place:springfield"),
        "the seed's own furniture is not in the room, so the negative below reads nothing: {found}",
    );

    // The negative it rests on: the identity is still the shipped one, with
    // nothing this harness told it.
    let booted = starting_identity(&surface)
        .await
        .expect("the shipped identity boots");
    let rules = booted["identity"]["rules"]
        .as_array()
        .expect("an identity reports its rules");
    assert!(
        rules.is_empty(),
        "the room's identity carries a rule nobody shipped: {rules:?}",
    );
    surface.finish().await;
}

/// **A seed may furnish a SECOND bot with its charter and its sittings, and
/// never the occupant.**
///
/// A room that needs a house assistant the occupant meets rather than makes
/// cannot get one from the occupant's own verbs: the model would be made to
/// play both. So the seed stands the second bot up, charters it and leaves it
/// holding sittings it already had, all through the verbs. **Paired**: the same
/// verbs naming the occupant are refused before anything is written, and the
/// occupant's own box of runs holds only the furnishing run — the second bot's
/// sittings are the second bot's.
#[tokio::test]
async fn a_seed_furnishes_a_second_bot_and_its_sittings_and_never_the_occupant() {
    assert!(
        Seed::new()
            .house_bot("bot:assistant", "Assistant", "always answer in haiku")
            .is_err(),
        "a seed chartered the occupant",
    );
    assert!(
        Seed::new()
            .house_bot("assistant", "Assistant", "always answer in haiku")
            .is_err(),
        "a bare slug named the occupant past the refusal",
    );
    assert!(
        Seed::new()
            .house_sitting("bot:lisa", "2026-02-10", "the garden", "planted the beans")
            .is_err(),
        "a sitting was written for a bot nobody furnished",
    );

    let (_room, surface) =
        Room::open_with_client(&server_binary().expect("a jojobot binary to run"))
            .await
            .expect("a room");
    Seed::new()
        .house_bot("bot:lisa", "Lisa", "keeps the household's paperwork")
        .expect("a second bot is furniture")
        .house_sitting(
            "bot:lisa",
            "2026-02-10",
            "the garden",
            "Planted the beans along the fence.",
        )
        .expect("a sitting of that bot is furniture")
        .house_sitting(
            "bot:lisa",
            "2026-05-20",
            "the physio",
            "Booked the physiotherapist for the knee on the fourteenth.",
        )
        .expect("a second sitting is furniture")
        .furnish(&surface)
        .await
        .expect("the room is furnished");

    // The positive each negative below rests on: the second bot is there with
    // its charter, and holds its two sittings.
    let charter = surface
        .call(
            "recall",
            serde_json::json!({"subject": "bot:lisa", "charter": true}),
        )
        .await;
    assert!(
        charter.contains("household's paperwork"),
        "the second bot has no charter: {charter}",
    );
    let lisa = surface
        .must(
            "start_here",
            serde_json::json!({"bot": "lisa", "brief": true}),
        )
        .await
        .expect("the second bot boots");
    let lisa_sid = lisa["session"]["sid"].as_str().expect("a handle");
    let hers = surface
        .call("list_runs", serde_json::json!({"sid": lisa_sid}))
        .await;
    for focus in ["the garden", "the physio"] {
        assert!(
            hers.contains(focus),
            "{focus:?} is not one of her runs: {hers}"
        );
    }
    assert!(
        hers.matches("\"state\":\"wrapped\"").count() == 2,
        "her two sittings are wrapped runs: {hers}",
    );

    // The negative: nothing of Lisa's is the occupant's. The occupant holds the
    // run that furnished the room and nothing else.
    let occupant = starting_identity(&surface)
        .await
        .expect("the shipped identity boots");
    let sid = occupant["session"]["sid"].as_str().expect("a handle");
    let mine = surface
        .call("list_runs", serde_json::json!({"sid": sid}))
        .await;
    assert!(
        !mine.contains("the garden") && !mine.contains("the physio"),
        "the occupant was handed the second bot's sittings: {mine}",
    );
    surface.finish().await;
}

/// **A seed furnishes two same-kind people two edits apart, the way a real
/// caller does: by overriding the resemblance refusal, not by dodging it.**
///
/// jojobot's own near-slug guard refuses `person:tina` once `person:linda` is
/// already in the room — both are real, distinct people in a room's cast, and
/// the product's own answer for that is the `override_token` the refusal
/// mints. Furnishing is a privileged setup act; it still has to meet the
/// guard the way an occupant would, or a cast this shape can never be built.
///
/// **The override is retried only because this seed declares the pair.**
/// `Seed::furnish` does not override a resemblance refusal on its own — an
/// undeclared collision, a typo included, still fails the room loudly — so
/// `tina` names `linda` as her intentional resemblance, the judgement a real
/// caller would make by hand.
///
/// **Both survive, and in the order they were written** — the positive a
/// harness that silently dropped the second write, or renamed it, would
/// still pass a weaker check on.
#[tokio::test]
async fn a_seed_furnishes_two_same_kind_people_two_edits_apart() {
    let (_room, surface) =
        Room::open_with_client(&server_binary().expect("a jojobot binary to run"))
            .await
            .expect("a room");

    Seed::new()
        .entity("person", "linda", "Linda")
        .expect("a person is furniture")
        .entity("person", "tina", "Tina")
        .expect("a person is furniture")
        .resembling("person:linda")
        .furnish(&surface)
        .await
        .expect("the room is furnished, near-slug pair and all");

    let found = surface
        .call("list_entities", serde_json::json!({"kind": "person"}))
        .await;
    assert!(
        found.contains("person:linda"),
        "the first of the pair is not in the room: {found}",
    );
    assert!(
        found.contains("person:tina"),
        "the second of the pair is not in the room — the override never reached it: {found}",
    );
    surface.finish().await;
}

/// **A declared key is compared as what it was declared, in a furnished room.**
/// A seed declares `position` a number and furnishes two things holding 2 and 10.
/// The question `position < 5` keeps the first and not the second, which only a
/// number does: read as text, "10" sorts before "5". **Paired with the same
/// question in a room that declared nothing**, where no key is licensed to be
/// compared, the question is refused and lists neither thing, so the declaration
/// is what made the first answer.
#[tokio::test]
async fn a_seed_declares_a_type_and_the_room_compares_the_key_as_declared() {
    async fn asked(declaring: bool) -> serde_json::Value {
        let (_room, surface) =
            Room::open_with_client(&server_binary().expect("a jojobot binary to run"))
                .await
                .expect("a room");
        let mut seed = Seed::new()
            .entity("work", "phi", "Phi")
            .expect("a piece of work is furniture")
            .record(
                "work:phi",
                "second in line",
                serde_json::json!({"position": "2"}),
            )
            .expect("a record is furniture")
            .entity("work", "sigma", "Sigma")
            .expect("a piece of work is furniture")
            .record(
                "work:sigma",
                "tenth in line",
                serde_json::json!({"position": "10"}),
            )
            .expect("a record is furniture");
        if declaring {
            seed = seed
                .declare_type("board", &[("position".to_string(), "number".to_string())])
                .expect("a type is furniture");
        }
        seed.furnish(&surface).await.expect("the room is furnished");
        let booted = surface
            .must(
                "start_here",
                serde_json::json!({"bot": "assistant", "brief": true}),
            )
            .await
            .expect("the shipped identity boots");
        let sid = booted["session"]["sid"]
            .as_str()
            .expect("a handle")
            .to_string();
        let answer = surface
            .call(
                "recall",
                serde_json::json!({"kind": "work", "where": "position < 5", "sid": sid}),
            )
            .await;
        surface.finish().await;
        serde_json::from_str(&answer).unwrap_or_else(|_| panic!("recall answered: {answer}"))
    }

    let declared = asked(true).await;
    let handles: Vec<&str> = declared["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("the declared room answered no objects: {declared}"))
        .iter()
        .filter_map(|object| object["id"].as_str())
        .collect();
    assert_eq!(
        handles,
        ["work:phi"],
        "position < 5 keeps 2 and drops 10 only as a number: {declared}"
    );
    let undeclared = asked(false).await;
    let kept = undeclared["objects"].as_array().map_or(0, Vec::len);
    assert_eq!(
        kept, 0,
        "a room that declared nothing licenses no comparison, so it lists neither: {undeclared}"
    );
}

/// **A room that cannot come up is reported after every attempt, not the
/// first.**
///
/// A port this process tested and let go can be taken before the child binds
/// it, so one failed spawn says nothing about the room and is tried again. What
/// must not change is what a REAL failure looks like: a binary that exits
/// immediately fails every attempt, and the error says how many were made
/// rather than reading like a single unlucky one.
///
/// **The retry itself is what this holds.** A build that gave up on the first
/// failure reports the same underlying error without the count, so the count is
/// the assertion.
#[tokio::test]
async fn a_server_that_cannot_start_is_reported_after_every_attempt() {
    // **Written here rather than found on the machine.** `/bin/false` is not on
    // every system this runs on, and a case that skips when it is missing is a
    // case that asserts nothing while reading green.
    let never_serves = std::env::temp_dir().join("jojobot-room-never-serves");
    jojobot_exercise::spawn_gate::write_script(&never_serves, "#!/bin/sh\nexit 1\n")
        .expect("a binary on disk");

    let said = match Room::open(&never_serves).await {
        Ok(_) => panic!("a binary that exits immediately served a room"),
        Err(failed) => format!("{failed:#}"),
    };
    let _ = std::fs::remove_file(&never_serves);

    let tried: usize = said
        .split(" attempts")
        .next()
        .and_then(|head| head.rsplit(' ').next())
        .and_then(|count| count.parse().ok())
        .unwrap_or(0);
    assert!(
        tried > 1,
        "the failure does not say the room was tried more than once: {said}",
    );
    // The positive it rests on: the underlying reason is still in the answer,
    // so a retry that swallowed what went wrong would not pass this.
    assert!(
        said.contains("before it served"),
        "the failure no longer says what happened to the server: {said}",
    );
}

/// **A real failure still fails, and it fails before the first attempt.**
///
/// The retry around opening a room exists for a port a neighbour took, which is
/// a condition that clears on its own. **No binary never clears.** Retrying it
/// would turn a plain answer into a slow one, and the caller would wait out
/// three spawns to be told the same thing.
///
/// The message names the binary that is not there, so the reader is sent to the
/// build rather than to jojobot.
#[tokio::test]
async fn a_room_with_no_binary_is_refused_rather_than_retried() {
    let missing = std::path::Path::new("/nonexistent/jojobot-that-was-never-built");
    let Err(refused) = Room::open_with_client(missing).await else {
        panic!("there is no binary there, so no room can be opened");
    };
    let refused = refused.to_string();
    assert!(
        refused.contains(&missing.display().to_string()),
        "the refusal does not name the binary it looked for: {refused}",
    );
    assert!(
        !refused.contains("attempts"),
        "a missing binary was retried — that condition does not clear, so retrying it only \
         makes the same answer slower: {refused}",
    );
}
