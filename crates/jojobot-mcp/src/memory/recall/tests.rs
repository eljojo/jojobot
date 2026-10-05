use super::*;
use crate::harness::*;
use crate::memory::RenameEntityArgs;
use crate::memory::testing::*;
use crate::session::testing::journal_entry;
use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::memory::Boot;
use jojobot_domain::session::testing::InMemorySessions;
use jojobot_domain::teaching::testing::InMemoryTeachings;

/// A fresh, empty store, shared across a setup handler and a test handler
/// so both see the same entities — the shape the search-routed path needs
/// proving against: a store real writes landed in, and a search port
/// configured separately from it.
fn shared_memory() -> Arc<InMemoryMemory> {
    Arc::new(InMemoryMemory::booted())
}

/// A handler over a given store and a given search port — the two halves
/// [`shared_memory`] and a caller-configured [`SpySearch`] let a test hold
/// apart, where [`handler`] fixes both.
fn handler_on(memory: Arc<InMemoryMemory>, search: Arc<SpySearch>) -> Jojobot {
    Jojobot::new(
        memory,
        search,
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(InMemorySessions::new()),
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    )
}

/// **The real entity a setup pass captured**, read back off the shared
/// store rather than hand-built — so the `Hit` a configured [`SpySearch`]
/// answers with is the thing that was actually written, not a guess at
/// its shape.
async fn captured(memory: &InMemoryMemory, id: &str) -> Entity {
    memory
        .list_entities(None)
        .await
        .expect("list ok")
        .into_iter()
        .find(|e| e.id.as_str() == id)
        .unwrap_or_else(|| panic!("{id} was not captured"))
}

/// The whole-page query, spelled once: one handle, its records and nothing
/// else. **It asks for the records**, which are off by default — a case
/// asserting on a claim's own wording has to ask for the claim.
fn of(subject: &str) -> RecallArgs {
    RecallArgs {
        view: None,
        subject: Some(subject.into()),
        kind: None,
        answers_type: None,
        fields: None,
        facts: Some(true),
        stood_for: None,
        prose: None,
        charter: None,
        follow: None,
        overdue: None,
        near: None,
        sid: None,
        history: None,
        history_record: None,
        history_most: None,
        values: None,
        values_most: None,
        built_on: None,
        backing: None,
    }
}

/// 🚨 **`near`'s `clock: "happened_at"` reaches the day a thing happened,
/// through the served surface** — not just the domain arithmetic
/// [`graph::tests`] already proves.
///
/// One claim, recorded on one day and carrying a `happened_at` far from
/// it. Both halves, on the SAME claim: a window around the day it
/// happened finds it, and a window around the day it was recorded does
/// not — the positive half proves the clock reaches its own field
/// through `parse_clock` and `NearArgs`, not merely that nothing
/// crashed.
#[tokio::test]
async fn nears_happened_at_clock_reaches_the_served_surface() {
    let jojobot = crate::harness::handler();
    let sid = writing_as(&jojobot);
    ensure(&jojobot, "person:milhouse").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(sid.clone()),
            recorded_at: Some("2026-08-16".into()),
            happened_at: Some("2027-06-01".into()),
            ..capture_args("person:milhouse", "booked the trip for next June")
        },
    )
    .await;

    let near_the_event = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                sid: Some(sid.clone()),
                near: Some(NearArgs {
                    day: Some("2027-06-04".into()),
                    within_days: Some(7),
                    clock: Some("happened_at".into()),
                }),
                ..of("person:milhouse")
            }))
            .await
            .expect("recall ok"),
    )
    .to_string();
    assert!(
        near_the_event.contains("booked the trip"),
        "a window around the day the claim HAPPENED did not find it: {near_the_event}",
    );

    let near_the_recording = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                sid: Some(sid),
                near: Some(NearArgs {
                    day: Some("2026-08-18".into()),
                    within_days: Some(7),
                    clock: Some("happened_at".into()),
                }),
                ..of("person:milhouse")
            }))
            .await
            .expect("recall ok"),
    )
    .to_string();
    assert!(
        !near_the_recording.contains("booked the trip"),
        "a window around the day the claim was RECORDED found it on the happened-at clock, \
             so this clock is reading the wrong field: {near_the_recording}",
    );
}

/// **A carrier the read has never seen appears in the same answer, and the
/// read does not change.**
///
/// This is the only thing that tells a general read from one that looks
/// general. The shape alone proves nothing: a read that asks a list it
/// built itself has exactly the same code as one that asks a list it was
/// given, and only a carrier from outside can say which it is.
///
/// **The stub lives here and nowhere else.** Nothing in the software ships
/// it, nothing registers it, and the read has no branch for its kind — it
/// is handed to the handler and turns up in the answer.
///
/// **Found by its interface's key, never by a kind — the case that proves
/// the point.** `Promises` claims no kind at all: there is nothing left on
/// `Carrier` to claim one with. A `person`, a kind its own code never
/// names, still turns up the moment it carries the key — proving the read
/// is genuinely structural and not a kind check wearing a new name.
/// Paired with the negative: a thing carrying NONE of the key, whatever
/// its kind, stays out — or this passes against a read that keeps
/// everything.
#[tokio::test]
async fn a_carrier_the_read_never_heard_of_answers_in_the_same_read() {
    /// A promise falls due on the day it says it does. Two lines of
    /// arithmetic that share nothing with a loop's, and no kind at all —
    /// `Carrier` has nowhere left to put one.
    struct Promises;

    impl attention::Carrier for Promises {
        fn interface(&self) -> jojobot_domain::memory::types::DeclaredType {
            jojobot_domain::memory::types::DeclaredType::new(
                "promises",
                vec![jojobot_domain::memory::types::Field::new(
                    "promised_for",
                    jojobot_domain::memory::types::ValueType::Date,
                )],
            )
        }

        fn due(&self, fields: &std::collections::BTreeMap<String, String>) -> attention::Due {
            match fields.get("promised_for").map(|held| held.trim().parse()) {
                Some(Ok(day)) => attention::Due::On(day),
                Some(Err(_)) => attention::Due::Unreadable,
                None => attention::Due::Never,
            }
        }
    }

    let mut carriers = attention::shipped();
    carriers.push(Box::new(Promises));
    let jojobot = crate::harness::handler_carrying(carriers);
    let sid = writing_as(&jojobot);

    // One owed and one not — the whole answer turns on the carrier's own
    // arithmetic, which the read has never read.
    for (slug, promised_for) in [("phi", "2026-08-01"), ("sigma", "2026-09-30")] {
        ensure(&jojobot, &format!("work:{slug}")).await;
        capture_ok(
            &jojobot,
            CaptureArgs {
                sid: Some(sid.clone()),
                fields: Some(
                    [("promised_for".to_string(), promised_for.to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args(&format!("work:{slug}"), "a promise")
            },
        )
        .await;
    }
    // A DIFFERENT kind, carrying the same key and owed the same way — the
    // carrier's own code never says "person", and it is found anyway.
    ensure(&jojobot, "person:beta").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(sid.clone()),
            fields: Some(
                [("promised_for".to_string(), "2026-08-01".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("person:beta", "a person can promise something too")
        },
    )
    .await;
    // And a thing carrying none of the key at all, whatever it is.
    ensure(&jojobot, "person:alpha").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(sid.clone()),
            ..capture_args("person:alpha", "a person is never late")
        },
    )
    .await;

    let owed = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                sid: Some(sid),
                // **One selection reaching every kind**, which is what
                // makes this the SAME answer rather than two reads
                // compared.
                fields: Some(vec![KeyFilterArgs {
                    key: Some("promised_for".into()),
                    value: None,
                    compare: None,
                    scope: None,
                }]),
                overdue: Some(OverdueArgs {
                    as_of: Some("2026-08-19".into()),
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    let said = owed.to_string();
    assert!(
        said.contains("work:phi"),
        "the stub carrier's own due moment reached the answer: {said}",
    );
    assert!(
        !said.contains("work:sigma"),
        "…and its arithmetic decided, rather than everything of that kind coming back: {said}",
    );
    assert!(
        said.contains("person:beta"),
        "found by the key it carries, over a kind the carrier's own code never names: {said}",
    );
    assert!(
        !said.contains("person:alpha"),
        "carrying none of any carrier's key owes nothing, whatever its kind: {said}",
    );
}

/// **A read can ask who backs each value it is about to act on.**
///
/// Two writes to one key — a guess, then the user's own word — and the
/// answer names the certainty of the one that WON. **The loser's is not
/// reported**, which is what stops this passing against a build that hands
/// back whichever claim it met first.
#[tokio::test]
async fn a_read_can_ask_who_backs_the_value_it_is_about_to_act_on() {
    let jojobot = handler();
    let sid = writing_as(&jojobot);
    async fn wrote(jojobot: &Jojobot, sid: &str, content: &str, rent: &str, testimony: bool) {
        capture_ok(
            jojobot,
            CaptureArgs {
                sid: Some(sid.to_string()),
                provenance: Some(if testimony { "testimony" } else { "inference" }.into()),
                fields: Some(
                    [("rent".to_string(), rent.to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("person:alpha", content)
            },
        )
        .await;
    }
    wrote(
        &jojobot,
        &sid,
        "worked it out from the listing",
        "900",
        false,
    )
    .await;
    wrote(&jojobot, &sid, "he said what the rent is", "950", true).await;

    let read = jojobot
        .recall(Parameters(RecallArgs {
            sid: Some(sid.clone()),
            backing: Some(true),
            facts: None,
            ..of("person:alpha")
        }))
        .await
        .expect("the read answers");
    let object = json_of(&read)["objects"][0].clone();
    assert_eq!(
        object["fields"]["rent"], "950",
        "the newest write is not the value: {object}"
    );
    assert_eq!(
        object["fields_backing"]["rent"]["provenance"], "testimony",
        "the value the user stated reads as a guess: {object}",
    );

    // **The answer that asked does not also carry the pointer**, which is
    // what stops the check below passing against a build that names the
    // call on every answer whether or not it left anything out.
    assert!(
        object["fields_backing_note"].is_null(),
        "the answer carrying the backing also tells the caller how to get it: {object}",
    );

    // **A read that did not ask carries no backing — and says so.** A value
    // arriving bare would otherwise read as one nobody stands behind, which
    // is a different answer from one nobody asked about.
    let plain = jojobot
        .recall(Parameters(RecallArgs {
            sid: Some(sid),
            facts: None,
            ..of("person:alpha")
        }))
        .await
        .expect("the read answers");
    let object = json_of(&plain)["objects"][0].clone();
    assert!(
        object["fields_backing"].is_null(),
        "a read that asked for no backing was given some anyway: {object}",
    );
    assert!(
        object["fields_backing_note"]
            .as_str()
            .is_some_and(|note| note.contains("backing")),
        "the answer leaves the backing out and does not say it exists: {object}",
    );
}

/// 🚨 **The caveat rides the value it is about.**
///
/// A record may say why its value is what it is — that a date was
/// approximated, what a number counts, what the operator hedged. **Read the
/// THING and the values come back folded, and that sentence was one hop
/// away with nothing pointing at it**: the page a person actually looks at
/// was the one place the caveat was missing, and a value with a caveat read
/// as flat fact.
///
/// **Paired, and the pair is the whole case.** A value whose record carries
/// a note carries it here; a value whose record carries none carries no key
/// at all. ⛔️ **The positive alone passes against a build that always emits
/// the field**, and an empty string would be worse than the absence — a
/// reader cannot tell it from a note somebody wrote saying nothing.
#[tokio::test]
async fn a_value_whose_record_says_why_carries_that_note_and_one_with_none_carries_no_key() {
    let jojobot = handler();
    let sid = writing_as(&jojobot);
    async fn wrote(jojobot: &Jojobot, sid: &str, key: &str, value: &str, why: Option<&str>) {
        capture_ok(
            jojobot,
            CaptureArgs {
                sid: Some(sid.to_string()),
                provenance: Some("testimony".into()),
                details: why.map(str::to_string),
                fields: Some([(key.to_string(), value.to_string())].into_iter().collect()),
                ..capture_args("person:alpha", &format!("what the {key} is"))
            },
        )
        .await;
    }
    wrote(
        &jojobot,
        &sid,
        "moved_in",
        "2026-08-15",
        Some("exact day not given, approximated as mid-summer"),
    )
    .await;
    wrote(&jojobot, &sid, "rent", "950", None).await;

    let read = jojobot
        .recall(Parameters(RecallArgs {
            sid: Some(sid),
            backing: Some(true),
            facts: None,
            ..of("person:alpha")
        }))
        .await
        .expect("the read answers");
    let backing = json_of(&read)["objects"][0]["fields_backing"].clone();

    assert!(
        backing["moved_in"]["note"]
            .as_str()
            .is_some_and(|note| note.contains("approximated")),
        "the value reads as flat fact and what its record says about it is not here: \
             {backing}",
    );
    assert!(
        backing["rent"]["note"].is_null(),
        "a value whose record says nothing carries a note key anyway: {backing}",
    );
}

/// **Two runs in two zones disagree about whether a reading has gone
/// stale, and both are right.**
///
/// Whether a claim has passed the day its writer said it stays good is a
/// question about a DAY, and which day it is belongs to the run asking.
/// The marker read a clock in UTC, so a run west of it was told a claim was
/// stale while its own calendar still said the day had not arrived.
///
/// **One stored claim, read twice.** The day it stays good is the one the
/// eastern run has already passed and the western one has not, so the two
/// answers must differ — **and the pair is what proves it: a build reading
/// one clock answers both reads the same, whichever clock it reads.**
#[tokio::test]
async fn two_runs_in_two_zones_disagree_about_a_stale_reading() {
    let jojobot = handler();
    make_bot(&jojobot, "otto").await;
    ensure(&jojobot, "person:alpha").await;

    // Twenty-six hours apart, the widest the map goes, so their days never
    // coincide. Both answer `new`: a bot may have several runs at once,
    // which is what lets one case hold two of them in two zones.
    let behind = booted_in(&jojobot, "otto", "Etc/GMT+12", Some("new")).await;
    let ahead = booted_in(&jojobot, "otto", "Pacific/Kiritimati", Some("new")).await;

    // The claim stays good until the day it is in the EASTERN run — which
    // that run has reached and the western one has not.
    let day_in = |zone: &str| {
        jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::get(zone).expect("a zone"))
            .date()
    };
    let stays_good_until = day_in("Etc/GMT+12");
    capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(behind.clone()),
            stale_after: Some(stays_good_until.to_string()),
            ..capture_args("person:alpha", "the rent is 900 a month")
        },
    )
    .await;
    assert_ne!(
        day_in("Etc/GMT+12"),
        day_in("Pacific/Kiritimati"),
        "the two zones share a day, so this case can prove nothing today",
    );

    let read_by = async |sid: &str| {
        let read = jojobot
            .recall(Parameters(RecallArgs {
                sid: Some(sid.to_string()),
                ..of("person:alpha")
            }))
            .await
            .expect("the read answers");
        json_of(&read)["objects"][0]["facts"][0]["stale"].clone()
    };

    assert!(
        read_by(&ahead).await == serde_json::json!(true),
        "the run whose day is past the one the claim stays good for reads it as fresh",
    );
    assert!(
        read_by(&behind).await.is_null(),
        "the run whose day has not reached it yet is told the reading has gone stale",
    );
}

/// **A claim past the day somebody set says so when it is read.**
///
/// It is not false and nothing here says it is: it is unverified, and a
/// reader is told to confirm it before acting. **Nothing fires** — no
/// sweep, no reminder; the claim says it when somebody looks.
///
/// Three claims, because the negative alone proves nothing: one past its
/// day, one still inside it, and one nobody set a day on. ⚠️ **The second
/// and third are what stop this passing on a build that marks everything
/// stale, and on one that reads absence as staleness.**
#[tokio::test]
async fn a_claim_past_the_day_it_stays_good_says_so_and_the_others_read_clean() {
    let jojobot = handler();
    let sid = writing_as(&jojobot);
    async fn claim(jojobot: &Jojobot, sid: &str, content: &str, stale_after: Option<&str>) {
        capture_ok(
            jojobot,
            CaptureArgs {
                sid: Some(sid.to_string()),
                stale_after: stale_after.map(str::to_string),
                ..capture_args("person:alpha", content)
            },
        )
        .await;
    }
    claim(
        &jojobot,
        &sid,
        "the rent is 900 a month",
        Some("2020-01-01"),
    )
    .await;
    claim(
        &jojobot,
        &sid,
        "the lease runs to the summer",
        Some("2099-01-01"),
    )
    .await;
    claim(&jojobot, &sid, "she keeps a spare key under the pot", None).await;

    let read = jojobot
        .recall(Parameters(RecallArgs {
            sid: Some(sid.clone()),
            ..of("person:alpha")
        }))
        .await
        .expect("the read answers");
    let facts = json_of(&read)["objects"][0]["facts"].clone();
    let of_claim = |needle: &str| {
        facts
            .as_array()
            .expect("the records")
            .iter()
            .find(|fact| fact["content"].as_str().is_some_and(|c| c.contains(needle)))
            .unwrap_or_else(|| panic!("no record saying {needle}: {facts}"))
            .clone()
    };

    let stale = of_claim("rent");
    assert!(
        stale["stale"] == serde_json::json!(true) && stale["stale_note"].is_string(),
        "a claim past its day does not say so: {stale}",
    );
    assert_eq!(
        stale["stale_after"], "2020-01-01",
        "the day itself does not come back: {stale}",
    );

    let fresh = of_claim("lease");
    assert!(
        fresh["stale"].is_null(),
        "a claim inside its window is reported as wanting a look: {fresh}",
    );
    assert_eq!(fresh["stale_after"], "2099-01-01");

    // **Absence is not staleness.** Most claims never need looking at
    // again, and a build that read a missing day as a passed one would make
    // every record in the store suspect.
    let ordinary = of_claim("spare key");
    assert!(
        ordinary["stale"].is_null(),
        "a claim nobody set a day on is reported as wanting a look: {ordinary}",
    );
    assert!(
        ordinary["stale_after"].is_null(),
        "a claim nobody set a day on came back carrying one: {ordinary}",
    );
}

/// **The values a key already holds, so a caller picks one instead of
/// inventing a spelling.**
///
/// A dynamic enum rather than a constraint: the answer says what is
/// recorded and how much of it, and **a value nobody has used is still
/// written**. The second half of this case is what stops the first from
/// being satisfied by a build that turned the list into a rule.
///
/// **It also cannot be satisfied by a build that ignores the store.** The
/// colour written half way through is invented here, so a hardcoded list
/// cannot contain it, and the counts change between the two reads.
#[tokio::test]
async fn recall_says_which_values_a_key_already_holds_and_refuses_no_new_one() {
    let jojobot = handler();
    let sid = writing_as(&jojobot);
    async fn paint(jojobot: &Jojobot, sid: &str, handle: &str, colour: &str) {
        capture_ok(
            jojobot,
            CaptureArgs {
                sid: Some(sid.to_string()),
                fields: Some(
                    [("colour".to_string(), colour.to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args(handle, "what colour it is")
            },
        )
        .await;
    }
    async fn asking(jojobot: &Jojobot, sid: &str, key: &str) -> serde_json::Value {
        let answered = jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("thing".into()),
                values: Some(key.to_string()),
                sid: Some(sid.to_string()),
                // The selection is the kind, not one handle: the values in
                // use are asked of every thing, which is the question a
                // caller picking a colour is asking.
                subject: None,
                facts: None,
                ..of("thing:kettle")
            }))
            .await
            .expect("a read of what is in use is an answer");
        json_of(&answered)
    }
    paint(&jojobot, &sid, "thing:kettle", "dark green").await;
    paint(&jojobot, &sid, "thing:jukebox", "dark green").await;
    paint(&jojobot, &sid, "thing:floor-pump", "amber").await;

    let body = asking(&jojobot, &sid, "colour").await;
    let values = &body["values"];
    assert_eq!(values["key"], "colour", "the answer names the key: {body}");
    assert_eq!(
        values["in_use"],
        serde_json::json!([
            {"value": "dark green", "things": 2},
            {"value": "amber", "things": 1},
        ]),
        "the values in use, most used first, each with how many things hold it: {body}"
    );
    assert_eq!(values["distinct"], 2, "how many values exist: {body}");

    // **The half that matters: a value nobody has used is written.** This
    // is a read of what is recorded, never a rule about what may be.
    let invented = "chartreuse";
    paint(&jojobot, &sid, "thing:bar-tape", invented).await;
    let after = asking(&jojobot, &sid, "colour").await;
    assert_eq!(
        after["values"]["distinct"], 3,
        "the new value is in use now: {after}"
    );
    assert!(
        after["values"]["in_use"]
            .as_array()
            .expect("the values in use")
            .iter()
            .any(|v| v["value"] == invented && v["things"] == 1),
        "a value that was in no list is recorded and comes back: {after}"
    );

    // **A key nobody has written is an answer.** Nothing is there to pick
    // from, and that is a fact about the store rather than a refusal.
    let unused = asking(&jojobot, &sid, "smell").await;
    assert_ne!(
        unused["status"], "blocked",
        "an unused key is no refusal: {unused}"
    );
    assert_eq!(
        unused["values"]["in_use"],
        serde_json::json!([]),
        "an unused key comes back with nothing in use: {unused}"
    );
    assert_eq!(unused["values"]["distinct"], 0, "and says so: {unused}");
}

/// A rhythm under something, holding a whole schedule.
async fn a_rhythm(jojobot: &Jojobot, handle: &str, cadence: &str, counts_from: &str) {
    ensure(jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", handle, handle)
        }))
        .await
        .expect("add ok");
    capture_ok(
        jojobot,
        CaptureArgs {
            fields: Some(
                [
                    ("cadence_days".to_string(), cadence.to_string()),
                    ("advances_from".to_string(), "due_date".to_string()),
                    ("counts_from".to_string(), counts_from.to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            ..capture_args(&format!("rhythm:{handle}"), "the loop, set up")
        },
    )
    .await;
}

/// Which handles an answer came back with, in order.
fn handles(body: &serde_json::Value) -> Vec<String> {
    body["objects"]
        .as_array()
        .expect("an answer carries objects")
        .iter()
        .map(|o| {
            o["id"]
                .as_str()
                .expect("an object has a handle")
                .to_string()
        })
        .collect()
}

/// 🚨 **A read answers in the day the run states, not the day the server
/// is having.**
///
/// Rule 222 does not distinguish a read from a write: a read that resolves
/// today off the clock is the server assuming a frame. A session working
/// through last March asking what has gone quiet is handed a WRONG ANSWER,
/// and nothing later contradicts it — where a wrongly dated write at least
/// leaves a record somebody can find.
///
/// The loop here fell due six days after the day the run states, so it has
/// gone quiet on the clock and has NOT gone quiet in March. **Both halves
/// against one store**: a run that stated no day still gets the clock, or
/// this becomes a change that breaks every caller that never stated one.
#[tokio::test]
async fn a_read_answers_in_the_day_the_run_states() {
    let jojobot = handler();
    make_bot(&jojobot, "otto").await;
    a_rhythm(&jojobot, "descale", "7", "2026-03-14").await;

    let acting = sid_of(&json_of(
        &jojobot
            .start_here(Parameters(OrientArgs {
                claim: None,
                bot: Some("otto".into()),
                today: Some("2026-03-15".into()),
                resume: Some("new".into()),
                brief: Some(true),
                timezone: None,
                skill: None,
                section: None,
                sid: None,
            }))
            .await
            .expect("the boot call is ok"),
    ))
    .expect("a boot that states a day hands back a handle");

    let in_march = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("rhythm".into()),
                overdue: Some(OverdueArgs { as_of: None }),
                sid: Some(acting.clone()),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        in_march["overdue_as_of"], "2026-03-15",
        "the read was taken as of the server's day: {in_march}"
    );
    assert!(
        handles(&in_march).is_empty(),
        "the loop falls due on the twenty-first and this run is on the fifteenth: {in_march}"
    );

    // ⚠️ **A run that stated no day still gets the clock**, and the loop
    // that had not gone quiet in March has gone quiet by now. Without this
    // half the case passes on a build that answers nothing to anybody.
    let now = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("rhythm".into()),
                overdue: Some(OverdueArgs { as_of: None }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        handles(&now),
        vec!["rhythm:descale".to_string()],
        "a run that stated no day is answered on the clock: {now}"
    );

    // **And a day the call names still wins over the run's.** A run working
    // through a period asks about other days, exactly as it writes about
    // them.
    let named = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("rhythm".into()),
                overdue: Some(OverdueArgs {
                    as_of: Some("2026-03-30".into()),
                }),
                sid: Some(acting),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(named["overdue_as_of"], "2026-03-30");
    assert_eq!(handles(&named), vec!["rhythm:descale".to_string()]);
}

/// 🚨 **The owed read orders what it found, oldest due first — "which has
/// gone quiet longest" answered by the order alone, not by a caller
/// re-sorting a bag of dates it was never given.**
///
/// `descale` is named ahead of `polish` alphabetically and falls due
/// LATER; `polish` falls due first. A read that merely filtered and left
/// the scan's own order standing would show `descale` first — this case
/// only passes if something actually orders by how overdue each one is.
///
/// **And it states the distance, not only the order.** An order a caller
/// cannot check is one they have to trust — polish fell due on 2026-01-08
/// and descale on 2026-03-08, 144 and 85 days respectively before
/// 2026-06-01, computed independently of `Due::staleness`'s own sort key
/// so this cannot pass on a build that orders correctly but reports the
/// wrong number.
#[tokio::test]
async fn the_owed_read_orders_the_longest_quiet_first() {
    let jojobot = handler();
    make_bot(&jojobot, "otto").await;
    a_rhythm(&jojobot, "polish", "7", "2026-01-01").await;
    a_rhythm(&jojobot, "descale", "7", "2026-03-01").await;

    let found = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("rhythm".into()),
                overdue: Some(OverdueArgs {
                    as_of: Some("2026-06-01".into()),
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        handles(&found),
        vec!["rhythm:polish".to_string(), "rhythm:descale".to_string()],
        "polish fell due in January and descale in March, so polish has gone quiet longer \
             and belongs first: {found}",
    );
    assert_eq!(
        found["objects"][0]["overdue_by_days"], 144,
        "polish fell due on 2026-01-08, 144 days before the day asked about: {found}",
    );
    assert_eq!(
        found["objects"][1]["overdue_by_days"], 85,
        "descale fell due on 2026-03-08, 85 days before the day asked about: {found}",
    );

    // The paired negative: a read that asks no overdue question states no
    // distance either, on the same objects.
    let unfiltered = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("rhythm".into()),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert!(
        unfiltered["objects"]
            .as_array()
            .expect("objects")
            .iter()
            .all(|o| o.get("overdue_by_days").is_none()),
        "no overdue question was asked, so no distance is answered: {unfiltered}",
    );
}

/// **A loop whose schedule cannot be read has no distance, and it still
/// sorts first.**
///
/// `Due::staleness` already puts `Unreadable` ahead of every dated one —
/// loud, because nothing here can say exactly how late it is, so it does
/// not hide behind a number that can be measured. That domain rule had no
/// case proving it through the served surface, only the unit test on
/// `Due::staleness` itself; this is the served one, and it is what tells a
/// caller whether the position is a domain guarantee or an accident of
/// this one store's scan order.
#[tokio::test]
async fn an_unreadable_schedule_sorts_first_and_carries_no_distance() {
    let jojobot = handler();
    a_rhythm(&jojobot, "descale", "7", "2026-08-01").await;
    ensure(&jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", "half-made", "Half Made")
        }))
        .await
        .expect("add ok");
    capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(
                [("cadence_days".to_string(), "7".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("rhythm:half-made", "every week, roughly")
        },
    )
    .await;

    let found = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("rhythm".into()),
                overdue: Some(OverdueArgs {
                    as_of: Some("2026-09-01".into()),
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        handles(&found),
        vec!["rhythm:half-made".to_string(), "rhythm:descale".to_string()],
        "the unreadable one sorts first, ahead of the dated one it would otherwise trail if \
             sorted by anything measurable: {found}",
    );
    assert!(
        found["objects"][0]["overdue_by_days"].is_null(),
        "nothing can say how late the unreadable one is, so its distance is null rather \
             than a guess: {found}",
    );
    assert_eq!(
        found["objects"][1]["overdue_by_days"], 24,
        "descale fell due on 2026-08-08, 24 days before the day asked about: {found}",
    );
}

/// **Which rhythms have gone quiet, as of a date the caller names.**
///
/// Two loops on different cadences and one date: the answer is the
/// difference between them. Asserted with the loop that is NOT overdue in
/// the same store, because an answer that returns everything is
/// indistinguishable from one that returns the right things.
///
/// The date is an argument, so the same store gives a different answer for
/// a later day — which is what makes *what is overdue as of next Friday* a
/// question this can be asked, and what stops the case rotting when the
/// calendar moves.
#[tokio::test]
async fn recall_says_which_rhythms_have_gone_quiet_as_of_a_date() {
    let jojobot = handler();
    a_rhythm(&jojobot, "descale", "7", "2026-08-01").await;
    a_rhythm(&jojobot, "deep-clean", "90", "2026-08-01").await;

    let quiet = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("rhythm".into()),
                overdue: Some(OverdueArgs {
                    as_of: Some("2026-08-10".into()),
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        handles(&quiet),
        vec!["rhythm:descale".to_string()],
        "the weekly loop fell due on the eighth and the quarterly one has not: {quiet}",
    );
    assert_eq!(
        quiet["overdue_as_of"], "2026-08-10",
        "the answer says which day it was asked about: {quiet}",
    );

    // The same store, a later day: the quarterly one has fallen due too.
    // Without this the case passes on a build that keeps whatever it likes.
    let later = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("rhythm".into()),
                overdue: Some(OverdueArgs {
                    as_of: Some("2026-11-01".into()),
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        handles(&later),
        // Oldest due first: descale fell due on the eighth of August,
        // deep-clean not until the end of October — descale has gone
        // quiet longer and sorts ahead of it.
        vec![
            "rhythm:descale".to_string(),
            "rhythm:deep-clean".to_string()
        ],
        "both loops have gone quiet by November: {later}",
    );

    // And the positive the filter rests on: without it the read is the
    // ordinary one and both come back whatever the date.
    let all = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("rhythm".into()),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        handles(&all).len(),
        2,
        "an unfiltered read keeps both: {all}"
    );
    assert_eq!(
        all["overdue_as_of"],
        serde_json::Value::Null,
        "and it names no date, because it asked about none: {all}",
    );
}

/// 🚨 **The overdue filter accounts for what it dropped.** Without this,
/// "not in this list" collapses "not due yet" into the same silence as
/// "the loop was never opened" or "nothing here carries a schedule" — the
/// same absence `near_unplaced` and `withheld` exist to stop a caller
/// mistaking for nothing having changed.
#[tokio::test]
async fn the_overdue_filter_says_how_many_it_dropped() {
    let jojobot = handler();
    a_rhythm(&jojobot, "descale", "7", "2026-08-01").await;
    a_rhythm(&jojobot, "deep-clean", "90", "2026-08-01").await;

    let quiet = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("rhythm".into()),
                overdue: Some(OverdueArgs {
                    as_of: Some("2026-08-10".into()),
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        handles(&quiet),
        vec!["rhythm:descale".to_string()],
        "the weekly loop is overdue and the quarterly one is not: {quiet}",
    );
    assert_eq!(
        quiet["overdue_excluded"], 1,
        "one rhythm was dropped by the filter and the count says how many: {quiet}",
    );

    // The positive the count rests on: a read that asked no overdue
    // question carries none, not a zero — there was no filter to report on.
    let all = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("rhythm".into()),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        all["overdue_excluded"],
        serde_json::Value::Null,
        "an unfiltered read names no count, because it dropped nothing and asked nothing: {all}",
    );
}

/// 🚨 **A browse that names no handle honours archived state.** The
/// direct door — naming the handle — stays open regardless, proved by
/// `an_archived_entitys_own_handle_still_returns_it_whole_with_its_reason`
/// in `list_entities.rs`: that case is untouched by this one and must
/// keep passing.
#[tokio::test]
async fn a_browse_with_no_handle_named_excludes_what_is_archived() {
    let jojobot = handler();
    ensure(&jojobot, "person:bart").await;
    ensure(&jojobot, "person:milhouse").await;
    jojobot
        .memory
        .archive_entity(&EntityId("person:bart".into()), "a mistaken write")
        .await
        .expect("archive_entity ok");

    let body = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("person".into()),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    let ids: Vec<&str> = body["objects"]
        .as_array()
        .expect("an answer carries objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an object has a handle"))
        .collect();
    assert!(
        !ids.contains(&"person:bart"),
        "an archived entity crossed a kind browse that named no handle: {body}",
    );
    assert!(
        ids.contains(&"person:milhouse"),
        "a live entity is missing from the same browse: {body}",
    );
}

/// 🚨 **The count `list_entities` already carries, on `recall`'s own
/// browse.** The exclusion above and this count are two different
/// fixes, and only one of them used to reach `recall` — a caller could
/// tell an archived entity was gone from the objects list but never how
/// many, so "only one exists" and "one was hidden" read alike.
#[tokio::test]
async fn a_browse_with_no_handle_named_reports_how_many_archival_excluded() {
    let jojobot = handler();
    ensure(&jojobot, "person:bart").await;
    ensure(&jojobot, "person:milhouse").await;
    jojobot
        .memory
        .archive_entity(&EntityId("person:bart".into()), "a mistaken write")
        .await
        .expect("archive_entity ok");

    let body = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("person".into()),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        body["archived_excluded"], 1,
        "one archived person did not count against a kind browse: {body}",
    );
}

/// **The direct door excludes nothing, so it counts nothing.** Naming
/// the archived entity's own handle returns it whole — untouched by
/// this change — and the count reads zero rather than being left off,
/// the same way `overdue_excluded` is null only when no overdue
/// question was asked at all, never when the answer is zero.
#[tokio::test]
async fn a_direct_handle_on_an_archived_entity_reports_nothing_excluded() {
    let jojobot = handler();
    ensure(&jojobot, "person:bart").await;
    jojobot
        .memory
        .archive_entity(&EntityId("person:bart".into()), "a mistaken write")
        .await
        .expect("archive_entity ok");

    let body = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("person:bart".into()),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        body["archived_excluded"], 0,
        "a direct-handle read excluded nothing, so it should count nothing: {body}",
    );
}

/// **A rhythm that cannot say when it is due is overdue**, which is the
/// loud answer rather than the tidy one: the alternative is a half-built
/// loop that surfaces at no boot ever and is never heard from again.
///
/// It comes back carrying its fields, so the caller can see which key it is
/// short of.
/// **The same stored loop has fallen due in one zone and not yet in the
/// other, and both answers are right.**
///
/// This is the sharp end of the frame belonging to the caller. One row, one
/// question, two runs — and a loop that falls due today has genuinely
/// arrived for the run whose day it already is and genuinely has not for
/// the run still on yesterday. **It is not a fault and there is nothing to
/// work around**, which is why the door says so in its own text.
///
/// The two zones are twenty-six hours apart, the widest the map goes, so
/// their local dates differ at every instant and this case does not pass or
/// fail by the hour it is run at. The due date is worked out from the
/// leading zone's own today, so nothing here rots when the calendar moves.
///
/// **Both directions are asserted in the one case.** A build ignoring zones
/// gives the two runs one answer, whichever answer that is, so pinning only
/// the arrival or only the absence would pass on it.
#[tokio::test]
async fn one_loop_falls_due_in_one_zone_and_not_yet_in_the_other() {
    let jojobot = handler();
    make_bot(&jojobot, "otto").await;

    let day_in = |zone: &str| {
        jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::get(zone).expect("a zone"))
            .date()
    };
    // A cadence of one day counting from the leading zone's yesterday: the
    // loop falls due on that zone's TODAY, which every other zone on the
    // map is either on or behind.
    let counts_from = day_in("Pacific/Kiritimati")
        .yesterday()
        .expect("a day before");
    a_rhythm(&jojobot, "descale", "1", &counts_from.to_string()).await;

    let overdue_for = async |sid: String| {
        let body = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    kind: Some("rhythm".into()),
                    sid: Some(sid),
                    // No `as_of`: the whole point is which day the RUN
                    // thinks it is.
                    overdue: Some(OverdueArgs { as_of: None }),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        (handles(&body), body)
    };

    let ahead = booted_in(&jojobot, "otto", "Pacific/Kiritimati", Some("new")).await;
    let behind = booted_in(&jojobot, "otto", "Etc/GMT+12", Some("new")).await;
    let (arrived, ahead_body) = overdue_for(ahead).await;
    let (not_yet, behind_body) = overdue_for(behind).await;

    assert_eq!(
        arrived,
        vec!["rhythm:descale".to_string()],
        "the run whose day it already is finds the loop due: {ahead_body}",
    );
    assert!(
        not_yet.is_empty(),
        "…and the run still on an earlier day does not, from the same row: {behind_body}",
    );
    assert_eq!(
        ahead_body["overdue_as_of"],
        day_in("Pacific/Kiritimati").to_string(),
        "each answer says which day it was asked about, in its own frame",
    );
    assert_eq!(
        behind_body["overdue_as_of"],
        day_in("Etc/GMT+12").to_string(),
    );
}

#[tokio::test]
async fn a_rhythm_with_half_a_schedule_is_overdue_rather_than_invisible() {
    let jojobot = handler();
    a_rhythm(&jojobot, "descale", "7", "2026-08-01").await;
    ensure(&jojobot, "thing:kettle").await;
    jojobot
        .add_entity(Parameters(AddEntityArgs {
            parent: Some("thing:kettle".into()),
            ..add_args("rhythm", "half-made", "Half Made")
        }))
        .await
        .expect("add ok");
    capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(
                [("cadence_days".to_string(), "7".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("rhythm:half-made", "every week, roughly")
        },
    )
    .await;

    // A date before the whole loop is due: the only reason the half-made
    // one is here is that nothing can say when it falls due.
    let quiet = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("rhythm".into()),
                overdue: Some(OverdueArgs {
                    as_of: Some("2026-08-02".into()),
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        handles(&quiet),
        vec!["rhythm:half-made".to_string()],
        "the loop nobody finished is the one that surfaces: {quiet}",
    );
    assert_eq!(
        quiet["objects"][0]["fields"]["cadence_days"], "7",
        "and it arrives with what it does hold, so the gap is readable: {quiet}",
    );
}

/// **The writes behind a key come back on the read that already exists**,
/// and only when the call asks for them.
///
/// Both halves, because either alone is satisfied by the wrong build: a
/// history that is always there costs every caller who never asked, and one
/// that is never there is a capability nothing can reach. The count is
/// asserted beside the values because counting is what the whole substrate
/// is for, and the addresses because a history of one record's edits and a
/// history of a key written by many records are the two answers this could
/// have been.
#[tokio::test]
async fn recall_answers_with_the_writes_behind_a_key_when_asked() {
    let jojobot = handler();
    for nth in 1..=3 {
        capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [("donuts_eaten".to_string(), nth.to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("alpha", &format!("ate one, number {nth}"))
            },
        )
        .await;
    }

    let asked = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                history: Some("donuts_eaten".into()),
                ..of("alpha")
            }))
            .await
            .expect("recall ok"),
    );
    let history = &asked["objects"][0]["history"];
    assert_eq!(history["key"], "donuts_eaten");
    assert_eq!(
        history["count"], 3,
        "the count of the writes is the answer to how many times: {history}"
    );
    assert_eq!(
        history["writes"]
            .as_array()
            .expect("the writes come back as a list")
            .iter()
            .map(|w| w["value"].as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>(),
        vec!["1", "2", "3"],
        "oldest first: {history}"
    );
    assert_eq!(
        history["writes"][0]["record"], "person:alpha#f1",
        "each write names the record it arrived in, and they differ: {history}"
    );
    assert_eq!(history["writes"][2]["record"], "person:alpha#f3");

    let plain = json_of(
        &jojobot
            .recall(Parameters(of("alpha")))
            .await
            .expect("recall ok"),
    );
    assert!(
        plain["objects"][0].get("history").is_none(),
        "a call that asked for no key is not charged for one: {plain}"
    );
}

/// **A thing comes back as one dense row, and the records it was folded
/// from say they are not here.**
///
/// The whole point of the read: a caller asking what a thing HOLDS gets one
/// value per key rather than every claim ever made about it. Both halves,
/// because a row that is always there and records that are always there is
/// a build where the caller was charged for both.
#[tokio::test]
async fn recall_answers_with_the_folded_row_and_names_what_it_left_out() {
    let jojobot = handler();
    for (key, value, content) in [
        ("weight", "11", "weighed at the bench"),
        ("wheel", "700c", "measured the rim"),
        ("weight", "12", "weighed again after the rebuild"),
    ] {
        capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some([(key.to_string(), value.to_string())].into_iter().collect()),
                ..capture_args("thing:gravel-bike", content)
            },
        )
        .await;
    }

    let dense = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                facts: Some(false),
                ..of("thing:gravel-bike")
            }))
            .await
            .expect("recall ok"),
    );
    let object = &dense["objects"][0];
    assert_eq!(
        object["fields"],
        serde_json::json!({"weight": "12", "wheel": "700c"}),
        "one row, the newest write winning a repeated key: {object}"
    );
    assert!(
        object.get("facts").is_none(),
        "the records were not asked for: {object}"
    );
    let note = object["records"]
        .as_str()
        .expect("an answer that left the records out says so");
    assert!(
        note.contains('3') && note.contains("facts"),
        "…how many there are, and the call that returns them: {note}"
    );

    // The other half: asked for, they come back — with the addresses that
    // make them editable, which is what a caller loses if the dense read
    // is the only one.
    let whole = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                facts: Some(true),
                ..of("thing:gravel-bike")
            }))
            .await
            .expect("recall ok"),
    );
    let object = &whole["objects"][0];
    assert_eq!(
        object["facts"].as_array().map(Vec::len),
        Some(3),
        "every record is reachable: {object}"
    );
    assert_eq!(object["facts"][0]["address"], "thing:gravel-bike#f1");
    assert!(
        object.get("records").is_none(),
        "…and nothing was left out, so nothing says it was: {object}"
    );
    assert_eq!(
        object["fields"], dense["objects"][0]["fields"],
        "the row does not change with the records"
    );
}

/// 🚨 **A shape's sources are left off the facts listing, and the answer
/// says how many and how to reach them.** Four halves, because each alone
/// passes on a build that answers nothing useful: the shape is served,
/// its source is not, the note names the count and the argument, and
/// `stood_for: true` serves both.
#[tokio::test]
async fn recall_folds_a_shapes_sources_out_of_the_facts_listing() {
    let jojobot = handler();
    let source = capture_ok(&jojobot, capture_args("alpha", "said the kiln was lit")).await;
    let source_address = address_of(&source);
    let shape = capture_ok(&jojobot, capture_args("alpha", "resolved now")).await;
    let shape_address = address_of(&shape);

    jojobot
        .update_fact(Parameters(UpdateFactArgs {
            stands_for: Some(vec![source_address.clone()]),
            ..update_args(&shape_address)
        }))
        .await
        .expect("update ok");

    let folded = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                facts: Some(true),
                ..of("person:alpha")
            }))
            .await
            .expect("recall ok"),
    );
    let facts = folded["objects"][0]["facts"].as_array().expect("facts");
    assert!(
        facts.iter().any(|f| f["address"] == shape_address),
        "the shape is served: {facts:?}"
    );
    assert!(
        facts.iter().all(|f| f["address"] != source_address),
        "its source is not, on the same listing: {facts:?}"
    );
    let note = folded["objects"][0]["stood_for"]
        .as_str()
        .expect("an answer that folded a source out says so");
    assert!(
        note.contains('1') && note.contains("stood_for"),
        "…how many, and the argument that returns them: {note}"
    );

    // The other argument: asking for the sources back serves both.
    let whole = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                facts: Some(true),
                stood_for: Some(true),
                ..of("person:alpha")
            }))
            .await
            .expect("recall ok"),
    );
    let facts = whole["objects"][0]["facts"].as_array().expect("facts");
    assert!(
        facts.iter().any(|f| f["address"] == source_address),
        "stood_for: true serves the source beside the shape: {facts:?}"
    );
    assert!(
        whole["objects"][0].get("stood_for").is_none(),
        "…and nothing was left out, so nothing says it was: {}",
        whole["objects"][0]
    );
}

/// **The records are off unless the call asks**, and the fields are not.
///
/// The default itself, which every other case here states explicitly — so
/// a build that quietly shipped every claim would pass all of them and fail
/// this one. It reads a plain call, the one an agent makes when it knows
/// nothing about arguments.
#[tokio::test]
async fn a_plain_recall_ships_the_fields_and_not_the_records() {
    let jojobot = handler();
    capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(
                [("weight".to_string(), "11".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("thing:gravel-bike", "weighed at the bench")
        },
    )
    .await;

    let plain = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("thing:gravel-bike".into()),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    let object = &plain["objects"][0];
    assert_eq!(
        object["fields"],
        serde_json::json!({"weight": "11"}),
        "what the thing is comes back unasked: {object}"
    );
    assert!(
        object.get("facts").is_none(),
        "…and the claims behind it do not: {object}"
    );
    assert!(
        object["records"].as_str().is_some_and(|n| n.contains('1')),
        "…and the answer says they are there and how to read them: {object}"
    );
}

/// **A history longer than the window comes back cut, and the answer says
/// how many exist and how to reach the rest.**
///
/// A read whose job is the small answer must not be able to flood the
/// caller it serves. The short case is asserted beside it because a cap
/// that fires always and a cap that fires never look the same from one
/// call.
#[tokio::test]
async fn a_long_history_is_capped_on_the_wire_and_says_what_it_left_out() {
    let jojobot = handler();
    let written = jojobot_domain::memory::graph::WRITES_SHOWN + 5;
    for nth in 1..=written {
        capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [("donuts_eaten".to_string(), nth.to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("alpha", &format!("ate one, number {nth}"))
            },
        )
        .await;
    }

    let capped = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                history: Some("donuts_eaten".into()),
                facts: Some(false),
                ..of("alpha")
            }))
            .await
            .expect("recall ok"),
    );
    let history = &capped["objects"][0]["history"];
    assert_eq!(
        history["count"], written,
        "the answer says how many writes exist: {history}"
    );
    assert_eq!(
        history["shown"],
        jojobot_domain::memory::graph::WRITES_SHOWN,
        "…and how many it handed over"
    );
    assert_eq!(
        history["writes"].as_array().map(Vec::len),
        Some(jojobot_domain::memory::graph::WRITES_SHOWN),
        "…which is what it actually handed over: {history}"
    );
    assert_eq!(
        history["writes"][0]["value"], "6",
        "the window is the newest writes, still oldest first: {history}"
    );
    let older = history["older"]
        .as_str()
        .expect("a cut answer says it was cut");
    assert!(
        older.contains('5') && older.contains("history_most"),
        "…how many are missing, and the way to them: {older}"
    );

    // Raising the window is that way, and it works — the whole history,
    // and no note, because nothing was left out.
    let whole = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                history: Some("donuts_eaten".into()),
                history_most: Some(written as u32),
                facts: Some(false),
                ..of("alpha")
            }))
            .await
            .expect("recall ok"),
    );
    let history = &whole["objects"][0]["history"];
    assert_eq!(history["writes"].as_array().map(Vec::len), Some(written));
    assert_eq!(history["writes"][0]["value"], "1");
    assert!(
        history.get("older").is_none(),
        "a history that came back whole carries no elision noise: {history}"
    );
}

/// 🚨 **A corrected claim's own writes come back on the same read**, named
/// by the record's address, and only when the call asks for them.
///
/// The trace exists in the store and this is the way a caller reaches it.
/// Three halves, because each alone passes on a build that is useless:
/// a read that always carries a chain costs every caller who never asked;
/// a read that never carries one is a capability nothing can reach; and a
/// chain on a claim nobody corrected must be the ONE write that claim has,
/// or the answer says every record in the store was rewritten.
///
/// **No write carries a moment.** Every write of a claim keeps the moment
/// the claim first entered the store, so a moment per write would report
/// corrections made months apart as one instant.
#[tokio::test]
async fn recall_answers_with_the_writes_behind_a_record_when_asked() {
    let jojobot = handler();
    capture_ok(&jojobot, capture_args("alpha", "works at the old place")).await;
    capture_ok(&jojobot, capture_args("alpha", "rides to work")).await;
    jojobot
        .update_fact(Parameters(UpdateFactArgs {
            provenance: Some("inference".to_string()),
            content: Some("works at the new place".into()),
            ..update_args("person:alpha#f1")
        }))
        .await
        .expect("update ok");

    let asked = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                history_record: Some("person:alpha#f1".into()),
                facts: Some(false),
                ..of("alpha")
            }))
            .await
            .expect("recall ok"),
    );
    let chain = &asked["objects"][0]["record_history"];
    assert_eq!(chain["record"], "person:alpha#f1");
    assert_eq!(
        chain["count"], 2,
        "the claim was written twice and the answer says so: {chain}"
    );
    assert_eq!(
        chain["writes"]
            .as_array()
            .expect("the writes come back as a list")
            .iter()
            .map(|w| w["content"].as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>(),
        vec!["works at the old place", "works at the new place"],
        "oldest first, so what the claim used to say is readable: {chain}"
    );
    assert_eq!(
        chain["writes"][0]["nth"], 1,
        "each write carries its place in the order: {chain}"
    );
    assert_eq!(chain["writes"][1]["nth"], 2);
    // **Each write says when it happened**, and the two differ — a claim
    // corrected months later did not have both writes made at once.
    let moment = |nth: usize| {
        chain["writes"][nth]["written_at"]
            .as_str()
            .unwrap_or_else(|| panic!("a write says when it happened: {chain}"))
            .to_string()
    };
    assert!(
        moment(0) < moment(1),
        "both writes report one moment, so the chain reads as corrections made at once: \
             {chain}"
    );
    // ⚠️ **And it is not the claim's own first-recorded moment**, which
    // answers when jojobot took the record in and is on the claim.
    assert!(
        chain["writes"][0].get("inserted_at").is_none(),
        "the claim's own moment is repeated onto its writes: {chain}"
    );

    // **A claim nobody corrected has one write.** Asked of the second
    // record, on the same object, through the same call.
    let untouched = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                history_record: Some("person:alpha#f2".into()),
                facts: Some(false),
                ..of("alpha")
            }))
            .await
            .expect("recall ok"),
    );
    let alone = &untouched["objects"][0]["record_history"];
    assert_eq!(
        alone["count"], 1,
        "a claim nobody corrected reads as one somebody rewrote: {alone}"
    );
    assert_eq!(alone["writes"][0]["content"], "rides to work");

    let plain = json_of(
        &jojobot
            .recall(Parameters(of("alpha")))
            .await
            .expect("recall ok"),
    );
    assert!(
        plain["objects"][0].get("record_history").is_none(),
        "a call that asked for no record is not charged for one: {plain}"
    );
}

/// 🚨 **An ordinary read of a claim says it has been written more than
/// once — without a caller having to already hold its address and ask for
/// `history_record`.**
///
/// A claim rewritten twice comes back field-for-field identical to one
/// written once unless something says otherwise; this is the signal that
/// says otherwise. Paired with a claim nobody corrected: the untouched
/// fact's silence is what a build that marks everything "revised" fails.
#[tokio::test]
async fn an_ordinary_read_says_a_claim_was_written_more_than_once() {
    let jojobot = handler();
    capture_ok(&jojobot, capture_args("alpha", "lent the drill to Ralph")).await;
    capture_ok(&jojobot, capture_args("alpha", "rides to work")).await;
    jojobot
        .update_fact(Parameters(UpdateFactArgs {
            provenance: Some("inference".to_string()),
            content: Some("Ralph gave the drill back".into()),
            ..update_args("person:alpha#f1")
        }))
        .await
        .expect("update ok");

    let read = json_of(
        &jojobot
            .recall(Parameters(of("alpha")))
            .await
            .expect("recall ok"),
    );
    let facts = read["objects"][0]["facts"].as_array().expect("a list");
    let corrected = facts
        .iter()
        .find(|f| f["address"] == "person:alpha#f1")
        .expect("the corrected claim is in the answer");
    assert_eq!(corrected["revised"], true, "{corrected}");
    assert_eq!(corrected["revision_count"], 2, "{corrected}");
    let note = corrected["revision_note"]
        .as_str()
        .expect("a note says how to reach the earlier wording");
    assert!(
        note.contains("history_record") && note.contains("person:alpha#f1"),
        "the note names the door and the address it opens: {note}"
    );

    // **The negative that gives it meaning.** A claim nobody rewrote is
    // silent on all three keys, not `false`/`1` — the same convention
    // `stale` already uses.
    let untouched = facts
        .iter()
        .find(|f| f["address"] == "person:alpha#f2")
        .expect("the untouched claim is in the answer");
    assert!(
        untouched.get("revised").is_none()
            && untouched.get("revision_count").is_none()
            && untouched.get("revision_note").is_none(),
        "a claim written once carries none of these keys: {untouched}"
    );
}

/// 🚨 **`built_on` alone is a complete selection, the same shape
/// `history_record` already has.** An address names the entity its
/// source claim is filed under, so a call naming only `built_on` is not
/// naming nothing — it was refused before this, on the domain's own
/// narrows-nothing check, because `built_on` filled no subject in.
#[tokio::test]
async fn built_on_alone_is_selection_enough_and_returns_the_derived_claims() {
    let jojobot = handler();
    let source = address_of(&capture_ok(&jojobot, capture_args("alpha", "the source claim")).await);
    capture_ok(
        &jojobot,
        CaptureArgs {
            derived_from: Some(source.clone()),
            ..capture_args("alpha", "built on the source claim")
        },
    )
    .await;

    let alone = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                built_on: Some(source.clone()),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        alone["objects"][0]["id"], "person:alpha",
        "the address names its own subject and the read did not use it: {alone}"
    );
    assert_eq!(alone["built_on"]["count"], 1, "{alone}");
    assert!(
        alone["built_on"]["claims"][0]["content"] == "built on the source claim",
        "the derived claim itself is not in the answer: {alone}"
    );
}

/// **A subject that agrees with `built_on`'s own address changes
/// nothing** — the positive this mechanism rests on: filling the subject
/// in is only ever a convenience for the caller who left it out, never a
/// different answer than naming it explicitly would give.
#[tokio::test]
async fn built_on_with_a_matching_subject_gives_the_same_answer() {
    let jojobot = handler();
    let source = address_of(&capture_ok(&jojobot, capture_args("alpha", "the source claim")).await);
    capture_ok(
        &jojobot,
        CaptureArgs {
            derived_from: Some(source.clone()),
            ..capture_args("alpha", "built on the source claim")
        },
    )
    .await;

    let named = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("alpha".into()),
                built_on: Some(source.clone()),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(named["built_on"]["count"], 1, "{named}");
    assert!(
        named["built_on"]["claims"][0]["content"] == "built on the source claim",
        "{named}"
    );
}

/// **`built_on`'s address is refused against a contradicting subject the
/// same way `history_record`'s is** — one shared path, not two guards
/// that could come to disagree about what counts as a contradiction.
#[tokio::test]
async fn built_on_with_a_contradicting_subject_is_refused() {
    let jojobot = handler();
    let source = address_of(&capture_ok(&jojobot, capture_args("alpha", "the source claim")).await);
    ensure(&jojobot, "person:beta").await;

    let refused = blocked(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("beta".into()),
                built_on: Some(source.clone()),
                ..of_nothing()
            }))
            .await
            .expect("a malformed query is an answer, not a protocol failure"),
    );
    let way = refused["how_to_proceed"]
        .as_str()
        .unwrap_or_else(|| panic!("a refusal says the way out: {refused}"));
    assert!(
        way.contains("person:alpha") && way.contains("person:beta"),
        "the refusal names both things the call pointed at: {way}"
    );
}

/// 🚨 **Two addresses on two different things, and no subject at all —
/// the refusal must not claim the caller asked for either entity.**
///
/// `history_record` fills the subject from its own address first, and
/// `built_on`'s address then disagrees with that FILLED-IN subject —
/// not with anything the caller actually sent. Wording the refusal as
/// "…and asks for {subject}" is true when a caller-supplied subject
/// disagrees and false here: nobody asked for the first address's
/// entity, the second loop iteration invented that claim.
#[tokio::test]
async fn two_addresses_on_different_things_are_named_without_inventing_a_subject() {
    let jojobot = handler();
    let alpha = address_of(&capture_ok(&jojobot, capture_args("alpha", "alpha's claim")).await);
    let beta = address_of(&capture_ok(&jojobot, capture_args("beta", "beta's claim")).await);

    let refused = blocked(
        &jojobot
            .recall(Parameters(RecallArgs {
                history_record: Some(alpha.clone()),
                built_on: Some(beta.clone()),
                ..of_nothing()
            }))
            .await
            .expect("a malformed query is an answer, not a protocol failure"),
    );
    let way = refused["how_to_proceed"]
        .as_str()
        .unwrap_or_else(|| panic!("a refusal says the way out: {refused}"));
    assert!(
        way.contains(&alpha) && way.contains(&beta),
        "the refusal names both addresses the call pointed at: {way}"
    );
    assert!(
        !way.contains("asks for"),
        "nobody asked for a subject — the caller sent none, and the wording must not \
             invent one: {way}"
    );
}

/// 🚨 **An address is a selection: tracing a record needs no subject beside
/// it.**
///
/// The address contains its subject, and the call demanded the subject
/// anyway — so the first two callers of the trace were both refused on
/// their first use, each having read the argument's own description
/// (rule 236).
///
/// ⚠️ **Paired, and the negative is what gives it meaning:** a subject that
/// names something else is refused rather than quietly answered. The
/// positive alone passes against a build that ignores the subject
/// entirely, and a caller whose two arguments disagree has made a mistake
/// worth hearing about.
#[tokio::test]
async fn a_traced_record_is_selection_enough_and_a_contradicting_subject_is_refused() {
    let jojobot = handler();
    capture_ok(&jojobot, capture_args("alpha", "works at the old place")).await;
    jojobot
        .update_fact(Parameters(UpdateFactArgs {
            provenance: Some("inference".to_string()),
            content: Some("works at the new place".into()),
            ..update_args("person:alpha#f1")
        }))
        .await
        .expect("update ok");
    ensure(&jojobot, "person:beta").await;

    // **The address alone.** No subject, no kind, no filter.
    let alone = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                history_record: Some("person:alpha#f1".into()),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        alone["objects"][0]["id"], "person:alpha",
        "the address names its own subject and the read did not use it: {alone}"
    );
    assert_eq!(
        alone["objects"][0]["record_history"]["count"], 2,
        "the chain the caller asked for is not here: {alone}"
    );

    // ⚠️ **A subject that contradicts the address.**
    let refused = blocked(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("beta".into()),
                history_record: Some("person:alpha#f1".into()),
                ..of_nothing()
            }))
            .await
            .expect("a malformed query is an answer, not a protocol failure"),
    );
    let way = refused["how_to_proceed"]
        .as_str()
        .unwrap_or_else(|| panic!("a refusal says the way out: {refused}"));
    assert!(
        way.contains("person:alpha") && way.contains("person:beta"),
        "the refusal names both things the call pointed at: {way}"
    );
}

/// **A record on a thing this call did not select comes back said, not
/// silently missing.**
///
/// The chain hangs on the object the record is filed under, so a call that
/// selected somebody else gets no chain anywhere — which a reader takes as
/// *this claim has no trace*. Both halves: the answer says nothing when the
/// record WAS reached, so the marker is not noise on every ordinary call.
#[tokio::test]
async fn a_traced_record_the_call_never_selected_is_said_rather_than_missing() {
    let jojobot = handler();
    capture_ok(&jojobot, capture_args("alpha", "works at the old place")).await;
    ensure(&jojobot, "org:guild").await;

    // **A selection that does not contradict the address and does not
    // reach it either.** A subject naming something else is a refusal —
    // the two arguments point at different things — where a KIND is a
    // legitimate question whose answer simply does not include the record's
    // home.
    let elsewhere = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("org".into()),
                history_record: Some("person:alpha#f1".into()),
                facts: Some(false),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    let said = elsewhere["record_history_unreached"]
        .as_str()
        .unwrap_or_else(|| panic!("the answer says the record was not reached: {elsewhere}"));
    assert!(
        said.contains("person:alpha#f1"),
        "the answer names the record and the handle to ask under: {said}"
    );
    // **A sentence whose whole purpose is to be read**: one line, and no
    // run of spaces where a wrapped literal lost its continuation.
    assert!(
        !said.contains('\n') && !said.contains("  "),
        "the sentence does not read as one: {said:?}"
    );

    let reached = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                history_record: Some("person:alpha#f1".into()),
                facts: Some(false),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert!(
        reached["record_history_unreached"].is_null(),
        "a chain that came back carries no marker saying it did not: {reached}"
    );
}

/// **The two halves of the history axis are one question, so naming both is
/// refused** — with nothing written and the way out named.
///
/// Answering whichever the code checked first would hand back a chain the
/// caller cannot tell from the one they did not get.
#[tokio::test]
async fn naming_a_key_and_a_record_to_trace_is_refused() {
    let jojobot = handler();
    capture_ok(&jojobot, capture_args("alpha", "works at the old place")).await;

    let refused = blocked(
        &jojobot
            .recall(Parameters(RecallArgs {
                history: Some("donuts_eaten".into()),
                history_record: Some("person:alpha#f1".into()),
                ..of("alpha")
            }))
            .await
            .expect("a malformed query is an answer, not a protocol failure"),
    );
    assert!(
        refused["how_to_proceed"]
            .as_str()
            .is_some_and(|way| way.contains("history_record")),
        "the refusal names the argument to drop: {refused}"
    );
}

/// Every recalled fact carries its address, and that address is what
/// `update_fact` takes — the pairing that makes editing possible.
#[tokio::test]
async fn recall_returns_addresses_that_update_fact_accepts() {
    let jojobot = handler();
    capture_ok(&jojobot, capture_args("alpha", "works at the old place")).await;

    let body = json_of(
        &jojobot
            .recall(Parameters(of("alpha")))
            .await
            .expect("recall ok"),
    );
    let address = body["objects"][0]["facts"][0]["address"]
        .as_str()
        .expect("every fact carries an address");
    assert_eq!(address, "person:alpha#f1");

    let updated = json_of(
        &jojobot
            .update_fact(Parameters(UpdateFactArgs {
                provenance: Some("inference".to_string()),
                content: Some("works at the new place".into()),
                details: Some("changed jobs in July".into()),
                ..update_args(address)
            }))
            .await
            .expect("update ok"),
    );
    // **The address recall handed over is the address the edit landed on**,
    // which is the whole claim of this case. The claim itself is read back
    // through the verb that reads, since the edit answers with a receipt.
    assert_eq!(updated["address"], "person:alpha#f1");
    assert_eq!(updated["content_elided"], true, "{updated}");
    let read = json_of(
        &jojobot
            .recall(Parameters(recall_args("person:alpha")))
            .await
            .expect("recall ok"),
    );
    let claim = &read["objects"][0]["facts"][0];
    assert_eq!(claim["content"], "works at the new place", "{read}");
    assert_eq!(claim["details"], "changed jobs in July", "{read}");
}

/// **An unknown handle is a miss at the wire too.** A read of a
/// nonexistent person answered with "reads fine, no facts" is the same
/// answer an empty page gives, so a caller can never repair a bad handle.
/// The miss comes back naming the handle and its near candidates, while an
/// empty-but-real entity reads fine.
#[tokio::test]
async fn recall_of_an_unknown_entity_is_a_miss_with_candidates() {
    let jojobot = handler();
    jojobot
        .add_entity(Parameters(add_args("person", "zenith", "Zenith")))
        .await
        .expect("add ok");

    let missed = blocked(
        &jojobot
            .recall(Parameters(of("person:zenit")))
            .await
            .expect("a handle that names nothing is an answer, not a protocol failure"),
    );
    assert_eq!(missed["attempted"], "person:zenit");
    assert_eq!(
        missed["candidates"][0]["handle"], "person:zenith",
        "the near candidate surfaces: {missed}"
    );

    let body = json_of(
        &jojobot
            .recall(Parameters(of("person:zenith")))
            .await
            .expect("an existing entity's empty page still reads"),
    );
    assert_eq!(
        body["objects"][0]["facts"]
            .as_array()
            .expect("a list")
            .len(),
        0
    );
}

/// **`recall` shows the edges too.** Search grew a neighborhood; a recall
/// that answered with the same rows stripped of their edges would make the
/// graph a thing you can only see by searching for it, and reading an
/// entity's own page is the commonest way anyone looks.
#[tokio::test]
async fn recall_returns_the_edge_a_fact_draws() {
    let jojobot = handler();
    jojobot
        .add_entity(Parameters(add_args("org", "guild", "The Guild")))
        .await
        .expect("add_entity ok");
    capture_ok(
        &jojobot,
        CaptureArgs {
            shape: Some("membership".into()),
            object: Some("org:guild".into()),
            ..capture_args("alpha", "joined in the spring")
        },
    )
    .await;

    let body = json_of(
        &jojobot
            .recall(Parameters(of("alpha")))
            .await
            .expect("recall ok"),
    );
    let edged = body["objects"][0]["facts"]
        .as_array()
        .expect("recall returns a list")
        .iter()
        .find(|f| f["content"] == "joined in the spring")
        .unwrap_or_else(|| panic!("the captured fact must come back: {body}"));
    assert_eq!(edged["edge"]["type"], "memberOf", "got {edged}");
    assert_eq!(edged["edge"]["object"], "org:guild");
}

/// **Objects of a kind, with their pages** — through the wire, and with
/// nothing in the call naming what the page is for.
///
/// The negative it rests on is the same query without `prose`: the key is
/// then absent rather than empty, so a caller can tell a page nobody asked
/// for from a page with nothing on it.
#[tokio::test]
async fn a_kind_comes_back_with_each_objects_prose() {
    let jojobot = handler();
    jojobot
        .add_entity(Parameters(add_args("bot", "gamma", "Gamma")))
        .await
        .expect("add_entity ok");
    let charter = "Keeps the roster.\n\nHard line: never writes to the ledger.";
    jojobot
        .set_charter(Parameters(SetCharterArgs {
            bot: "gamma".into(),
            prose: charter.into(),
            sid: Some(crate::harness::TEST_SID.into()),
        }))
        .await
        .expect("set_charter ok");

    let asked = RecallArgs {
        kind: Some("bot".into()),
        prose: Some(true),
        facts: Some(false),
        ..of_nothing()
    };
    let body = json_of(&jojobot.recall(Parameters(asked)).await.expect("recall ok"));
    let gamma = body["objects"]
        .as_array()
        .expect("a list of objects")
        .iter()
        .find(|o| o["id"] == "bot:gamma")
        .unwrap_or_else(|| panic!("the bot must be in a query for its kind: {body}"));
    assert_eq!(
        gamma["prose"], charter,
        "the page comes back whole: {gamma}"
    );
    assert!(
        gamma.get("facts").is_none(),
        "facts were declined, so the key is absent: {gamma}"
    );

    let unasked = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("bot".into()),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert!(
        unasked["objects"][0].get("prose").is_none(),
        "prose nobody asked for is absent rather than empty: {unasked}"
    );
}

/// 🚨 **The routed path fires, and it fires only for the one shape that
/// needed it.**
///
/// Both answers are correct whichever path served them — that is the
/// whole risk this pair exists to close: a case asserting only the
/// answer would pass identically whether the routing predicate has ever
/// fired once. `SpySearch::reached` is the observable that tells the two
/// apart, over the one port a routed query has to touch and a
/// kind-scoped one never does.
#[tokio::test]
async fn a_type_only_overdue_read_routes_through_search_and_a_kind_scoped_one_never_does() {
    // **The positive: no kind, no subject, a declared type and overdue —
    // exactly the shape `graph::walk` cannot answer without a full read.**
    let memory = shared_memory();
    let setup = handler_on(memory.clone(), Arc::new(SpySearch::default()));
    // The real build ships "runs-out" at every boot (seed.rs); a bare
    // test store starts with no declared types at all, so this declares
    // the same shape by hand.
    setup
        .declare_type(Parameters(DeclareTypeArgs {
            name: "runs-out".into(),
            fields: vec![FieldArgs {
                key: attention::RUNS_OUT.into(),
                holds: Some("date".into()),
                folds: None,
                required: false,
                one_of: None,
            }],
            sid: Some(crate::harness::TEST_SID.into()),
        }))
        .await
        .expect("declare_type ok");
    ensure(&setup, "thing:battery").await;
    capture_ok(
        &setup,
        CaptureArgs {
            fields: Some(
                [(attention::RUNS_OUT.to_string(), "2026-01-01".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("thing:battery", "the spare battery")
        },
    )
    .await;
    let battery = captured(&memory, "thing:battery").await;
    let spy = Arc::new(SpySearch::answering(vec![Hit::Entity {
        entity: battery,
        doc_id: "doc-battery".into(),
        edges: vec![],
        answers: None,
    }]));
    let routed = handler_on(memory.clone(), spy.clone());
    let found = json_of(
        &routed
            .recall(Parameters(RecallArgs {
                answers_type: Some("runs-out".into()),
                overdue: Some(OverdueArgs {
                    as_of: Some("2026-06-01".into()),
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        found["objects"][0]["id"], "thing:battery",
        "the routed read did not find what its own search port handed it: {found}",
    );
    assert!(
        spy.reached(),
        "a type-only, kind-less, subject-less query never asked the search port at all",
    );
    assert_eq!(
        spy.query().answers_type.as_ref().map(|t| t.name.as_str()),
        Some("runs-out"),
        "the search port was asked, but not for the type the call actually named",
    );

    // **The negative that exercises the SAME guard**: the type is still
    // named, but a kind is too — kind alone already makes `graph::walk`
    // cheap, so this must stay on the old path exactly as a bare kind
    // query does. This is what a "route everything with a type" break
    // catches that the rhythm case below cannot, because the rhythm case
    // never names a type at all. Same store as the positive half, so
    // "runs-out" is already a declared type here.
    let guarded = Arc::new(SpySearch::default());
    let kind_and_type = handler_on(memory, guarded.clone());
    let _ = json_of(
        &kind_and_type
            .recall(Parameters(RecallArgs {
                kind: Some("thing".into()),
                answers_type: Some("runs-out".into()),
                overdue: Some(OverdueArgs {
                    as_of: Some("2026-06-01".into()),
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert!(
        !guarded.reached(),
        "a query naming both a kind and a type reached the search port anyway",
    );

    // **The negative, same case: a kind-scoped overdue read — every
    // existing caller, rhythms included — must still never touch search
    // at all.** A predicate that routes everything would pass the
    // positive half above and this is the only thing that catches it.
    let rhythm_memory = shared_memory();
    let rhythm_setup = handler_on(rhythm_memory.clone(), Arc::new(SpySearch::default()));
    make_bot(&rhythm_setup, "otto").await;
    a_rhythm(&rhythm_setup, "descale", "7", "2026-01-01").await;
    let untouched = Arc::new(SpySearch::default());
    let kind_scoped = handler_on(rhythm_memory, untouched.clone());
    let overdue = json_of(
        &kind_scoped
            .recall(Parameters(RecallArgs {
                kind: Some("rhythm".into()),
                overdue: Some(OverdueArgs {
                    as_of: Some("2026-06-01".into()),
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        handles(&overdue),
        vec!["rhythm:descale".to_string()],
        "the kind-scoped read itself broke: {overdue}",
    );
    assert!(
        !untouched.reached(),
        "a kind-scoped overdue query reached the search port, which is the OLD path's job \
             alone",
    );
}

/// 🚨 **The type-only search path has its own ceiling, and nothing on the
/// wire ever said so.** `CARRIER_CANDIDATES_LIMIT` bounds the routed
/// search itself; a store holding more things than that ceiling still
/// only ever gets the first slice, and the answer reads as complete
/// either way.
///
/// **Both directions, on the same shape of query, through the real
/// surface** — a flag that is never `false` is not measuring anything.
#[tokio::test]
async fn a_type_only_recall_says_when_it_hit_its_own_candidate_ceiling() {
    let memory = shared_memory();
    let setup = handler_on(memory.clone(), Arc::new(SpySearch::default()));
    setup
        .declare_type(Parameters(DeclareTypeArgs {
            name: "runs-out".into(),
            fields: vec![FieldArgs {
                key: attention::RUNS_OUT.into(),
                holds: Some("date".into()),
                folds: None,
                required: false,
                one_of: None,
            }],
            sid: Some(crate::harness::TEST_SID.into()),
        }))
        .await
        .expect("declare_type ok");

    let one_hit = || Hit::Entity {
        entity: Entity {
            id: EntityId("thing:battery".into()),
            kind: EntityKind::THING,
            name: "the spare battery".into(),
            aliases: Vec::new(),
            source: "user-named".into(),
            crm: None,
            parent: None,
            boot: Boot::OnDemand,
            merged_into: None,
            badge: None,
            archived: None,
        },
        doc_id: "doc-battery".into(),
        edges: Vec::new(),
        answers: None,
    };

    let at_ceiling: Vec<Hit> = std::iter::repeat_with(one_hit)
        .take(CARRIER_CANDIDATES_LIMIT)
        .collect();
    let capped_reader = handler_on(memory.clone(), Arc::new(SpySearch::answering(at_ceiling)));
    let capped = json_of(
        &capped_reader
            .recall(Parameters(RecallArgs {
                answers_type: Some("runs-out".into()),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        capped["candidates_capped"], true,
        "a search answer sitting exactly at the ceiling was not reported as capped: {capped}"
    );

    let under_ceiling: Vec<Hit> = std::iter::repeat_with(one_hit)
        .take(CARRIER_CANDIDATES_LIMIT - 1)
        .collect();
    let plain_reader = handler_on(memory, Arc::new(SpySearch::answering(under_ceiling)));
    let plain = json_of(
        &plain_reader
            .recall(Parameters(RecallArgs {
                answers_type: Some("runs-out".into()),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        plain["candidates_capped"], false,
        "an answer one short of the ceiling was reported as capped anyway: {plain}"
    );
}

/// **What a mistyped key WANTED is what the declaration says, including a
/// narrowed set.**
///
/// A key narrowed to a named set holds text, because a closed vocabulary is
/// tokens — so a payload that reports the value type reports that text is
/// what the key wanted, in front of a caller looking at text. The values
/// are the whole of what the key wants, and there is one function that says
/// so.
///
/// Both halves, because either alone passes on a wrong build: the set is
/// named, and the bare value type is NOT what the answer says it wanted.
#[tokio::test]
async fn a_narrowed_key_reports_the_set_it_wanted() {
    // **A type-only, kind-less, subject-less `answers_type` query routes
    // through search** — see `type_clause` in search.rs. A setup handler
    // does the writing; the store it wrote into is what the real handler
    // reads, its search port told what that write produced.
    let memory = shared_memory();
    let setup = handler_on(memory.clone(), Arc::new(SpySearch::default()));
    ensure(&setup, "person:alpha").await;
    setup
        .declare_type(Parameters(DeclareTypeArgs {
            name: "shift".into(),
            fields: vec![FieldArgs {
                key: "worked".into(),
                holds: None,
                folds: None,
                required: false,
                one_of: Some(vec!["early".into(), "late".into()]),
            }],
            sid: Some(crate::harness::TEST_SID.into()),
        }))
        .await
        .expect("declaring a type is accepted");
    capture_ok(
        &setup,
        CaptureArgs {
            fields: Some(
                [("worked".to_string(), "overnight".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("person:alpha", "took a shift nobody named")
        },
    )
    .await;
    let alpha = captured(&memory, "person:alpha").await;
    let jojobot = handler_on(
        memory,
        Arc::new(SpySearch::answering(vec![Hit::Entity {
            entity: alpha,
            doc_id: "doc-alpha".into(),
            edges: vec![],
            answers: None,
        }])),
    );

    let body = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                answers_type: Some("shift".into()),
                facts: Some(false),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    let mistyped = &body["objects"][0]["answers"]["mistyped"][0];
    let declared = mistyped["declared"]
        .as_str()
        .unwrap_or_else(|| panic!("the key that holds something else is reported: {body}"));
    assert!(
        declared.contains("early") && declared.contains("late"),
        "the answer names the value type and not the set, so a caller reading it writes \
             another value the key does not hold: {body}",
    );
    assert_eq!(
        mistyped["value"], "overnight",
        "…and what is actually in the key: {body}",
    );
}

/// **A colleague reads the whole charter of the identity the software
/// ships, and stays nobody.**
///
/// A charter has two layers and only one of them is stored: the core the
/// build carries, and the instance's own text under it. The stored half is
/// what `prose` returns, and for the shipped identity it is the smaller
/// half — so a reader with only that route reads a fraction of a charter
/// with nothing saying so, and the whole of it was reachable only by
/// booting as that bot, which is the one act the rules refuse.
///
/// **Both layers asserted, and the core against the constant rather than a
/// quoted phrase**, so the case tracks an edit to the shipped wording where
/// a needle would go green on a build that reworded it.
///
/// 🚨 **And it boots nobody.** That is the property rather than the
/// sentence: a route that hands back a session handle has made the caller
/// somebody. Asserted over the WHOLE answer, because a handle anywhere in
/// it is a handle a caller will use.
#[tokio::test]
async fn a_charter_reads_whole_without_booting_as_the_bot_it_belongs_to() {
    let jojobot = handler();
    jojobot
        .add_entity(Parameters(add_args("bot", "assistant", "Assistant")))
        .await
        .expect("the shipped identity is an entity like any other");
    let own = "Keeps the household ledger. Never writes to it on a Sunday.";
    jojobot
        .set_charter(Parameters(SetCharterArgs {
            bot: "assistant".into(),
            prose: own.into(),
            sid: Some(crate::harness::TEST_SID.into()),
        }))
        .await
        .expect("set_charter ok");

    let body = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                charter: Some(true),
                facts: Some(false),
                ..of("bot:assistant")
            }))
            .await
            .expect("recall ok"),
    );
    let read = body["objects"][0]["charter"]
        .as_str()
        .unwrap_or_else(|| panic!("the charter comes back: {body}"))
        .to_string();
    assert_eq!(
        read, own,
        "the charter this bot answers with comes back whole: {body}",
    );
    assert!(
        !body.to_string().contains("\"sid\""),
        "this route hands back a session handle, so reading a colleague made the caller \
             somebody: {body}",
    );

    // **The page is not shipped beside the charter it is inside.** The
    // caller asked for one of them.
    assert!(
        body["objects"][0].get("prose").is_none(),
        "the page rides inside the charter answer, so sending it again is a cost \
             nobody asked for: {body}",
    );

    // **And the reader who asked for the page is told what is missing from
    // it**, since a fraction of a charter reads exactly like all of one.
    let page = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                prose: Some(true),
                facts: Some(false),
                ..of("bot:assistant")
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        page["objects"][0]["prose"], own,
        "the page is what the store holds, unchanged: {page}",
    );
    assert!(
        page["objects"][0]["charter"].is_null(),
        "the caller asked for the page, so the charter key is not shipped beside it: {page}",
    );
}

/// **A bot recalled without its charter says so, rather than leaving a
/// reader to guess whether there is nothing to read or nobody asked.**
///
/// House style everywhere else this project elides something: the answer
/// names what it left out and which call returns it (`records`, `older`,
/// `fields_backing_note`). Charter had no such marker, which is what let
/// `view:colleagues` ship every charter unasked — nothing told a caller
/// there was a cheaper answer available.
#[tokio::test]
async fn a_bot_recalled_without_charter_says_so_and_names_the_way_back() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    jojobot
        .set_charter(Parameters(SetCharterArgs {
            bot: "gamma".into(),
            prose: "Keeps the kitchen running.".into(),
            sid: Some(crate::harness::TEST_SID.into()),
        }))
        .await
        .expect("set_charter ok");

    let body = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                facts: Some(false),
                ..of("bot:gamma")
            }))
            .await
            .expect("recall ok"),
    );
    let object = &body["objects"][0];
    assert!(
        object["charter"].is_null(),
        "the charter text does not ship when nobody asked for it: {body}",
    );
    assert_eq!(
        object["charter_elided"], true,
        "the answer says the charter was left out: {body}",
    );
    let note = object["charter_note"]
        .as_str()
        .unwrap_or_else(|| panic!("the elision names the way back: {body}"));
    assert!(
        note.contains("charter: true"),
        "the note names the call that returns it: {note}",
    );
}

/// 🚨 **A bot with no charter is not "elided" — there is nothing behind
/// the note to ask again for.**
///
/// The branch above proves the note appears when a charter is really
/// being withheld. This proves the OTHER half: a bot that never had one
/// set must not get the same "ask again" advice, because asking again
/// would return nothing — a caller who took the advice would spend a
/// call to learn what this answer already knew.
#[tokio::test]
async fn a_bot_with_no_charter_is_not_told_to_ask_again_for_one() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;

    let body = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                facts: Some(false),
                ..of("bot:gamma")
            }))
            .await
            .expect("recall ok"),
    );
    let object = &body["objects"][0];
    assert!(
        object["charter"].is_null(),
        "still not shipped unasked: {body}",
    );
    assert_eq!(
        object["charter_elided"], false,
        "nothing was left out — there is no charter to elide: {body}",
    );
    assert!(
        object.get("charter_note").is_none(),
        "a bot with no charter must not be sent to ask again for one: {body}",
    );
}

/// **Asking for the charter still gets the whole of it**, and the
/// elision flag says so.
#[tokio::test]
async fn asking_for_the_charter_lifts_the_elision() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    let own = "Keeps the kitchen running.";
    jojobot
        .set_charter(Parameters(SetCharterArgs {
            bot: "gamma".into(),
            prose: own.into(),
            sid: Some(crate::harness::TEST_SID.into()),
        }))
        .await
        .expect("set_charter ok");

    let body = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                charter: Some(true),
                facts: Some(false),
                ..of("bot:gamma")
            }))
            .await
            .expect("recall ok"),
    );
    let object = &body["objects"][0];
    assert_eq!(object["charter"], own, "{body}");
    assert_eq!(
        object["charter_elided"], false,
        "nothing was left out this time: {body}",
    );
}

/// **A view shaped like the shipped `view:colleagues`** — `selects: bot`
/// and nothing else — declared the way an operator's own view is, against
/// [`handler`]'s bare store. That proves the one mechanism both paths
/// share (`asked_by_name` / `graph::asked_by_view` — rule 106) without
/// needing the real shipped data; `handler_shipped` plus
/// `the_shipped_colleagues_view_answers_with_the_small_list_by_default`
/// below is what proves the SHIPPED provision itself is this shape.
async fn declared_view(jojobot: &Jojobot, slug: &str, keys: &[(&str, &str)]) {
    let added = jojobot
        .add_entity(Parameters(add_args("view", slug, slug)))
        .await
        .expect("add_entity call ok");
    assert_ne!(
        json_of(&added)["status"],
        "blocked",
        "the fixture view {slug:?} was not created",
    );
    capture_ok(
        jojobot,
        CaptureArgs {
            fields: Some(
                keys.iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ),
            ..capture_args(&format!("view:{slug}"), "declared for a test")
        },
    )
    .await;
}

/// **A view that selects `bot` answers with the small list by default**:
/// every charter elided, none of their text anywhere in the answer.
///
/// This is the shape of the bug the operator found using `view:colleagues`:
/// six charters, tens of thousands of characters, for a question whose
/// useful answer is six lines. The shipped view no longer supplies
/// `shows: charter` (see `views.rs`), so it now takes this same default.
#[tokio::test]
async fn a_bot_selecting_view_answers_with_the_small_list_by_default() {
    let jojobot = handler();
    declared_view(&jojobot, "colleagues", &[("selects", "bot")]).await;
    make_bot(&jojobot, "gamma").await;
    make_bot(&jojobot, "delta").await;
    let expensive = "X".repeat(5_000);
    for bot in ["gamma", "delta"] {
        jojobot
            .set_charter(Parameters(SetCharterArgs {
                bot: bot.into(),
                prose: expensive.clone(),
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("set_charter ok");
    }

    let body = json_of(
        &jojobot
            .recall(Parameters(by_view("colleagues")))
            .await
            .expect("recall ok"),
    );
    assert!(
        body["count"].as_u64().unwrap_or(0) >= 2,
        "both colleagues come back: {body}",
    );
    assert!(
        !body.to_string().contains(&expensive),
        "no charter text rides along unasked: {body}",
    );
    for object in body["objects"].as_array().expect("objects is a list") {
        assert_eq!(
            object["charter_elided"], true,
            "the small list elides every charter: {object}",
        );
    }
}

/// **The opt-in still works from inside a view**: a caller who names a
/// view that selects `bot` and also asks for `charter: true` gets it,
/// because the caller's own arguments win over what the view fills in.
#[tokio::test]
async fn a_bot_selecting_view_still_hands_over_a_charter_when_asked() {
    let jojobot = handler();
    declared_view(&jojobot, "colleagues", &[("selects", "bot")]).await;
    make_bot(&jojobot, "gamma").await;
    let own = "Keeps the kitchen running.";
    jojobot
        .set_charter(Parameters(SetCharterArgs {
            bot: "gamma".into(),
            prose: own.into(),
            sid: Some(crate::harness::TEST_SID.into()),
        }))
        .await
        .expect("set_charter ok");

    let body = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                charter: Some(true),
                ..by_view("colleagues")
            }))
            .await
            .expect("recall ok"),
    );
    let gamma = body["objects"]
        .as_array()
        .expect("objects is a list")
        .iter()
        .find(|o| o["id"] == "bot:gamma")
        .unwrap_or_else(|| panic!("gamma is in the answer: {body}"));
    assert_eq!(gamma["charter"], own, "{body}");
}

/// **A view declaring `shows: facts` returns the claims unasked.**
/// `args.facts.or(asked.facts.then_some(true))` in this file is the only
/// line that reads the view's own `facts` arm; falsifying it (forced to
/// `false`) leaves every case in this module green, so this is the one
/// that must go red on it.
#[tokio::test]
async fn a_view_declaring_shows_facts_returns_the_claims_unasked() {
    let jojobot = handler();
    declared_view(
        &jojobot,
        "shows-facts",
        &[("selects", "bot"), ("shows", "facts")],
    )
    .await;
    declared_view(&jojobot, "shows-neither", &[("selects", "bot")]).await;
    make_bot(&jojobot, "gamma").await;
    capture_ok(
        &jojobot,
        capture_args("bot:gamma", "gamma keeps the kitchen running"),
    )
    .await;

    let shown = json_of(
        &jojobot
            .recall(Parameters(by_view("shows-facts")))
            .await
            .expect("recall ok"),
    );
    let gamma = shown["objects"]
        .as_array()
        .expect("objects is a list")
        .iter()
        .find(|o| o["id"] == "bot:gamma")
        .unwrap_or_else(|| panic!("gamma is in the answer: {shown}"));
    assert_eq!(
        gamma["facts"][0]["content"], "gamma keeps the kitchen running",
        "the view's own shows: facts must be honoured without the caller asking: {shown}",
    );

    let neither = json_of(
        &jojobot
            .recall(Parameters(by_view("shows-neither")))
            .await
            .expect("recall ok"),
    );
    let gamma = neither["objects"]
        .as_array()
        .expect("objects is a list")
        .iter()
        .find(|o| o["id"] == "bot:gamma")
        .unwrap_or_else(|| panic!("gamma is in the answer: {neither}"));
    assert!(
        gamma.get("facts").is_none(),
        "a view declaring neither facts nor prose must ship neither: {neither}",
    );
}

/// **A view declaring `shows: prose` returns the prose unasked** — the
/// same case as `shows: facts`, one line over in the same read.
#[tokio::test]
async fn a_view_declaring_shows_prose_returns_the_prose_unasked() {
    let jojobot = handler();
    declared_view(
        &jojobot,
        "shows-prose",
        &[("selects", "bot"), ("shows", "prose")],
    )
    .await;
    declared_view(&jojobot, "prose-not-shown", &[("selects", "bot")]).await;
    make_bot(&jojobot, "gamma").await;
    let own = "Keeps the kitchen running.";
    jojobot
        .set_charter(Parameters(SetCharterArgs {
            bot: "gamma".into(),
            prose: own.into(),
            sid: Some(crate::harness::TEST_SID.into()),
        }))
        .await
        .expect("set_charter ok");

    let shown = json_of(
        &jojobot
            .recall(Parameters(by_view("shows-prose")))
            .await
            .expect("recall ok"),
    );
    let gamma = shown["objects"]
        .as_array()
        .expect("objects is a list")
        .iter()
        .find(|o| o["id"] == "bot:gamma")
        .unwrap_or_else(|| panic!("gamma is in the answer: {shown}"));
    assert_eq!(
        gamma["prose"], own,
        "the view's own shows: prose must be honoured without the caller asking: {shown}",
    );

    let neither = json_of(
        &jojobot
            .recall(Parameters(by_view("prose-not-shown")))
            .await
            .expect("recall ok"),
    );
    let gamma = neither["objects"]
        .as_array()
        .expect("objects is a list")
        .iter()
        .find(|o| o["id"] == "bot:gamma")
        .unwrap_or_else(|| panic!("gamma is in the answer: {neither}"));
    assert!(
        gamma.get("prose").is_none(),
        "a view declaring neither facts nor prose must ship neither: {neither}",
    );
}

/// **A view declaring `shows: charter` returns the charter unasked** —
/// the third arm of the same reader, the same case as `shows: facts`
/// and `shows: prose` one line over.
#[tokio::test]
async fn a_view_declaring_shows_charter_returns_the_charter_unasked() {
    let jojobot = handler();
    declared_view(
        &jojobot,
        "shows-charter",
        &[("selects", "bot"), ("shows", "charter")],
    )
    .await;
    declared_view(&jojobot, "charter-not-shown", &[("selects", "bot")]).await;
    make_bot(&jojobot, "gamma").await;
    let own = "Keeps the kitchen running.";
    jojobot
        .set_charter(Parameters(SetCharterArgs {
            bot: "gamma".into(),
            prose: own.into(),
            sid: Some(crate::harness::TEST_SID.into()),
        }))
        .await
        .expect("set_charter ok");

    let shown = json_of(
        &jojobot
            .recall(Parameters(by_view("shows-charter")))
            .await
            .expect("recall ok"),
    );
    let gamma = shown["objects"]
        .as_array()
        .expect("objects is a list")
        .iter()
        .find(|o| o["id"] == "bot:gamma")
        .unwrap_or_else(|| panic!("gamma is in the answer: {shown}"));
    assert_eq!(
        gamma["charter"], own,
        "the view's own shows: charter must be honoured without the caller asking: {shown}",
    );

    let neither = json_of(
        &jojobot
            .recall(Parameters(by_view("charter-not-shown")))
            .await
            .expect("recall ok"),
    );
    let gamma = neither["objects"]
        .as_array()
        .expect("objects is a list")
        .iter()
        .find(|o| o["id"] == "bot:gamma")
        .unwrap_or_else(|| panic!("gamma is in the answer: {neither}"));
    assert_eq!(
        gamma["charter_elided"], true,
        "a view declaring neither key must elide the charter: {neither}",
    );
}

/// **A filter is its own record on the view** — a fact whose own
/// fields carry `key` and `value` (and, when needed, `compare` and
/// `scope`), the same shape every other claim on this store is.
async fn declared_view_filter(jojobot: &Jojobot, view_slug: &str, key: &str, value: &str) {
    capture_ok(
        jojobot,
        CaptureArgs {
            fields: Some(
                [
                    ("key".to_string(), key.to_string()),
                    ("value".to_string(), value.to_string()),
                ]
                .into(),
            ),
            ..capture_args(
                &format!("view:{view_slug}"),
                &format!("filters to {key} {value}"),
            )
        },
    )
    .await;
}

/// 🚨 **A view can hold a real question — a key filter — reaching an
/// answer a caller setting only the five view-fillable arguments cannot
/// reach without already knowing the view's own words.** This is the
/// case the slice exists to prove: before this, a view was strictly
/// less expressive than the bare read, because it could only ever fill
/// in a kind and four booleans, every one of which was already a
/// top-level argument.
///
/// The fixture view built here, `view:urgent-things`, is declared
/// rather than shipped — a test's own, never a name real callers use.
///
/// **Paired**: the view finds the matching thing and excludes the
/// other, AND the same kind selection with no view still carries both —
/// so the view's filter is doing real work, not narrowing something the
/// bare read already narrowed.
#[tokio::test]
async fn a_view_carrying_a_filter_reaches_what_the_five_flags_alone_cannot() {
    let jojobot = handler();
    declared_view(&jojobot, "urgent-things", &[("selects", "thing")]).await;
    declared_view_filter(&jojobot, "urgent-things", "priority", "urgent").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some([("priority".to_string(), "urgent".to_string())].into()),
            ..capture_args("thing:contract-view-filter-urgent", "an urgent thing")
        },
    )
    .await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some([("priority".to_string(), "routine".to_string())].into()),
            ..capture_args("thing:contract-view-filter-routine", "a routine thing")
        },
    )
    .await;

    let ids = |body: &serde_json::Value| -> Vec<String> {
        body["objects"]
            .as_array()
            .expect("objects is a list")
            .iter()
            .map(|o| {
                o["id"]
                    .as_str()
                    .expect("every object carries an id")
                    .to_string()
            })
            .collect()
    };

    let viewed = json_of(
        &jojobot
            .recall(Parameters(by_view("urgent-things")))
            .await
            .expect("recall ok"),
    );
    let viewed_ids = ids(&viewed);
    assert!(
        viewed_ids.contains(&"thing:contract-view-filter-urgent".to_string()),
        "the view's own filter must select the matching thing: {viewed}",
    );
    assert!(
        !viewed_ids.contains(&"thing:contract-view-filter-routine".to_string()),
        "the view's own filter must exclude the non-matching thing: {viewed}",
    );

    let bare = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: None,
                kind: Some("thing".into()),
                ..of("unused")
            }))
            .await
            .expect("recall ok"),
    );
    let bare_ids = ids(&bare);
    assert!(
        bare_ids.contains(&"thing:contract-view-filter-routine".to_string()),
        "a bare kind selection must not already exclude what only the view's filter does, \
             or the view proves nothing: {bare}",
    );
}

/// **A caller's own `fields` still wins over the view's** — a view is a
/// starting point, not a cage, the same rule every other view-fillable
/// argument already answers to.
///
/// **Paired the same way the case above is**: the caller's filter keeps
/// its own match AND excludes the view's — an unfiltered answer would
/// carry both and pass the positive half for the wrong reason.
#[tokio::test]
async fn a_callers_own_fields_still_override_the_views_filter() {
    let jojobot = handler();
    declared_view(&jojobot, "urgent-things", &[("selects", "thing")]).await;
    declared_view_filter(&jojobot, "urgent-things", "priority", "urgent").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some([("priority".to_string(), "urgent".to_string())].into()),
            ..capture_args("thing:contract-view-filter-urgent", "an urgent thing")
        },
    )
    .await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some([("priority".to_string(), "routine".to_string())].into()),
            ..capture_args("thing:contract-view-filter-routine", "a routine thing")
        },
    )
    .await;

    let body = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                fields: Some(vec![KeyFilterArgs {
                    key: Some("priority".into()),
                    value: Some("routine".into()),
                    compare: None,
                    scope: None,
                }]),
                ..by_view("urgent-things")
            }))
            .await
            .expect("recall ok"),
    );
    let ids: Vec<String> = body["objects"]
        .as_array()
        .expect("objects is a list")
        .iter()
        .map(|o| {
            o["id"]
                .as_str()
                .expect("every object carries an id")
                .to_string()
        })
        .collect();
    assert!(
        ids.contains(&"thing:contract-view-filter-routine".to_string()),
        "the caller's own filter must keep its own match: {body}",
    );
    assert!(
        !ids.contains(&"thing:contract-view-filter-urgent".to_string()),
        "the caller's own filter must win over the view's, excluding what only the view's \
             filter would have kept: {body}",
    );
}

/// 🚨 **A view can name a type to select structurally** — the same
/// `answers_type` the argument already has, reached by naming the view
/// rather than the type. A bare kind selection cannot tell the two
/// things apart; only the type name can.
///
/// **Paired**: the view keeps the thing carrying the type's key and
/// excludes the one carrying none of it, AND the same kind selection
/// with no view still carries both.
///
/// The fixture view, `view:marked-things`, is declared here rather than
/// shipped.
#[tokio::test]
async fn a_view_naming_a_type_reaches_what_a_bare_kind_cannot() {
    let jojobot = handler();
    jojobot
        .declare_type(Parameters(DeclareTypeArgs {
            name: "contract-view-answers-marker".into(),
            fields: vec![FieldArgs {
                key: "flagged".into(),
                holds: None,
                folds: None,
                required: false,
                one_of: None,
            }],
            sid: Some(crate::harness::TEST_SID.into()),
        }))
        .await
        .expect("declaring a type is accepted");
    capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some([("flagged".to_string(), "yes".to_string())].into()),
            ..capture_args("thing:contract-view-answers-marked", "carries the marker")
        },
    )
    .await;
    ensure(&jojobot, "thing:contract-view-second-thing").await;

    declared_view(
        &jojobot,
        "marked-things",
        &[
            ("selects", "thing"),
            ("answers_type", "contract-view-answers-marker"),
        ],
    )
    .await;

    let ids_of = |body: &serde_json::Value| -> Vec<String> {
        body["objects"]
            .as_array()
            .expect("objects is a list")
            .iter()
            .map(|o| {
                o["id"]
                    .as_str()
                    .expect("every object carries an id")
                    .to_string()
            })
            .collect()
    };
    let viewed = json_of(
        &jojobot
            .recall(Parameters(by_view("marked-things")))
            .await
            .expect("recall ok"),
    );
    let viewed_ids = ids_of(&viewed);
    assert!(
        viewed_ids.contains(&"thing:contract-view-answers-marked".to_string()),
        "the view's own type must select the thing carrying the key: {viewed}",
    );
    assert!(
        !viewed_ids.contains(&"thing:contract-view-second-thing".to_string()),
        "the view's own type must exclude the thing carrying none of its keys: {viewed}",
    );

    let bare = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: None,
                kind: Some("thing".into()),
                ..of("unused")
            }))
            .await
            .expect("recall ok"),
    );
    let bare_ids = ids_of(&bare);
    assert!(
        bare_ids.contains(&"thing:contract-view-second-thing".to_string()),
        "a bare kind selection must not already exclude what only the view's type does, or \
             the view proves nothing: {bare}",
    );
}

/// 🚨 **A view can name a relation walk — a key, a direction, a depth —
/// reaching what it walks to rather than what it names directly.** A
/// bare kind or subject selection cannot reach a neighbour; only a walk
/// can.
///
/// The fixture view, `view:my-pets`, is declared here rather than
/// shipped.
#[tokio::test]
async fn a_view_naming_a_walk_reaches_the_neighbour_a_bare_selection_cannot() {
    let jojobot = handler();
    jojobot
        .declare_type(Parameters(DeclareTypeArgs {
            name: "contract-view-walk-pet".into(),
            fields: vec![FieldArgs {
                key: "owner".into(),
                holds: Some("reference".into()),
                folds: None,
                required: false,
                one_of: None,
            }],
            sid: Some(crate::harness::TEST_SID.into()),
        }))
        .await
        .expect("declaring a type is accepted");
    ensure(&jojobot, "person:contract-view-walk-owner").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(
                [(
                    "owner".to_string(),
                    "person:contract-view-walk-owner".to_string(),
                )]
                .into(),
            ),
            ..capture_args("pet:contract-view-walk-pet", "belongs to its owner")
        },
    )
    .await;

    declared_view(
        &jojobot,
        "my-pets",
        &[("follow_relation", "owner"), ("follow_direction", "in")],
    )
    .await;

    let body = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("person:contract-view-walk-owner".into()),
                ..by_view("my-pets")
            }))
            .await
            .expect("recall ok"),
    );
    let object = &body["objects"][0];
    let connected: Vec<&str> = object["connected"]
        .as_array()
        .expect("connected is a list")
        .iter()
        .map(|c| c["id"].as_str().expect("a connected object carries an id"))
        .collect();
    assert!(
        connected.contains(&"pet:contract-view-walk-pet"),
        "the view's own walk must reach the pet through the owner relation: {body}",
    );
}

/// **A one-liner rides in the small list as an ordinary field** — no new
/// mechanism, and no fallback to the charter. A bot that never had one
/// written simply carries no `one_liner` key, exactly as any other
/// unwritten field reads: this is what "reads back as having none" means,
/// as opposed to deriving one from the charter's first line.
#[tokio::test]
async fn a_colleagues_one_liner_rides_in_the_small_list_and_absence_is_plain() {
    let jojobot = handler();
    declared_view(&jojobot, "colleagues", &[("selects", "bot")]).await;
    make_bot(&jojobot, "gamma").await;
    make_bot(&jojobot, "delta").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(
                [(
                    crate::orientation::charter::ONE_LINER_KEY.to_string(),
                    "Keeps the kitchen running.".to_string(),
                )]
                .into_iter()
                .collect(),
            ),
            ..capture_args("bot:gamma", "wrote its own one-liner")
        },
    )
    .await;

    let body = json_of(
        &jojobot
            .recall(Parameters(by_view("colleagues")))
            .await
            .expect("recall ok"),
    );
    let objects = body["objects"].as_array().expect("objects is a list");
    let gamma = objects
        .iter()
        .find(|o| o["id"] == "bot:gamma")
        .unwrap_or_else(|| panic!("gamma is in the answer: {body}"));
    assert_eq!(
        gamma["fields"]["one_liner"], "Keeps the kitchen running.",
        "{body}",
    );
    let delta = objects
        .iter()
        .find(|o| o["id"] == "bot:delta")
        .unwrap_or_else(|| panic!("delta is in the answer: {body}"));
    assert!(
        delta["fields"].get("one_liner").is_none(),
        "a bot with none written carries no key at all, rather than a derived one: {body}",
    );
}

/// **The SHIPPED `view:colleagues`, resolved through this crate's own
/// `provisions()` — not a declared stand-in of the same shape.**
///
/// Every other case here proves the mechanism a shipped view runs
/// through; this one proves the shipped DATA is what it should be. A
/// typo in `views.rs`'s handle, or a `shows` key nobody meant to ship,
/// would pass every case built on `handler()` and only reddens here.
#[tokio::test]
async fn the_shipped_colleagues_view_answers_with_the_small_list_by_default() {
    let jojobot = handler_shipped();
    make_bot(&jojobot, "gamma").await;
    make_bot(&jojobot, "delta").await;
    let expensive = "X".repeat(5_000);
    jojobot
        .set_charter(Parameters(SetCharterArgs {
            bot: "gamma".into(),
            prose: expensive.clone(),
            sid: Some(crate::harness::TEST_SID.into()),
        }))
        .await
        .expect("set_charter ok");

    let body = json_of(
        &jojobot
            .recall(Parameters(by_view("colleagues")))
            .await
            .expect("recall ok"),
    );
    assert!(
        body["count"].as_u64().unwrap_or(0) >= 2,
        "the shipped view selects bots: {body}",
    );
    assert!(
        !body.to_string().contains(&expensive),
        "the shipped view does not carry a charter unasked: {body}",
    );
}

/// The whole-page query for a `view`, spelled once: no subject, no facts —
/// the view supplies what the call asks for a query of its own.
fn by_view(name: &str) -> RecallArgs {
    RecallArgs {
        view: Some(name.into()),
        subject: None,
        facts: None,
        ..of("unused")
    }
}

/// **A key's value selects, and the walk nests** — the two halves this
/// verb grew, through the surface a caller uses.
#[tokio::test]
async fn a_value_selects_and_a_walk_nests() {
    let jojobot = handler();
    for (kind, slug, name) in [
        ("event", "birthday-party", "Birthday Party"),
        ("person", "patana", "Patana"),
    ] {
        jojobot
            .add_entity(Parameters(add_args(kind, slug, name)))
            .await
            .expect("add_entity ok");
    }
    capture_ok(
        &jojobot,
        CaptureArgs {
            shape: Some("attendance".into()),
            object: Some("event:birthday-party".into()),
            fields: Some(
                [("answer".to_string(), "yes".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("patana", "coming to the party")
        },
    )
    .await;
    capture_ok(&jojobot, capture_args("patana", "vegetarian")).await;

    let selected = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                fields: Some(vec![KeyFilterArgs {
                    key: Some("answer".into()),
                    value: Some("yes".into()),
                    compare: None,
                    // The record that answered is what this half is about,
                    // so it asks the question that is about records.
                    scope: Some("record".into()),
                }]),
                // The claim that answered is what this case is about, so
                // it asks for the records the fold is taken from.
                facts: Some(true),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(selected["objects"][0]["id"], "person:patana", "{selected}");
    assert_eq!(
        selected["objects"][0]["facts"].as_array().map(Vec::len),
        Some(1),
        "the object carries the record that answered, not its whole page: {selected}"
    );

    let walked = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("event:birthday-party".into()),
                follow: Some(FollowArgs {
                    shape: Some("attendance".into()),
                    relation: None,
                    direction: Some("in".into()),
                    depth: None,
                    keeping: None,
                    fits_type: None,
                }),
                // The claim a reached object carries is what the walk half
                // asserts on, so this half asks for the records too.
                facts: Some(true),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    let guest = &walked["objects"][0]["connected"][0];
    assert_eq!(guest["id"], "person:patana", "{walked}");
    assert_eq!(guest["via"]["direction"], "in", "{walked}");
    assert!(
        guest["facts"]
            .as_array()
            .expect("the guest's own facts")
            .iter()
            .any(|f| f["content"] == "vegetarian"),
        "a reached object arrives carrying its own page: {walked}"
    );
}

/// **A wide inbound fan-in is capped, and the cap says what it left out.**
///
/// The paired positive is the case just above this one
/// (`a_value_selects_and_a_walk_nests`): an ordinary, small `connected`
/// carries no elision note at all. This is the negative — a fan-in wide
/// enough to overflow the budget — so the two together prove the note
/// appears exactly when something was actually left out, never as noise
/// on an ordinary answer.
#[tokio::test]
async fn a_wide_inbound_fan_in_is_capped_and_names_what_it_left_out() {
    let jojobot = handler();
    ensure(&jojobot, "place:leftorium").await;
    // Every one of these is already on the fixture roster for other
    // tests; reusing them costs nothing new there and is plenty to
    // overflow a 20,000-character budget at a couple of hundred
    // characters per bare connected object. Long and mutually distant —
    // the write guard's near-slug screen (edit distance <= 2) would
    // otherwise block a set this size before the walk is ever reached.
    let admirers = [
        "contract-chainless",
        "contract-citation-editpath",
        "contract-claim-histories",
        "contract-corrected",
        "contract-current-handle-now",
        "contract-derived",
        "contract-fields",
        "contract-graph-fold",
        "contract-orient",
        "contract-revision-count",
        "contract-stands-for-basic",
        "contract-stands-for-decoy",
        "contract-archived-link-moved-on",
        "contract-addressable",
        "contract-alias-borrower",
        "contract-pumpback",
        "contract-summertime",
        "contract-appended",
        "contract-away-talker",
        "contract-backing",
        "contract-backslash",
        "contract-brimful",
        "contract-cleared",
        "contract-clear-marker",
        "contract-clocks",
        "contract-confirmed-guess",
        "contract-conn-one",
        "contract-counted",
        "contract-crate-partial",
        "contract-crate-whole",
        "contract-crosslink",
        "contract-demotable",
        "contract-duet",
        "contract-edged",
        "contract-edge-guarded",
        "contract-editable",
        "contract-evented",
        "contract-field-edit",
        "contract-folded-kept",
        "contract-fold-rename-before",
        "contract-gene",
        "contract-filed",
        "contract-graph-coming",
        "contract-hedged-word",
        "contract-inked",
        "contract-hijack-subject",
        "contract-injector",
        "contract-late-edge",
        "contract-lineage",
        "contract-listed-pointer-alpha",
        "contract-many-labelled",
        "contract-mention-author",
        "contract-parent-child",
        "contract-rename-author",
        "contract-retype-parent",
        "contract-stale-edit-was",
        "contract-mention-bookkeeper",
        "contract-mention-broken",
        "contract-milhouse",
        "contract-miskinded",
        "contract-missing-row",
        "contract-multi",
        "contract-nearslug",
        "contract-never-captured",
        "contract-nickname-only",
    ];
    for admirer in admirers {
        capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some("about".into()),
                object: Some("place:leftorium".into()),
                ..capture_args(admirer, "keeps talking about the store")
            },
        )
        .await;
    }

    let walked = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("place:leftorium".into()),
                follow: Some(FollowArgs {
                    shape: Some("about".into()),
                    relation: None,
                    direction: Some("in".into()),
                    depth: None,
                    keeping: None,
                    fits_type: None,
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    let root = &walked["objects"][0];
    let connected = root["connected"]
        .as_array()
        .expect("a capped connected list is still a list");
    assert!(
        connected.len() < admirers.len(),
        "the fan-in overflowed the budget, so fewer than all {} admirers came back: {walked}",
        admirers.len()
    );
    let left_out = root["connected_left_out"]
        .as_str()
        .expect("a capped answer names what it dropped");
    let left_out_count: usize = left_out
        .split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .expect("the note leads with the count");
    assert_eq!(
        connected.len() + left_out_count,
        admirers.len(),
        "kept plus left out accounts for every admirer, none silently vanished: {walked}"
    );
    assert!(
        left_out.contains("recall"),
        "the note names the way back to the rest: {left_out}"
    );
}

/// **A `record`-scoped filter narrows what comes back and says so.**
///
/// The unscoped question is the positive half: everything on the page
/// comes back, so the paired negative — the scoped answer naming what it
/// left out — is not passing over an empty set. `facts_held` is fixed
/// against the unscoped read, so a build that let a `record` filter
/// shrink it back down would fail the second assertion even though the
/// first still passes.
#[tokio::test]
async fn a_record_scoped_filter_says_what_it_left_out() {
    let jojobot = handler();
    ensure(&jojobot, "patana").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(
                [("answer".to_string(), "yes".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("patana", "coming to the party")
        },
    )
    .await;
    capture_ok(&jojobot, capture_args("patana", "vegetarian")).await;

    let unscoped = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("person:patana".into()),
                facts: Some(true),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        unscoped["objects"][0]["facts"].as_array().map(Vec::len),
        Some(2),
        "the unscoped question returns the full page: {unscoped}"
    );
    assert!(
        unscoped["objects"][0].get("scoped_out").is_none(),
        "nothing was left out, so there is nothing to name: {unscoped}"
    );

    let scoped = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("person:patana".into()),
                fields: Some(vec![KeyFilterArgs {
                    key: Some("answer".into()),
                    value: Some("yes".into()),
                    compare: None,
                    scope: Some("record".into()),
                }]),
                facts: Some(true),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        scoped["objects"][0]["facts"].as_array().map(Vec::len),
        Some(1),
        "the scoped question narrows what comes back: {scoped}"
    );
    assert_eq!(
        scoped["objects"][0]["scoped_out"],
        "1 more records exist on this thing and do not answer the record filter — ask \
             again with a broader one, or none, to read them",
        "the note names what the scope left out: {scoped}"
    );
}

/// **A query that narrows nothing is refused**, with a way forward rather
/// than a protocol failure — and the same call with one filter is served,
/// so the refusal is about the argument and not about the verb.
#[tokio::test]
async fn a_query_that_narrows_nothing_is_blocked() {
    let jojobot = handler();
    let refused = blocked(
        &jojobot
            .recall(Parameters(of_nothing()))
            .await
            .expect("a malformed query is an answer, not a protocol failure"),
    );
    assert_eq!(refused["wrote"], false, "{refused}");

    jojobot
        .recall(Parameters(RecallArgs {
            kind: Some("person".into()),
            ..of_nothing()
        }))
        .await
        .expect("one filter is enough");
}

/// **A type query reaches a thing nobody labelled.**
///
/// A thing answers a type by the keys its records carry, and they carried
/// them whether or not their writer also named a class. While a key could
/// only be written beside a label, the set a type query ran over was the
/// set that opted in, so a type reported the writers who knew about it
/// rather than the things that answer it.
#[tokio::test]
async fn a_type_query_reaches_a_record_written_with_no_label() {
    // Same shape as `a_narrowed_key_reports_the_set_it_wanted`: a
    // type-only query with no kind and no subject routes through search,
    // so the setup writes and the search port that answers for them are
    // built apart, over one shared store.
    let memory = shared_memory();
    let setup = handler_on(memory.clone(), Arc::new(SpySearch::default()));
    ensure(&setup, "alpha").await;
    setup
        .declare_type(Parameters(DeclareTypeArgs {
            name: "service".into(),
            fields: vec![FieldArgs {
                key: "odometer".into(),
                holds: Some("number".into()),
                folds: None,
                required: false,
                one_of: None,
            }],
            sid: Some(crate::harness::TEST_SID.into()),
        }))
        .await
        .expect("declare_type ok");
    capture_ok(
        &setup,
        CaptureArgs {
            fields: Some([("odometer".to_string(), "18000".to_string())].into()),
            ..capture_args("person:alpha", "the chain was replaced")
        },
    )
    .await;
    let alpha = captured(&memory, "person:alpha").await;
    let jojobot = handler_on(
        memory,
        Arc::new(SpySearch::answering(vec![Hit::Entity {
            entity: alpha,
            doc_id: "doc-alpha".into(),
            edges: vec![],
            answers: None,
        }])),
    );

    let found = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                answers_type: Some("service".into()),
                facts: Some(true),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert_eq!(
        found["objects"][0]["id"], "person:alpha",
        "the record answers the type by its key: {found}"
    );
    assert_eq!(
        found["objects"][0]["facts"][0]["content"], "the chain was replaced",
        "{found}"
    );
}

/// **A declared reference key is walkable through the wire**, both ways,
/// and the ordering a declaration licenses narrows what the walk reaches.
///
/// The whole slice through the surface a caller holds: nothing here calls
/// the resolver, so it fails on a build where the arguments never reach it.
#[tokio::test]
async fn a_relation_is_walkable_and_a_walk_can_filter_what_it_reaches() {
    let jojobot = handler();
    for (kind, slug, name) in [
        ("person", "bart", "Bart"),
        ("pet", "santas-little-helper", "Santa's Little Helper"),
        ("pet", "snowball", "Snowball"),
    ] {
        jojobot
            .add_entity(Parameters(add_args(kind, slug, name)))
            .await
            .expect("add_entity ok");
    }
    jojobot
        .declare_type(Parameters(DeclareTypeArgs {
            name: "pet".into(),
            fields: vec![
                FieldArgs {
                    key: "born".into(),
                    holds: Some("date".into()),
                    folds: None,
                    required: false,
                    one_of: None,
                },
                FieldArgs {
                    key: "owner".into(),
                    holds: Some("reference".into()),
                    folds: None,
                    required: false,
                    one_of: None,
                },
            ],
            sid: Some(crate::harness::TEST_SID.into()),
        }))
        .await
        .expect("declare_type ok");
    for (slug, born) in [
        ("santas-little-helper", "2019-04-15"),
        ("snowball", "2024-11-02"),
    ] {
        capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [
                        ("born".to_string(), born.to_string()),
                        ("owner".to_string(), "person:bart".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args(&format!("pet:{slug}"), "one of the pets")
            },
        )
        .await;
    }

    let has_many = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("person:bart".into()),
                facts: Some(false),
                follow: Some(FollowArgs {
                    relation: Some("owner".into()),
                    direction: Some("in".into()),
                    ..no_follow()
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    let reached: Vec<&str> = has_many["objects"][0]["connected"]
        .as_array()
        .expect("the pets hang off the owner")
        .iter()
        .filter_map(|o| o["id"].as_str())
        .collect();
    assert_eq!(
        reached,
        vec!["pet:santas-little-helper", "pet:snowball"],
        "the reverse of a declared reference key is the has-many: {has_many}"
    );
    assert_eq!(
        has_many["objects"][0]["connected"][0]["via"]["relation"], "owner",
        "and a reached object says which relation carried it: {has_many}"
    );

    let older = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("person:bart".into()),
                facts: Some(false),
                follow: Some(FollowArgs {
                    relation: Some("owner".into()),
                    direction: Some("in".into()),
                    keeping: Some(vec![KeyFilterArgs {
                        key: Some("born".into()),
                        value: Some("2020-01-01".into()),
                        compare: Some("before".into()),
                        scope: None,
                    }]),
                    ..no_follow()
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    let kept: Vec<&str> = older["objects"][0]["connected"]
        .as_array()
        .expect("a filtered walk still answers with a list")
        .iter()
        .filter_map(|o| o["id"].as_str())
        .collect();
    assert_eq!(
        kept,
        vec!["pet:santas-little-helper"],
        "the ordering the declaration licenses narrows what the walk reaches: {older}"
    );
}

/// **An ordering no declaration licenses is refused**, and so is a name no
/// declaration backs, and so is a call naming both link vocabularies at
/// once. Each comes back blocked with a way forward, rather than as an
/// answer to a question nobody asked.
#[tokio::test]
async fn an_unlicensed_ordering_and_an_unbacked_relation_are_blocked() {
    let jojobot = handler();
    jojobot
        .add_entity(Parameters(add_args("person", "bart", "Bart")))
        .await
        .expect("add_entity ok");

    let unlicensed = blocked(
        &jojobot
            .recall(Parameters(RecallArgs {
                fields: Some(vec![KeyFilterArgs {
                    key: Some("born".into()),
                    value: Some("2020-01-01".into()),
                    compare: Some("before".into()),
                    scope: None,
                }]),
                ..of_nothing()
            }))
            .await
            .expect("an unlicensed ordering is an answer, not a protocol failure"),
    );
    assert_eq!(unlicensed["wrote"], false, "{unlicensed}");

    // A name no declaration backs. `pet.owner` was a relation name once
    // and is not one now, so this is also the case that pins the rename.
    let unbacked = blocked(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("person:bart".into()),
                follow: Some(FollowArgs {
                    relation: Some("pet.owner".into()),
                    ..no_follow()
                }),
                ..of_nothing()
            }))
            .await
            .expect("a name no declaration backs is an answer, not a protocol failure"),
    );
    assert_eq!(unbacked["wrote"], false, "{unbacked}");

    // **Both vocabularies at once, under ONE word.** A key may be spelled
    // like an edge shape, so the refusal has to be legible when the two
    // names are identical rather than merely adjacent.
    let both = blocked(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("person:bart".into()),
                follow: Some(FollowArgs {
                    shape: Some("location".into()),
                    relation: Some("location".into()),
                    ..no_follow()
                }),
                ..of_nothing()
            }))
            .await
            .expect("naming both vocabularies is an answer, not a protocol failure"),
    );
    assert_eq!(both["wrote"], false, "{both}");
    let why = both.to_string();
    assert!(
        why.contains("shape") && why.contains("relation"),
        "the refusal says which vocabulary is which, or one word names two things: {both}"
    );

    // The positive the refusals rest on: the same shape of call, naming one
    // vocabulary, is served.
    jojobot
        .recall(Parameters(RecallArgs {
            subject: Some("person:bart".into()),
            follow: Some(FollowArgs {
                shape: Some("connection".into()),
                direction: Some("in".into()),
                ..no_follow()
            }),
            ..of_nothing()
        }))
        .await
        .expect("an edge shape takes a direction");
}

/// A `follow` naming nothing — the base the walk cases vary.
/// 🚨 **A value found without naming the key it is under, through the
/// surface a caller holds — and prose that does not match.**
///
/// Selecting by a named key was expressible and reporting one key's values
/// was expressible. **Neither asked whether a string is a value at all**,
/// which is what *did somebody record this so it can be asked for later*
/// reduces to.
///
/// **The prose half is what keeps it from being the breadth verb.** A
/// string written into a claim's sentence is not something a later question
/// can be asked of, and an answer that matched it would be `search` under
/// another name.
#[tokio::test]
async fn a_value_is_found_without_naming_its_key_and_prose_is_not() {
    let jojobot = handler();
    let sid = writing_as(&jojobot);
    ensure(&jojobot, "person:beta").await;
    let day = "2026-08-11";
    capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(sid.clone()),
            fields: Some(
                [("serviced_on".to_string(), day.to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("person:alpha", "the service happened")
        },
    )
    .await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            sid: Some(sid.clone()),
            ..capture_args("person:beta", "the service happened on 2026-08-11")
        },
    )
    .await;

    let holding = |value: &str| {
        let value = value.to_string();
        let sid = sid.clone();
        async {
            json_of(
                &jojobot
                    .recall(Parameters(RecallArgs {
                        fields: Some(vec![KeyFilterArgs {
                            key: None,
                            value: Some(value),
                            compare: None,
                            scope: None,
                        }]),
                        sid: Some(sid),
                        ..of_nothing()
                    }))
                    .await
                    .expect("a value filter is a selection"),
            )
        }
    };

    let found = holding(day).await;
    assert!(
        found.to_string().contains("person:alpha"),
        "a value stored under a key was not found by asking for the value: {found}",
    );
    assert!(
        !found.to_string().contains("person:beta"),
        "the same string in a claim's prose matched, so this is the breadth verb rather than \
             a question about what is held: {found}",
    );
    assert_eq!(
        holding("2011-01-01").await["count"],
        0,
        "a string nothing holds came back with objects",
    );

    // ⛔️ **A filter that asks nothing is refused rather than answered.**
    // It selects everything or nothing depending which way the predicate
    // falls, and neither is what anybody asked for.
    let empty = blocked(
        &jojobot
            .recall(Parameters(RecallArgs {
                fields: Some(vec![KeyFilterArgs {
                    key: None,
                    value: None,
                    compare: None,
                    scope: None,
                }]),
                sid: Some(sid),
                ..of_nothing()
            }))
            .await
            .expect("a filter asking nothing is an answer, not a protocol failure"),
    );
    assert_eq!(empty["wrote"], false, "{empty}");
}

fn no_follow() -> FollowArgs {
    FollowArgs {
        shape: None,
        relation: None,
        direction: None,
        depth: None,
        keeping: None,
        fits_type: None,
    }
}

/// A call naming nothing at all — the base every case above varies.
fn of_nothing() -> RecallArgs {
    RecallArgs {
        view: None,
        subject: None,
        kind: None,
        answers_type: None,
        fields: None,
        facts: None,
        stood_for: None,
        prose: None,
        charter: None,
        follow: None,
        overdue: None,
        near: None,
        sid: None,
        history: None,
        history_record: None,
        history_most: None,
        values: None,
        values_most: None,
        built_on: None,
        backing: None,
    }
}

/// 🚨 **A query naming a displaced type does not match silently against
/// the wrong shape.** Before this, `answers_type` resolved straight to
/// the type the software ships now — the same silence `declare_type`'s
/// own refusal used to carry, on the one path that never reaches a
/// refusal at all: a caller who queries by name never tries to
/// redeclare it.
///
/// **Once per answer, not per object.** The note is a property of the
/// NAME the call resolved, not of how many things it matched —
/// asserted here with the search port answering nothing at all, so the
/// note cannot be riding on a hit.
#[tokio::test]
async fn a_query_naming_a_displaced_type_names_it_once_in_the_answer() {
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};

    let memory = shared_memory();
    let jojobot = handler_on(memory.clone(), Arc::new(SpySearch::default()));
    memory
        .declare_type(DeclaredType::new(
            "roster",
            vec![Field::new("shift_lead", ValueType::Reference)],
        ))
        .await
        .expect("the caller's own declaration lands");
    memory
        .declare_type(DeclaredType::shipped(
            "roster",
            vec![Field::new("starts", ValueType::Date)],
        ))
        .await
        .expect("the software's own write replaces a caller's");

    let found = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                answers_type: Some("roster".into()),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    let notes = found["type_displaced"]
        .as_array()
        .unwrap_or_else(|| panic!("names what the query's own type displaced: {found}"));
    assert_eq!(notes.len(), 1, "once per answer, not per object: {found}");
    assert!(
        notes[0].as_str().unwrap_or_default().contains("shift_lead"),
        "names the caller's own key: {found}"
    );
}

/// **The positive above rests on this**: a type never displaced names
/// nothing. Without it, an answer that always carried something under
/// `type_displaced` would pass the case above for the wrong reason.
#[tokio::test]
async fn a_query_naming_a_type_never_displaced_names_nothing() {
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};

    let memory = shared_memory();
    let jojobot = handler_on(memory.clone(), Arc::new(SpySearch::default()));
    memory
        .declare_type(DeclaredType::shipped(
            "always-shipped-recall",
            vec![Field::new("starts", ValueType::Date)],
        ))
        .await
        .expect("the software declares its own types");

    let found = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                answers_type: Some("always-shipped-recall".into()),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    assert!(
        found.get("type_displaced").is_none(),
        "nothing was ever a caller's under this name, so nothing is named: {found}"
    );
}

/// **`follow`'s own served description names what an unscoped walk now
/// reaches** — a capability whose own verb description does not mention
/// it has no path to it.
#[test]
fn an_unscoped_follow_names_mention_and_ref_on_the_verbs_own_description() {
    let tools = Jojobot::tool_router().list_all();
    let recall = tools
        .iter()
        .find(|t| t.name.as_ref() == "recall")
        .expect("recall is a tool");
    let tool_description = recall.description.as_deref().unwrap_or_default();
    assert!(
        tool_description.contains("MENTION"),
        "the tool-level description does not name mention: {tool_description}"
    );
    assert!(
        tool_description.contains("REF"),
        "the tool-level description does not name ref: {tool_description}"
    );
}

/// **An unscoped `follow` reaches an edge, a mention and a ref at once,
/// each labelled apart from the others** — proven over the production
/// stack (`mention::Mentioning` in front of memory), because a bare
/// store would pass this for the wrong reason: an unrendered mention is
/// ordinary text, not a link.
#[tokio::test]
async fn an_unscoped_follow_in_reaches_the_edge_the_mention_and_the_ref_labelled_apart() {
    let jojobot = handler_mentioning();
    ensure(&jojobot, "topic:all-rules").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            object: Some("topic:all-rules".into()),
            shape: Some("connection".into()),
            ..capture_args(
                "person:homer",
                "draws an edge, its own words never repeat the target's name",
            )
        },
    )
    .await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            ..capture_args(
                "person:gayle",
                "mentions @topic:all-rules directly in its own words",
            )
        },
    )
    .await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            refs: Some(vec!["topic:all-rules".into()]),
            ..capture_args("person:hugo", "touches it without saying how")
        },
    )
    .await;

    let walked = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("topic:all-rules".into()),
                follow: Some(FollowArgs {
                    direction: Some("in".into()),
                    ..no_follow()
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    let reached = walked["objects"][0]["connected"]
        .as_array()
        .expect("connected objects");
    assert_eq!(reached.len(), 3, "all three routes are reached: {walked}");
    let via_of = |handle: &str| {
        reached
            .iter()
            .find(|o| o["id"] == handle)
            .unwrap_or_else(|| panic!("{handle} was not reached: {walked}"))["via"]
            .clone()
    };
    assert_eq!(
        via_of("person:homer")["type"],
        EdgeShape::Connection.as_name(),
        "the edge writer is labelled as its edge shape: {walked}"
    );
    assert_eq!(
        via_of("person:gayle")["mention"],
        true,
        "the mention writer is labelled as a mention, never as an edge: {walked}"
    );
    assert_eq!(
        via_of("person:hugo")["ref"],
        true,
        "the ref writer is labelled as a ref: {walked}"
    );
}

/// **A shaped walk answers exactly as it always has**: only the edge
/// claim, the negative paired with the positive above so a version that
/// dropped the shape filter entirely could not pass both.
#[tokio::test]
async fn a_follow_scoped_to_one_edge_shape_never_returns_a_mention_or_a_ref() {
    let jojobot = handler_mentioning();
    ensure(&jojobot, "topic:all-rules").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            object: Some("topic:all-rules".into()),
            shape: Some("connection".into()),
            ..capture_args(
                "person:homer",
                "draws an edge, its own words never repeat the target's name",
            )
        },
    )
    .await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            ..capture_args(
                "person:gayle",
                "mentions @topic:all-rules directly in its own words",
            )
        },
    )
    .await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            refs: Some(vec!["topic:all-rules".into()]),
            ..capture_args("person:hugo", "touches it without saying how")
        },
    )
    .await;

    let walked = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("topic:all-rules".into()),
                follow: Some(FollowArgs {
                    shape: Some("connection".into()),
                    direction: Some("in".into()),
                    ..no_follow()
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    let reached = walked["objects"][0]["connected"]
        .as_array()
        .expect("connected objects");
    assert_eq!(
        reached.iter().map(|o| o["id"].clone()).collect::<Vec<_>>(),
        vec!["person:homer"],
        "only the edge claim answers a walk scoped to its own shape: {walked}"
    );
}

/// **The outbound mirror**: from the subject whose claim mentions the
/// target, an unscoped walk out reaches it, labelled as a mention.
#[tokio::test]
async fn an_unscoped_follow_out_reaches_the_target_of_its_own_mention() {
    let jojobot = handler_mentioning();
    ensure(&jojobot, "topic:all-rules").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            ..capture_args(
                "person:gayle",
                "mentions @topic:all-rules directly in its own words",
            )
        },
    )
    .await;

    let walked = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("person:gayle".into()),
                follow: Some(FollowArgs {
                    direction: Some("out".into()),
                    ..no_follow()
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    let reached = walked["objects"][0]["connected"]
        .as_array()
        .expect("connected objects");
    assert_eq!(
        reached.iter().map(|o| o["id"].clone()).collect::<Vec<_>>(),
        vec!["topic:all-rules"],
        "the mentioned target is reached going out: {walked}"
    );
    assert_eq!(
        reached[0]["via"]["mention"], true,
        "labelled as a mention: {walked}"
    );
}

/// **A rename of the target is still followed through a mention** — the
/// reversal is rebuilt fresh from already-rendered facts on every walk,
/// so it never holds a stale handle to begin with.
#[tokio::test]
async fn a_rename_of_the_target_is_still_followed_through_a_mention() {
    let jojobot = handler_mentioning();
    ensure(&jojobot, "topic:all-rules").await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            ..capture_args(
                "person:gayle",
                "mentions @topic:all-rules directly in its own words",
            )
        },
    )
    .await;

    let sid = writing_as(&jojobot);
    let renamed = jojobot
        .rename_entity(Parameters(RenameEntityArgs {
            handle: "topic:all-rules".into(),
            to: "topic:the-five-words".into(),
            parent: None,
            recorded_at: None,
            override_token: None,
            sid: Some(sid),
        }))
        .await
        .expect("rename_entity call ok");
    assert_ne!(
        json_of(&renamed)["status"],
        "blocked",
        "{}",
        text_of(&renamed)
    );

    let walked = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("topic:the-five-words".into()),
                follow: Some(FollowArgs {
                    direction: Some("in".into()),
                    ..no_follow()
                }),
                ..of_nothing()
            }))
            .await
            .expect("recall ok"),
    );
    let reached = walked["objects"][0]["connected"]
        .as_array()
        .expect("connected objects");
    assert_eq!(
        reached.iter().map(|o| o["id"].clone()).collect::<Vec<_>>(),
        vec!["person:gayle"],
        "the mention still resolves through the rename: {walked}"
    );
    assert_eq!(reached[0]["via"]["mention"], true, "{walked}");
}

/// 🚨 **Discoverability: the verb's own description names the
/// argument.** A capability whose only path is that somebody read the
/// diff has no path.
#[test]
fn stood_for_is_named_on_the_verbs_own_description() {
    let tools = Jojobot::tool_router().list_all();
    let recall = tools
        .iter()
        .find(|t| t.name.as_ref() == "recall")
        .expect("recall is a tool");
    let tool_description = recall.description.as_deref().unwrap_or_default();
    assert!(
        tool_description.contains("stood_for"),
        "the tool-level description does not name the argument: {tool_description}"
    );
    let schema = serde_json::to_value(&recall.input_schema).expect("the schema serializes");
    assert!(
        schema["properties"]["stood_for"]["description"]
            .as_str()
            .is_some_and(|d| !d.is_empty()),
        "stood_for carries no schema description of its own: {schema}"
    );
}

/// **A session's chronology is recalled, and capture onto its handle
/// stays refused — the read half and the protecting half proved in one
/// case.**
///
/// The read half is new: `session::projected` builds the object, but
/// nothing in `graph::walk` reads it yet, so this fails at the recall
/// step until that wiring lands. The protecting half is not new — `capture`
/// already refuses a session-kind subject through `validate_write_subject`
/// — and this is what proves that refusal keeps holding once the same
/// session is a reachable object rather than an invisible one.
#[tokio::test]
async fn a_sessions_chronology_is_recalled_and_capture_onto_it_stays_refused() {
    let jojobot = crate::harness::handler();
    let sid = writing_as(&jojobot);
    let receipt = journal_entry(
        &jojobot,
        &sid,
        "traced the boot slowdown to a synchronous reindex on every capture",
    )
    .await;
    let session_id = receipt["session"]
        .as_str()
        .expect("journal answers with the session it landed in")
        .to_string();
    let handle = format!("session:{session_id}");

    let found = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                sid: Some(sid.clone()),
                ..of(&handle)
            }))
            .await
            .expect("recall answers rather than failing the protocol"),
    )
    .to_string();
    assert!(
        found.contains("traced the boot slowdown"),
        "a session's own chronology did not come back through the ordinary lookup: {found}",
    );

    let refused = blocked(
        &jojobot
            .capture(Parameters(CaptureArgs {
                sid: Some(sid),
                ..capture_args(
                    &handle,
                    "a claim about the run, not written through journal",
                )
            }))
            .await
            .expect("capture answers rather than failing the protocol"),
    )
    .to_string();
    assert!(
        refused.contains("session") && refused.contains("journal"),
        "a session reachable by recall must still refuse an ordinary write onto its \
             handle, and name the way forward: {refused}",
    );
}

/// **A session answers its own bot alone.** `recall` fetches every run
/// on the board (`all_summaries`, cheap — no beat's text leaves the
/// store to build the entity index), so a second bot naming the first
/// bot's session DOES reach it in the index; `graph::walk`'s owner check
/// (`session::projected_summary` sets `owner`, proven on this exact
/// shape at `19fca07`) is what refuses it, as somebody else's, never as
/// absent — the same distinction an ordinary owned entity already gets.
#[tokio::test]
async fn a_bots_session_is_not_readable_by_a_different_bot() {
    let jojobot = crate::harness::handler();
    let sid = writing_as(&jojobot);
    let receipt = journal_entry(&jojobot, &sid, "a beat that belongs to bot:otto alone").await;
    let session_id = receipt["session"]
        .as_str()
        .expect("journal answers with the session it landed in")
        .to_string();
    let handle = format!("session:{session_id}");

    let other_sid = jojobot
        .registry
        .mint(&EntityId("bot:milhouse".into()), None)
        .expect("a free handle in a fresh registry")
        .to_string();

    let refused = blocked(
        &jojobot
            .recall(Parameters(RecallArgs {
                sid: Some(other_sid),
                ..of(&handle)
            }))
            .await
            .expect("recall answers rather than failing the protocol"),
    );
    assert_eq!(refused["attempted"], handle, "{refused}");
    let how = refused["how_to_proceed"]
        .as_str()
        .expect("a blocked answer says how to proceed");
    // **The refusal must say the object is another's, never that it does
    // not exist** — the pairing this bar exists for: a build that
    // regressed to "unknown" would still be `status: blocked` and would
    // still pass a check that stopped at that.
    assert!(
        how.contains("not yours"),
        "a different bot's run must be refused as somebody else's, never as absent: {how}",
    );
    // It says the run is not the caller's and does not say whose it is.
    assert!(
        !how.contains("bot:otto"),
        "the refusal named the bot that owns the run: {how}",
    );
    assert!(
        !refused
            .to_string()
            .contains("a beat that belongs to bot:otto alone"),
        "a different bot's run leaked into a caller that does not own it: {refused}",
    );
}

/// **Browsing runs by kind returns only the caller's own, and the rest
/// is counted — a total, never a breakdown — rather than vanishing.**
/// 🚨 **Both halves in one read, the pairing the bar demands**: a caller
/// finds its own run AND the other bot's is `withheld` rather than
/// silently absent, so this cannot pass against a build where nothing
/// is indexed at all. The caller's own also reads whole — prose
/// included — proving selectable and readable land together.
#[tokio::test]
async fn browsing_runs_by_kind_finds_only_the_callers_own_and_counts_the_rest() {
    let jojobot = crate::harness::handler();
    let sid = writing_as(&jojobot);
    let receipt = journal_entry(&jojobot, &sid, "otto's own beat, browsed by kind").await;
    let session_id = receipt["session"]
        .as_str()
        .expect("journal answers with the session it landed in")
        .to_string();
    let handle = format!("session:{session_id}");

    let other_sid = jojobot
        .registry
        .mint(&EntityId("bot:milhouse".into()), None)
        .expect("a free handle in a fresh registry")
        .to_string();
    journal_entry(
        &jojobot,
        &other_sid,
        "milhouse's own beat, never otto's to read",
    )
    .await;

    let answer = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                subject: None,
                kind: Some("session".into()),
                prose: Some(true),
                sid: Some(sid),
                ..of("unused")
            }))
            .await
            .expect("recall answers rather than failing the protocol"),
    );
    assert_ne!(answer["status"], "blocked", "{answer}");
    let objects = answer["objects"]
        .as_array()
        .expect("a kind browse answers with a list");
    assert_eq!(
        objects.len(),
        1,
        "a bot must find only its own past run among both: {answer}"
    );
    assert_eq!(objects[0]["id"], handle, "{answer}");
    assert_eq!(
        objects[0]["prose"], "otto's own beat, browsed by kind",
        "the caller's own run must read whole, not merely be found: {answer}"
    );
    assert_eq!(
        answer["withheld"], 1,
        "the other bot's run must be counted as withheld, not dropped in silence: {answer}"
    );
    assert!(
        !answer.to_string().contains("milhouse's own beat"),
        "the other bot's content must never leak into a browse that withheld it: {answer}"
    );
}

/// A scratch directory this test owns alone, removed when it is done —
/// the same shape `boundary`'s own fixture uses, local here because this
/// is the one case in this file needing a real store rather than the
/// fake.
struct LazinessScratch(std::path::PathBuf);

impl LazinessScratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "jojobot-mcp-recall-laziness-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("a clock after 1970")
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        LazinessScratch(path)
    }
}

impl Drop for LazinessScratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 🚨 **Proves the lazy path is lazy, quantitatively, against a real
/// store — the same counter that already proves `summaries_of` avoids
/// the full read, copied to prove this one does too.** Five of the
/// caller's own runs exist; naming ONE by handle must cost exactly one
/// `full_reads` — never five, which is what the old
/// `sessions_of(&caller.bot)` fetch cost on every single call regardless
/// of which run, if any, the query actually wanted.
#[tokio::test]
async fn recall_reads_only_the_wanted_runs_chronology_not_every_ones() {
    let scratch = LazinessScratch::new();
    let mut store =
        jojobot_adapters::dolt::Dolt::start(&scratch.0, jojobot_adapters::testing::free_port())
            .await
            .expect("the real store comes up");
    jojobot_adapters::dolt::migrate::run(store.pool())
        .await
        .expect("the schema");
    jojobot_adapters::dolt::migrate::seed_kinds(store.pool())
        .await
        .expect("the kinds are seeded");
    let sessions = std::sync::Arc::new(jojobot_adapters::dolt::sessions::DoltSessions::open(
        store.pool().clone(),
    ));

    let jojobot = Jojobot::new(
        Arc::new(InMemoryMemory::booted()),
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        sessions.clone() as Arc<dyn jojobot_domain::session::Sessions>,
        Arc::new(InMemoryTeachings::new()),
        seeded_registry(),
    );
    // **Five DIFFERENT runs, not five beats on one** — `writing_as` is
    // deliberately idempotent (one handle, one run), so each run here
    // mints its own fresh handle the same way a fresh boot would.
    let mut wanted_handle = String::new();
    let mut last_sid = String::new();
    for n in 0..5 {
        let sid = jojobot
            .registry
            .mint(&EntityId("bot:otto".into()), None)
            .expect("a free handle in a fresh registry")
            .to_string();
        let receipt = journal_entry(&jojobot, &sid, &format!("run {n}'s own beat")).await;
        if n == 2 {
            wanted_handle = format!(
                "session:{}",
                receipt["session"]
                    .as_str()
                    .expect("journal answers with the session it landed in")
            );
        }
        last_sid = sid;
    }
    let sid = last_sid;

    let before = sessions.full_reads();
    let answer = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                sid: Some(sid),
                prose: Some(true),
                ..of(&wanted_handle)
            }))
            .await
            .expect("recall answers rather than failing the protocol"),
    );
    assert_ne!(answer["status"], "blocked", "{answer}");
    assert_eq!(
        answer["objects"][0]["prose"], "run 2's own beat",
        "the named run must still read whole: {answer}"
    );
    assert_eq!(
        sessions.full_reads() - before,
        1,
        "naming one of five runs must cost exactly one full read, not one per run in the \
             index"
    );

    store.stop().await;
}

/// **A bot reading its own room, adding nothing, still learns what aged
/// out of it.** Ageing's own refusal already says how many aged out;
/// this is the other half — an ordinary `recall`, no write at all, over
/// a room already at capacity with one thought aged past the cutoff.
/// The answer must carry the room's own count AND let a caller pick the
/// aged thought out of the facts already in front of it, since nothing
/// here is a new verb: the pointer rides the answer that already
/// exists.
#[tokio::test]
async fn a_bots_room_read_says_what_aged_out_without_a_write() {
    let sessions = Arc::new(jojobot_domain::session::testing::InMemorySessions::new());
    let jojobot = Jojobot::new(
        Arc::new(InMemoryMemory::booted()),
        Arc::new(SpySearch::default()),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        sessions.clone(),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    );
    let bot = "bot:mcp-thought-aging";
    ensure(&jojobot, bot).await;
    capture_ok(
        &jojobot,
        CaptureArgs {
            fields: Some(
                [(
                    jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                    "1".to_string(),
                )]
                .into_iter()
                .collect(),
            ),
            ..capture_args(bot, "capacity is one")
        },
    )
    .await;

    let old = capture_ok(
        &jojobot,
        CaptureArgs {
            shape: Some("connection".into()),
            object: Some("thing:the-couch".into()),
            ..capture_args(bot, "the couch needs a leg fixed")
        },
    )
    .await;
    let old_address = address_of(&old);

    for n in 0..20 {
        sessions
            .begin(jojobot_domain::session::NewSession {
                bot: EntityId(bot.to_string()),
                sid: jojobot_domain::session::Sid(format!("rr{n:02}")),
                focus: "working".into(),
                started_at: jiff::Timestamp::now(),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("seeding a run");
    }

    // Nothing captured since the runs — a plain read, adding nothing.
    let recalled = json_of(
        &jojobot
            .recall(Parameters(recall_args(bot)))
            .await
            .expect("recall ok"),
    );
    let object = &recalled["objects"][0];
    assert_eq!(object["room"]["capacity"], 1, "{object}");
    assert_eq!(
        object["room"]["live"], 0,
        "the only thought in the room aged out, so live must read zero: {object}"
    );
    assert_eq!(
        object["room"]["aged_out"], 1,
        "a read that adds nothing must still say one thought aged out: {object}"
    );

    let facts = object["facts"].as_array().expect("facts asked for");
    let old_fact = facts
        .iter()
        .find(|f| f["address"] == old_address)
        .expect("the aged thought is still an ordinary fact on the answer");
    assert_eq!(
        old_fact["aged_out"], true,
        "the aged thought must be pickable out of the facts already returned, with no \
             second call: {old_fact}"
    );
    assert_eq!(
        old_fact["status"], "active",
        "ageing must not archive a row a plain read passes over: {old_fact}"
    );
}

/// 🚨 **A caller with no identity is told how many runs exist and nothing
/// else.** A run's entity is named for its focus text and owned by a bot, and
/// the walk's near-miss screen and its not-yours branch both say so. Feeding
/// the runs into the walk for a caller that owns none let a near miss of a
/// run's id return the focus text of somebody else's run, a subject equal to
/// the slugified focus confirm it, and an exact id name the owner.
///
/// Paired with the count itself, which is still the number of runs that exist:
/// an answer that said nothing at all would pass every negative below.
#[tokio::test]
async fn a_caller_with_no_identity_is_told_how_many_runs_exist_and_nothing_else() {
    const FOCUS: &str = "pricing the replacement wheels";
    let jojobot = handler();
    let sid = writing_as(&jojobot);
    jojobot
        .journal(Parameters(crate::session::JournalArgs {
            entry: "started on the wheels".into(),
            focus: Some(FOCUS.into()),
            sid: sid.clone(),
        }))
        .await
        .expect("journal ok");

    let runs_query = |sid: Option<String>, subject: Option<String>| RecallArgs {
        subject,
        kind: Some("session".into()),
        sid,
        ..recall_args("person:bart")
    };
    // What an identified caller sees: its own run shown, the rest counted.
    let identified = json_of(
        &jojobot
            .recall(Parameters(runs_query(Some(sid.clone()), None)))
            .await
            .expect("recall ok"),
    );
    let own = identified["objects"].as_array().expect("a list").len() as u64;
    assert_eq!(own, 1, "the run exists and is the caller's: {identified}");
    let run_id = identified["objects"][0]["id"]
        .as_str()
        .expect("a run carries its id")
        .to_string();
    let everything = own + identified["withheld"].as_u64().expect("a count");

    let text_of_answer = |answered: Result<CallToolResult, McpError>| match answered {
        Ok(result) => text_of(&result),
        Err(error) => error.message.to_string(),
    };

    // The count: every run that exists, shown none.
    let anonymous = json_of(
        &jojobot
            .recall(Parameters(runs_query(None, None)))
            .await
            .expect("recall ok"),
    );
    assert!(
        anonymous["objects"].as_array().expect("a list").is_empty(),
        "a caller with no identity owns no run: {anonymous}"
    );
    assert_eq!(
        anonymous["withheld"].as_u64(),
        Some(everything),
        "withheld is the number of runs that exist: {anonymous}"
    );

    // Every way of naming a run: a near miss of its id, the slug of its focus,
    // and its exact id.
    let mut near_miss = run_id.clone();
    let last = near_miss.pop().expect("an id is not empty");
    near_miss.push(if last == 'a' { 'b' } else { 'a' });
    let by_focus = format!("session:{}", jojobot_domain::memory::guard::slugify(FOCUS));
    for subject in [near_miss, by_focus, run_id] {
        let text = text_of_answer(
            jojobot
                .recall(Parameters(runs_query(None, Some(subject.clone()))))
                .await,
        );
        assert!(!text.is_empty(), "{subject} got no answer at all");
        // The caller's own words come back to it in the answer, and a subject
        // spelled from the focus carries the focus. That is an echo, not a leak.
        let text = text.replace(subject.as_str(), "");
        assert!(
            !text.contains("wheels"),
            "naming {subject} gave a caller with no identity a run's focus text: {text}"
        );
        assert!(
            !text.contains("bot:otto"),
            "naming {subject} gave a caller with no identity a run's owner: {text}"
        );
    }
}
