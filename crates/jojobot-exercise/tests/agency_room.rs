//! **The agency room, held for free.**
//!
//! Nothing here drives a model. A synthetic occupant plays the four sittings
//! through the verbs, on each sitting's own day, and the run's own boundaries
//! are taken between them, so every lock is read the way a run reads it.
//!
//! **Two cases for every lock.** The solvability case: an occupant that writes
//! each fact where its reader looks holds every lock of the sitting. The
//! discrimination case: the same occupant writes ONE fact the plausible wrong
//! way, and that lock reds and no other. The room is a probe and is not built to
//! be won, but a harness that cannot be passed is not a probe of the product.

use jojobot_exercise::expectations::{self, AGENCY_ROOM};
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{
    Boundary, Expectation, Observed, Outcome, boundary, boundary_asking, boundary_names,
    phase_end_queries,
};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};

fn document() -> Playbook {
    Playbook::read(&expectations::room_document(AGENCY_ROOM))
        .unwrap_or_else(|e| panic!("the shipped room must read: {e:#}"))
}

/// A room as a run furnishes it, the locks the room asserts, and the readings a
/// run takes as it goes.
struct Lab {
    _room: Room,
    surface: Surface,
    locks: Vec<Box<dyn Expectation>>,
    boundaries: Vec<Boundary>,
    names: Vec<String>,
}

async fn lab() -> Lab {
    let (room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    expectations::seed_for(AGENCY_ROOM)
        .expect("the room has furniture")
        .furnish(&surface)
        .await
        .expect("the room is furnished");
    let names = boundary_names(&document());
    let boundaries = vec![boundary(&surface, &names[0]).await];
    Lab {
        _room: room,
        surface,
        locks: expectations::for_playbook(AGENCY_ROOM).expect("the room asserts"),
        boundaries,
        names,
    }
}

impl Lab {
    /// **A session for one sitting, in the day that sitting is in.**
    async fn sitting(&self, day: &str) -> String {
        let booted = self
            .surface
            .must(
                "start_here",
                json!({"bot": "assistant", "brief": true, "today": day}),
            )
            .await
            .expect("the shipped identity boots");
        if let Some(sid) = booted["session"]["sid"].as_str() {
            return sid.to_string();
        }
        let answered = self
            .surface
            .must(
                "start_here",
                json!({"bot": "assistant", "brief": true, "today": day, "resume": "new"}),
            )
            .await
            .expect("the boot answering the choice is ok");
        answered["session"]["sid"]
            .as_str()
            .unwrap_or_else(|| panic!("no handle on {day}: {answered}"))
            .to_string()
    }

    /// **What a run records when a sitting ends.** The locks of that sitting
    /// that ask at its end are sent now, and the answers kept on the boundary.
    async fn ends(&mut self, phase_key: &str, at: usize) {
        let queries = phase_end_queries(&self.locks, phase_key);
        let taken = boundary_asking(&self.surface, &self.names[at], &queries).await;
        self.boundaries.push(taken);
    }

    async fn call(&self, sid: &str, verb: &str, mut args: Value) -> String {
        args["sid"] = json!(sid);
        self.surface.call(verb, args).await
    }

    /// A claim written as the occupant would write it. The address comes back.
    async fn say(&self, sid: &str, subject: &str, content: &str, extra: Value) -> String {
        let mut args = json!({"subject": subject, "content": content, "provenance": "testimony"});
        for (key, value) in extra.as_object().expect("an object") {
            args[key] = value.clone();
        }
        let said = self.call(sid, "capture", args).await;
        assert!(
            !said.contains("\"status\":\"blocked\"") && !said.contains("\"error\""),
            "the sitting's own write was refused: {said}"
        );
        serde_json::from_str::<Value>(&said).expect("a receipt")["address"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    /// The address of the claim on `subject` whose words hold `needle`.
    async fn address_of(&self, sid: &str, subject: &str, needle: &str) -> String {
        let read = self
            .call(sid, "recall", json!({"subject": subject, "facts": true}))
            .await;
        let parsed: Value = serde_json::from_str(&read).expect("json");
        parsed["objects"][0]["facts"]
            .as_array()
            .and_then(|facts| {
                facts
                    .iter()
                    .find(|f| f["content"].as_str().is_some_and(|c| c.contains(needle)))
            })
            .and_then(|f| f["address"].as_str())
            .unwrap_or_else(|| panic!("no claim on {subject} holds {needle}: {read}"))
            .to_string()
    }

    /// A claim rewritten in place: its words and its one link.
    async fn rewrite(&self, sid: &str, address: &str, content: &str, about: &str) {
        let said = self
            .call(
                sid,
                "update_fact",
                json!({"address": address, "content": content, "provenance": "testimony",
                       "shape": "about", "object": about}),
            )
            .await;
        assert!(!said.contains("blocked"), "{said}");
    }

    async fn add(&self, sid: &str, kind: &str, handle: &str, name: &str, parent: Option<&str>) {
        let mut args =
            json!({"kind": kind, "handle": handle, "name": name, "source": "user-named"});
        if let Some(parent) = parent {
            args["parent"] = json!(parent);
        }
        let said = self.call(sid, "add_entity", args).await;
        assert!(!said.contains("blocked"), "{said}");
    }

    async fn judge(&self) -> Vec<Outcome> {
        let seen = Observed {
            room: &self.surface,
            boundaries: &self.boundaries,
        };
        let mut outcomes = Vec::new();
        for check in &self.locks {
            outcomes.push(check.check(&seen).await);
        }
        outcomes
    }
}

fn saying(outcomes: &[Outcome]) -> String {
    outcomes
        .iter()
        .enumerate()
        .map(|(at, o)| format!("\n  {at:>2} [{}] {}", o.held, o.saying))
        .collect()
}

/// The campaign and the app, as the occupant files them. The handles are on the
/// roster and the names say what each is.
const CAMPAIGN: &str = "project:atlas";
const APP: &str = "project:visa";
/// The app's ordering phase, as the occupant files it. The handle is on the roster.
const ORDERING: &str = "work:red-bike";

/// **Which lock of a sitting a case writes the wrong way**, by its place in the
/// sitting. `None` is the occupant that writes every fact where its reader looks.
type Wrong = Option<usize>;

fn is(wrong: Wrong, at: usize) -> bool {
    wrong == Some(at)
}

/// **January.** Thirteen locks, in the order the room lists them.
async fn january(lab: &mut Lab, wrong: Wrong) {
    let sid = lab.sitting("2026-01-15").await;
    lab.add(&sid, "project", "atlas", "The Krusty Burger Campaign", None)
        .await;
    lab.add(&sid, "project", "visa", "The Ordering App", None)
        .await;
    // The five dates. A date the lock asks for is under a key unless this case
    // writes it only into a sentence.
    let dates = [
        ("launch", "2026-10-12"),
        ("copy_due", "2026-03-01"),
        ("design_due", "2026-04-15"),
        ("production_due", "2026-07-01"),
        ("media_due", "2026-08-15"),
    ];
    let mut fields = serde_json::Map::new();
    for (at, (key, day)) in dates.iter().enumerate() {
        if !is(wrong, at) {
            fields.insert((*key).to_string(), json!(day));
        }
    }
    lab.say(
        &sid,
        CAMPAIGN,
        "Launch on 12 October and it never moves. Copy by 1 March, design by 15 April, \
         production by 1 July, media booked by 15 August. One market, Alpha.",
        json!({"fields": fields}),
    )
    .await;
    // The contact report that falls due tomorrow.
    if is(wrong, 5) {
        lab.say(
            &sid,
            CAMPAIGN,
            "Contact report to Krusty tomorrow.",
            json!({}),
        )
        .await;
    } else {
        lab.add(&sid, "person", "ned-flanders", "Ned Flanders", None)
            .await;
        lab.add(
            &sid,
            "promise",
            "return-the-desk",
            "Contact report to Krusty",
            Some("person:ned-flanders"),
        )
        .await;
        lab.say(
            &sid,
            "promise:return-the-desk",
            "Send Krusty a contact report.",
            json!({"fields": {"promised_by": "2026-01-16"}}),
        )
        .await;
    }
    // The budget is hearsay: open, not settled.
    let standing = if is(wrong, 6) { "settled" } else { "open" };
    lab.say(
        &sid,
        CAMPAIGN,
        "Krusty says Burns approved about 200k for the campaign. Nothing is in writing.",
        json!({"standing": standing}),
    )
    .await;
    let head = if is(wrong, 7) {
        "Homer decided: we sync the menu first, an order for a menu we have not yet synchronised is an \
         order for nothing."
    } else {
        "Homer decided: \"we sync the menu first: an order for a menu we have not synced is an \
         order for nothing\"."
    };
    lab.say(&sid, APP, head, json!({})).await;
    let done = if is(wrong, 8) {
        "The spot is done when Lisa has approved it, legal has cleared it, and it has gone to the \
         stations."
    } else {
        "The spot is done when Lisa has approved it, legal has cleared it, and it has gone to \
         the stations in three formats."
    };
    lab.say(&sid, CAMPAIGN, done, json!({})).await;
    let legal = if is(wrong, 9) {
        "Legal needs lead time for any product claim."
    } else {
        "Legal needs six weeks for any product claim."
    };
    lab.say(&sid, CAMPAIGN, legal, json!({})).await;
    let turned = if is(wrong, 10) {
        "Martin's option was turned down."
    } else {
        "Martin's option, ordering first on a hard-coded menu, was turned down."
    };
    lab.say(&sid, APP, turned, json!({})).await;
    let question = if is(wrong, 11) {
        "Drafted and not asked yet: does ordering need a special mode? It blocks ordering."
    } else {
        "Drafted and not asked yet: does ordering need an offline mode? It blocks ordering."
    };
    lab.say(&sid, APP, question, json!({})).await;
    // The ordering phase is a thing of its own, and the question Krusty asked
    // about it is kept where the phase reaches it. Written the wrong way, the
    // question sits on the app with nothing joining it to the phase.
    lab.add(&sid, "work", "red-bike", "The ordering phase", Some(APP))
        .await;
    let host = if is(wrong, 12) { APP } else { ORDERING };
    lab.say(
        &sid,
        host,
        "Krusty asked whether the ordering screen can split one bill between guests. Nobody has \
         answered yet.",
        json!({}),
    )
    .await;
    lab.ends("Phase 1", 1).await;
}

/// **May.** Eleven locks.
async fn may(lab: &mut Lab, wrong: Wrong) {
    let sid = lab.sitting("2026-05-20").await;
    let mut fields = serde_json::Map::new();
    if !is(wrong, 0) {
        fields.insert("copy_landed".into(), json!("2026-03-15"));
    }
    if !is(wrong, 1) {
        fields.insert("design_due".into(), json!("2026-05-06"));
    }
    lab.say(
        &sid,
        CAMPAIGN,
        "Copy landed on 15 March, two weeks late. Design is now due 6 May.",
        json!({"fields": fields}),
    )
    .await;
    let pulled = if is(wrong, 2) {
        "Milhouse was pulled over to another team from 6 to 26 April, and the design waited."
    } else {
        "Milhouse was pulled over to Moe's Tavern from 6 to 26 April."
    };
    lab.say(&sid, CAMPAIGN, pulled, json!({})).await;
    let markets = if is(wrong, 3) {
        "Krusty added two markets on the phone."
    } else {
        "Krusty added two markets, Beta and Gamma, on the phone and nothing is written down."
    };
    lab.say(&sid, CAMPAIGN, markets, json!({})).await;
    let disclaimer = if is(wrong, 4) {
        "Beta needs something Lisa has to clear."
    } else {
        "Beta needs a local-language disclaimer that Lisa has to clear."
    };
    lab.say(&sid, CAMPAIGN, disclaimer, json!({})).await;
    let landed = if is(wrong, 5) {
        "Login landed as a commit."
    } else {
        "Login landed as commit a1b2c3d."
    };
    lab.say(&sid, APP, landed, json!({})).await;
    let suite = if is(wrong, 6) {
        "A test suite verified login."
    } else {
        "The LoginFlowSuite verified login."
    };
    lab.say(&sid, APP, suite, json!({})).await;
    let quote = if is(wrong, 7) {
        "Reception at the basement location is bad."
    } else {
        "Krusty said \"the basement location has terrible reception\"."
    };
    let quote_at = lab.say(&sid, APP, quote, json!({})).await;
    let mut derived = json!({"provenance": "inference"});
    if !is(wrong, 8) {
        derived["derived_from"] = json!(quote_at);
    }
    lab.say(
        &sid,
        APP,
        "Orders must work offline, because of the basement reception.",
        derived,
    )
    .await;
    // Martin answers the question January left on the ordering phase. The wrong
    // ways: the answer is filed against Krusty, or it is written over the
    // question it answers.
    let answer = "Martin answered that splitting a bill needs the payments phase, so it cannot ship \
                  with ordering and moves after payments.";
    let who = if is(wrong, 9) {
        "person:krusty"
    } else {
        "person:martin"
    };
    if is(wrong, 10) {
        let asked = lab.address_of(&sid, ORDERING, "ordering screen").await;
        lab.rewrite(&sid, &asked, answer, who).await;
    } else {
        lab.say(
            &sid,
            ORDERING,
            answer,
            json!({"shape": "about", "object": who}),
        )
        .await;
    }
    lab.ends("Phase 2", 2).await;
}

/// **September.** Thirteen locks.
async fn september(lab: &mut Lab, wrong: Wrong) {
    let sid = lab.sitting("2026-09-08").await;
    let budget = if is(wrong, 0) {
        "Burns says the budget is lower and he never approved it."
    } else {
        "Burns says the budget is 150k and he never approved 200k."
    };
    lab.say(&sid, CAMPAIGN, budget, json!({})).await;
    let three = if is(wrong, 1) {
        "Lisa approved a draft of the film on 28 August, on condition that the legal claim clears."
    } else {
        "Lisa approved draft 3 of the film on 28 August, on condition that the legal claim clears."
    };
    lab.say(&sid, CAMPAIGN, three, json!({})).await;
    let four = if is(wrong, 2) {
        "A newer draft exists and Krusty prefers it. Nobody has approved it."
    } else {
        "Draft 4 has existed since 2 September, Krusty prefers it, nobody has approved it."
    };
    lab.say(&sid, CAMPAIGN, four, json!({})).await;
    let mut dropped = json!({});
    if !is(wrong, 3) {
        dropped["happened_at"] = json!("2026-07-22");
    }
    lab.say(
        &sid,
        CAMPAIGN,
        "The print ads were dropped at a meeting on 22 July and never written down.",
        dropped,
    )
    .await;
    let mut fields = serde_json::Map::new();
    for (at, (key, day)) in [
        (4, ("print_cancel_by", "2026-09-30")),
        (5, ("production_due", "2026-09-21")),
        (6, ("stations_deadline", "2026-09-25")),
    ] {
        if !is(wrong, at) {
            fields.insert(key.to_string(), json!(day));
        }
    }
    lab.say(
        &sid,
        CAMPAIGN,
        "Print inventory has a cancellation deadline of 30 September. Production slipped to \
         21 September. The stations need materials by 25 September.",
        json!({"fields": fields}),
    )
    .await;
    // The requirement is taken back. The claim is the one May wrote. The wrong
    // way takes back the quote it was worked out from instead.
    let target = if is(wrong, 7) {
        "terrible reception"
    } else {
        "must work offline"
    };
    let read = lab
        .call(&sid, "recall", json!({"subject": APP, "facts": true}))
        .await;
    let parsed: Value = serde_json::from_str(&read).expect("json");
    let address = parsed["objects"][0]["facts"]
        .as_array()
        .and_then(|facts| {
            facts
                .iter()
                .find(|f| f["content"].as_str().is_some_and(|c| c.contains(target)))
        })
        .and_then(|f| f["address"].as_str())
        .unwrap_or_else(|| panic!("May's claim is on the app: {read}"))
        .to_string();
    let said = lab
        .call(
            &sid,
            "update_fact",
            json!({"address": address, "status": "archived",
                   "details": "the quote was misread, so the requirement is withdrawn"}),
        )
        .await;
    assert!(!said.contains("blocked"), "{said}");
    let provider = if is(wrong, 8) {
        "A second question is drafted and not asked yet. It blocks payments."
    } else {
        "A second question is drafted and not asked yet: which payment provider. It blocks payments."
    };
    lab.say(&sid, APP, provider, json!({})).await;
    let synced = if is(wrong, 9) {
        "Menu sync landed."
    } else {
        "Menu sync landed as commit c0ffee1."
    };
    lab.say(&sid, APP, synced, json!({})).await;
    let waiting = if is(wrong, 10) {
        "Ordering now waits on Homer."
    } else {
        "Ordering now waits on Homer's sign-off."
    };
    lab.say(&sid, APP, waiting, json!({})).await;
    // Krusty closes the question. The wrong ways: the closing word is filed
    // against Martin, or it is written over May's answer.
    let closing = "Krusty said staff split bills at the till, so the app does not need to split \
                   bills. The question is closed as dropped.";
    let closer = if is(wrong, 11) {
        "person:martin"
    } else {
        "person:krusty"
    };
    if is(wrong, 12) {
        let answered = lab.address_of(&sid, ORDERING, "after payments").await;
        lab.rewrite(&sid, &answered, closing, closer).await;
    } else {
        lab.say(
            &sid,
            ORDERING,
            closing,
            json!({"shape": "about", "object": closer}),
        )
        .await;
    }
    lab.ends("Phase 3", 3).await;
}

/// **December.** Sixteen locks: the budget, five owner pairings, and ten slots.
async fn december(lab: &mut Lab, wrong: Wrong) {
    let sid = lab.sitting("2026-12-10").await;
    lab.say(
        &sid,
        CAMPAIGN,
        "The answer to budget_truth.",
        json!({"fields": {"budget_truth": if is(wrong, 0) { "200" } else { "150" }}}),
    )
    .await;
    // Who owns what. Each deliverable is a thing of its own that holds the
    // owner's handle as a value. Written the wrong way, the owner is a name in
    // the value and not a handle.
    for (handle, name) in [
        ("handcart", "The TV spot"),
        ("first-mix", "The print ads"),
        ("phi", "The social posts"),
        ("sigma", "The launch event"),
    ] {
        lab.add(&sid, "work", handle, name, Some(CAMPAIGN)).await;
    }
    let owner = |at: usize, handle: &'static str, name: &'static str| {
        if is(wrong, at) { name } else { handle }
    };
    lab.say(
        &sid,
        "work:handcart",
        "The film, owned by Milhouse with the copy by Nelson.",
        json!({"fields": {"owners": owner(1, "person:milhouse", "Milhouse"),
                          "copy_by": owner(2, "person:nelson", "Nelson")}}),
    )
    .await;
    lab.say(
        &sid,
        "work:first-mix",
        "Print ads, owned by Milhouse.",
        json!({"fields": {"owners": owner(3, "person:milhouse", "Milhouse")}}),
    )
    .await;
    lab.say(
        &sid,
        "work:phi",
        "Social, owned by Nelson.",
        json!({"fields": {"owners": owner(4, "person:nelson", "Nelson")}}),
    )
    .await;
    if is(wrong, 5) {
        // Written against the project, which is not the thing about the event.
        lab.say(
            &sid,
            CAMPAIGN,
            "The launch event is owned by Ralph.",
            json!({"fields": {"owners": "person:ralph"}}),
        )
        .await;
        // Ralph's print booking, filed under a key that names the inventory and
        // holds his handle. It is not the event, and a needle that is a
        // fragment of the word `event` can match the word `inventory`.
        lab.say(
            &sid,
            "work:first-mix",
            "Ralph booked the print inventory.",
            json!({"fields": {"print_inventory": "person:ralph"}}),
        )
        .await;
    } else {
        lab.say(
            &sid,
            "work:sigma",
            "The launch event, owned by Ralph.",
            json!({"fields": {"owners": "person:ralph"}}),
        )
        .await;
    }
    // The rest. Each slot: where it is written, its key, the right value, and
    // the value a plausible wrong answer gives.
    let slots: [(&str, &str, &str, &str); 9] = [
        (CAMPAIGN, "approval", "3", "4"),
        (CAMPAIGN, "spot_done", "no", "yes"),
        (CAMPAIGN, "slips", "3", "2"),
        (
            CAMPAIGN,
            "undelivered",
            "the Beta disclaimer, the third station format, and the print ads",
            "the Beta disclaimer and the third station format",
        ),
        (
            APP,
            "app_next",
            "ordering is next, waiting on Homer's sign-off; payments is blocked behind it and the \
             provider question",
            "ordering is next, waiting on Homer's sign-off",
        ),
        (
            APP,
            "menu_first",
            "Homer: \"we sync the menu first: an order for a menu we have not synced is an order \
             for nothing\"; Martin's hard-coded menu option was turned down",
            "Homer: \"we sync the menu first: an order for a menu we have not synced is an order \
             for nothing\"",
        ),
        (
            APP,
            "rests_on_misread",
            "the offline requirement and the offline half of ordering",
            "the payment plan",
        ),
        (
            APP,
            "never_asked",
            "the payment provider question",
            "the offline question",
        ),
        (
            APP,
            "app_landed",
            "login a1b2c3d, verified by the LoginFlowSuite; menu sync c0ffee1",
            "login a1b2c3d, verified by the LoginFlowSuite",
        ),
    ];
    for (at, (subject, key, right, other)) in slots.iter().enumerate() {
        let value = if is(wrong, at + 6) { other } else { right };
        lab.say(
            &sid,
            subject,
            &format!("The answer to {key}."),
            json!({"fields": {*key: value}}),
        )
        .await;
    }
    // The discussion's conclusion, who closed it and in which month. The wrong
    // way names the state it was in before it was closed.
    let split = if is(wrong, 15) {
        "moved after payments, as Martin answered in May"
    } else {
        "dropped, closed by Krusty in September"
    };
    lab.say(
        &sid,
        APP,
        "The answer to split_bill.",
        json!({"fields": {"split_bill": split}}),
    )
    .await;
    lab.ends("Phase 4", 4).await;
}

/// Which locks of the sitting ending at `phase` held, in the room's order, and
/// what they said. Only that sitting's own locks are read.
fn of_sitting(outcomes: &[Outcome], phase: &str) -> Vec<(bool, String)> {
    outcomes
        .iter()
        .filter(|o| o.name.starts_with(phase))
        .map(|o| (o.held, o.saying.clone()))
        .collect()
}

/// How many locks the room puts in each sitting.
const LOCKS: [usize; 4] = [13, 11, 13, 16];

fn all_but(count: usize, red: Wrong) -> Vec<bool> {
    (0..count).map(|at| red != Some(at)).collect()
}

/// **The sittings played in order, up to and including `through`, the last one
/// with one fact written the wrong way.**
async fn played(through: usize, wrong: Wrong) -> Lab {
    let mut lab = lab().await;
    for at in 0..=through {
        let wrong_here = if at == through { wrong } else { None };
        match at {
            0 => january(&mut lab, wrong_here).await,
            1 => may(&mut lab, wrong_here).await,
            2 => september(&mut lab, wrong_here).await,
            _ => december(&mut lab, wrong_here).await,
        }
    }
    lab
}

async fn held_in(through: usize, wrong: Wrong) -> (Vec<bool>, String) {
    let lab = played(through, wrong).await;
    let outcomes = lab.judge().await;
    let phase = format!("Phase {}", through + 1);
    let ours = of_sitting(&outcomes, &phase);
    (
        ours.iter().map(|(held, _)| *held).collect(),
        saying(&outcomes),
    )
}

// ── The shape of the room ────────────────────────────────────────────────────

/// **Four cold sittings, each on a day of its own.**
#[test]
fn the_room_is_four_cold_sittings_each_on_its_own_day() {
    let room = document();
    assert_eq!(room.phases.len(), 4);
    let days: Vec<_> = room.phases.iter().map(|p| p.day.clone()).collect();
    assert_eq!(
        days,
        ["2026-01-15", "2026-05-20", "2026-09-08", "2026-12-10"].map(|d| Some(d.to_string()))
    );
    for phase in &room.phases {
        assert!(
            phase.fresh_session,
            "{}: every sitting arrives cold",
            phase.name
        );
        assert_eq!(phase.deliveries.len(), 1, "{}", phase.name);
    }
}

/// **The entries name no verb and no place to look.**
#[tokio::test]
async fn the_entries_name_no_verb_and_no_place_to_look() {
    let lab = lab().await;
    let verbs = lab
        .surface
        .tools_for_the_model()
        .await
        .expect("the room lists its verbs");
    assert!(verbs.len() > 5, "{} verbs served", verbs.len());
    for phase in &document().phases {
        for verb in &verbs {
            let name = verb["name"].as_str().expect("a verb has a name");
            assert!(
                !phase.prompt.contains(name),
                "{} names the verb {name}",
                phase.name
            );
        }
        let said = phase.prompt.to_lowercase();
        for coached in [
            "promised_by",
            "due_on",
            "runs_out",
            "decide_by",
            "provenance",
            "testimony",
            "inference",
            "standing",
            "derived_from",
            "starred",
            "charter",
            "work:",
            "project:",
            "comment",
            "thread",
        ] {
            assert!(!said.contains(coached), "{} says {coached}", phase.name);
        }
    }
}

/// **The campaign's own word for the film is never `TV spot` after January.**
/// Later sittings ask in drifted words, and the harness holds that.
#[test]
fn later_sittings_say_the_film_and_never_tv_spot() {
    let room = document();
    for phase in room.phases.iter().skip(1) {
        let said = phase.prompt.to_lowercase();
        assert!(!said.contains("tv spot"), "{} says tv spot", phase.name);
    }
    assert!(room.phases[0].prompt.to_lowercase().contains("tv spot"));
}

/// **The sittings' locks are counted, so a lock added or dropped is seen.**
#[test]
fn the_room_has_fifty_three_locks_in_four_sittings() {
    let names: Vec<String> = expectations::for_playbook(AGENCY_ROOM)
        .expect("the room asserts")
        .iter()
        .map(|c| c.name().to_string())
        .collect();
    for (at, count) in LOCKS.iter().enumerate() {
        let phase = format!("Phase {}", at + 1);
        assert_eq!(
            names.iter().filter(|n| n.starts_with(&phase)).count(),
            *count,
            "{phase}: {names:?}"
        );
    }
    assert_eq!(names.len(), LOCKS.iter().sum::<usize>());
}

/// **No lock asks for the finished board.** Every query lock carries a window.
#[test]
fn every_query_lock_asks_at_the_end_of_its_own_sitting() {
    let document =
        std::fs::read_to_string(expectations::room_document(AGENCY_ROOM)).expect("the room reads");
    let asked = document
        .lines()
        .filter(|l| l.starts_with("recall ") || l.starts_with("search "))
        .count();
    let windowed = document
        .lines()
        .filter(|l| l.trim() == "window  phase-end")
        .count();
    assert_eq!(asked, windowed);
    assert!(asked > 0);
}

/// **The room is shipped and is not the default.**
#[test]
fn the_agency_room_is_shipped_and_is_not_the_default() {
    assert!(expectations::shipped_rooms().any(|room| room == AGENCY_ROOM));
    assert_ne!(expectations::default_room(), AGENCY_ROOM);
}

// ── Solvability: an occupant that writes each fact where its reader looks ────

#[tokio::test]
async fn solvable_january_holds_every_floor() {
    let (held, said) = held_in(0, None).await;
    assert_eq!(held, all_but(LOCKS[0], None), "{said}");
}

#[tokio::test]
async fn solvable_may_holds_every_floor() {
    let (held, said) = held_in(1, None).await;
    assert_eq!(held, all_but(LOCKS[1], None), "{said}");
}

#[tokio::test]
async fn solvable_september_holds_every_floor() {
    let (held, said) = held_in(2, None).await;
    assert_eq!(held, all_but(LOCKS[2], None), "{said}");
}

#[tokio::test]
async fn solvable_december_holds_every_slot() {
    let (held, said) = held_in(3, None).await;
    assert_eq!(held, all_but(LOCKS[3], None), "{said}");
}

/// **An untouched room holds no lock.** The room is a probe and is not won by
/// writing nothing.
#[tokio::test]
async fn an_untouched_room_holds_no_lock() {
    let mut lab = lab().await;
    for (at, key) in ["Phase 1", "Phase 2", "Phase 3", "Phase 4"]
        .iter()
        .enumerate()
    {
        lab.ends(key, at + 1).await;
    }
    let outcomes = lab.judge().await;
    assert_eq!(outcomes.len(), LOCKS.iter().sum::<usize>());
    for outcome in &outcomes {
        assert!(
            !outcome.held,
            "an untouched room held a lock: {}",
            saying(&outcomes)
        );
    }
}

// ── Discrimination: one fact written the plausible wrong way ─────────────────

/// One case per lock. The sitting is played with that one fact written wrong,
/// every earlier sitting right, and that lock reds and no other.
async fn discriminates(sitting: usize, at: usize) {
    let (held, said) = held_in(sitting, Some(at)).await;
    assert_eq!(held, all_but(LOCKS[sitting], Some(at)), "{said}");
}

#[tokio::test]
async fn january_launch_day_in_a_sentence_reds_only_the_launch_lock() {
    discriminates(0, 0).await;
}

#[tokio::test]
async fn january_copy_day_in_a_sentence_reds_only_the_copy_lock() {
    discriminates(0, 1).await;
}

#[tokio::test]
async fn january_design_day_in_a_sentence_reds_only_the_design_lock() {
    discriminates(0, 2).await;
}

#[tokio::test]
async fn january_production_day_in_a_sentence_reds_only_the_production_lock() {
    discriminates(0, 3).await;
}

#[tokio::test]
async fn january_media_day_in_a_sentence_reds_only_the_media_lock() {
    discriminates(0, 4).await;
}

#[tokio::test]
async fn january_contact_report_in_a_sentence_reds_only_the_owed_lock() {
    discriminates(0, 5).await;
}

#[tokio::test]
async fn january_hearsay_filed_settled_reds_only_the_budget_lock() {
    discriminates(0, 6).await;
}

#[tokio::test]
async fn january_a_paraphrase_of_the_heads_words_reds_only_that_lock() {
    discriminates(0, 7).await;
}

#[tokio::test]
async fn january_done_without_the_three_formats_reds_only_that_lock() {
    discriminates(0, 8).await;
}

#[tokio::test]
async fn january_legal_without_six_weeks_reds_only_that_lock() {
    discriminates(0, 9).await;
}

#[tokio::test]
async fn january_rejected_option_without_its_detail_reds_only_that_lock() {
    discriminates(0, 10).await;
}

#[tokio::test]
async fn january_the_drafted_question_without_its_subject_reds_only_that_lock() {
    discriminates(0, 11).await;
}

#[tokio::test]
async fn may_copy_landed_in_a_sentence_reds_only_that_lock() {
    discriminates(1, 0).await;
}

#[tokio::test]
async fn may_design_day_in_a_sentence_reds_only_that_lock() {
    discriminates(1, 1).await;
}

#[tokio::test]
async fn may_the_pull_without_the_tavern_reds_only_that_lock() {
    discriminates(1, 2).await;
}

#[tokio::test]
async fn may_markets_without_their_names_reds_only_that_lock() {
    discriminates(1, 3).await;
}

#[tokio::test]
async fn may_beta_without_the_disclaimer_reds_only_that_lock() {
    discriminates(1, 4).await;
}

#[tokio::test]
async fn may_login_without_its_commit_reds_only_that_lock() {
    discriminates(1, 5).await;
}

#[tokio::test]
async fn may_login_without_its_suite_reds_only_that_lock() {
    discriminates(1, 6).await;
}

#[tokio::test]
async fn may_the_quote_paraphrased_reds_only_that_lock() {
    discriminates(1, 7).await;
}

#[tokio::test]
async fn may_the_derivation_without_a_pointer_reds_only_that_lock() {
    discriminates(1, 8).await;
}

#[tokio::test]
async fn september_a_budget_without_its_number_reds_only_that_lock() {
    discriminates(2, 0).await;
}

#[tokio::test]
async fn september_an_approval_without_the_draft_reds_only_that_lock() {
    discriminates(2, 1).await;
}

#[tokio::test]
async fn september_the_new_draft_unnamed_reds_only_that_lock() {
    discriminates(2, 2).await;
}

#[tokio::test]
async fn september_print_dropped_without_the_meeting_day_reds_only_that_lock() {
    discriminates(2, 3).await;
}

#[tokio::test]
async fn september_cancel_day_in_a_sentence_reds_only_that_lock() {
    discriminates(2, 4).await;
}

#[tokio::test]
async fn september_production_day_in_a_sentence_reds_only_that_lock() {
    discriminates(2, 5).await;
}

#[tokio::test]
async fn september_station_day_in_a_sentence_reds_only_that_lock() {
    discriminates(2, 6).await;
}

#[tokio::test]
async fn september_the_wrong_claim_taken_back_reds_only_that_lock() {
    discriminates(2, 7).await;
}

#[tokio::test]
async fn september_the_second_question_without_its_subject_reds_only_that_lock() {
    discriminates(2, 8).await;
}

#[tokio::test]
async fn september_menu_sync_without_its_commit_reds_only_that_lock() {
    discriminates(2, 9).await;
}

#[tokio::test]
async fn september_ordering_waiting_on_nobody_named_reds_only_that_lock() {
    discriminates(2, 10).await;
}

#[tokio::test]
async fn december_a_wrong_budget_reds_only_the_budget_slot() {
    discriminates(3, 0).await;
}

#[tokio::test]
async fn december_the_film_owner_as_a_name_reds_only_that_pairing() {
    discriminates(3, 1).await;
}

#[tokio::test]
async fn december_the_film_copy_owner_as_a_name_reds_only_that_pairing() {
    discriminates(3, 2).await;
}

#[tokio::test]
async fn december_print_owner_as_a_name_reds_only_that_pairing() {
    discriminates(3, 3).await;
}

#[tokio::test]
async fn december_social_owner_as_a_name_reds_only_that_pairing() {
    discriminates(3, 4).await;
}

#[tokio::test]
async fn december_event_owner_as_a_name_reds_only_that_pairing() {
    discriminates(3, 5).await;
}

#[tokio::test]
async fn december_the_wrong_draft_reds_only_the_approval_slot() {
    discriminates(3, 6).await;
}

#[tokio::test]
async fn december_done_as_yes_reds_only_the_done_slot() {
    discriminates(3, 7).await;
}

#[tokio::test]
async fn december_two_slips_reds_only_the_slips_slot() {
    discriminates(3, 8).await;
}

#[tokio::test]
async fn december_undelivered_without_print_reds_only_that_slot() {
    discriminates(3, 9).await;
}

#[tokio::test]
async fn december_app_next_without_payments_reds_only_that_slot() {
    discriminates(3, 10).await;
}

#[tokio::test]
async fn december_menu_first_without_the_rejected_option_reds_only_that_slot() {
    discriminates(3, 11).await;
}

#[tokio::test]
async fn december_a_misread_that_names_nothing_reds_only_that_slot() {
    discriminates(3, 12).await;
}

#[tokio::test]
async fn december_never_asked_naming_the_wrong_question_reds_only_that_slot() {
    discriminates(3, 13).await;
}

#[tokio::test]
async fn december_landed_without_the_second_commit_reds_only_that_slot() {
    discriminates(3, 14).await;
}

#[tokio::test]
async fn january_the_question_filed_on_the_app_with_no_link_reds_only_the_phase_lock() {
    discriminates(0, 12).await;
}

#[tokio::test]
async fn may_the_answer_filed_against_krusty_reds_only_the_attribution_lock() {
    discriminates(1, 9).await;
}

#[tokio::test]
async fn may_the_question_written_over_by_its_answer_reds_only_the_readable_lock() {
    discriminates(1, 10).await;
}

#[tokio::test]
async fn september_the_closing_word_filed_against_martin_reds_only_the_attribution_lock() {
    discriminates(2, 11).await;
}

#[tokio::test]
async fn september_the_answer_written_over_by_the_closing_word_reds_only_the_readable_lock() {
    discriminates(2, 12).await;
}

#[tokio::test]
async fn december_the_split_bill_slot_naming_the_earlier_state_reds_only_that_slot() {
    discriminates(3, 15).await;
}

// ── A right write in other capitals still holds ──────────────────────────────

/// **One lock of a sitting, after the sittings before it were played right and
/// only `claims` were written in it.** Each claim is a subject, the words and
/// the handles to stand up first, so a case can write a fact the way a right
/// occupant might spell it without playing the whole sitting again.
async fn held_after_writing(
    prior: Option<usize>,
    day: &str,
    phase: &str,
    at: usize,
    stand_up: &[(&str, &str, &str)],
    claims: &[(&str, &str)],
) -> (bool, String) {
    let mut lab = match prior {
        Some(through) => played(through, None).await,
        None => self::lab().await,
    };
    let sid = lab.sitting(day).await;
    for (kind, handle, name) in stand_up {
        lab.add(&sid, kind, handle, name, None).await;
    }
    for (subject, content) in claims {
        lab.say(&sid, subject, content, json!({})).await;
    }
    let (key, boundary_at) = match phase {
        "Phase 1" => ("Phase 1", 1),
        _ => ("Phase 2", 2),
    };
    lab.ends(key, boundary_at).await;
    let outcomes = lab.judge().await;
    let ours = of_sitting(&outcomes, phase);
    (ours[at].0, saying(&outcomes))
}

/// **A place stored as its handle reads in lower case**, so a needle that needs
/// the capital reds a model that linked the thing instead of typing its name.
#[tokio::test]
async fn may_the_pull_written_as_a_handle_holds_the_pull_lock() {
    let (held, said) = held_after_writing(
        Some(0),
        "2026-05-20",
        "Phase 2",
        2,
        &[],
        &[(
            CAMPAIGN,
            "Milhouse was pulled over to @org:moes-tavern from 6 to 26 April.",
        )],
    )
    .await;
    assert!(held, "{said}");
}

#[tokio::test]
async fn may_a_market_written_as_a_handle_holds_the_markets_lock() {
    let (held, said) = held_after_writing(
        Some(0),
        "2026-05-20",
        "Phase 2",
        3,
        &[("thing", "gamma", "Gamma")],
        &[(
            CAMPAIGN,
            "Krusty added two markets, Beta and @thing:gamma, on the phone and nothing is written down.",
        )],
    )
    .await;
    assert!(held, "{said}");
}

/// **A quote that opens the sentence is capitalised** and is still the quote.
#[tokio::test]
async fn may_the_quote_opening_a_sentence_holds_the_quote_lock() {
    let (held, said) = held_after_writing(
        Some(0),
        "2026-05-20",
        "Phase 2",
        7,
        &[],
        &[(
            APP,
            "Krusty said \"Terrible reception at the basement location\".",
        )],
    )
    .await;
    assert!(held, "{said}");
}

#[tokio::test]
async fn january_the_heads_words_opening_a_sentence_hold_the_decision_lock() {
    let (held, said) = held_after_writing(
        None,
        "2026-01-15",
        "Phase 1",
        7,
        &[("project", "visa", "The Ordering App")],
        &[(
            APP,
            "Homer decided: \"A menu we have not synced is an order for nothing, so we sync it first\".",
        )],
    )
    .await;
    assert!(held, "{said}");
}

/// **December's decision slot holds when the quote opens the value.**
#[tokio::test]
async fn december_the_heads_words_opening_the_value_hold_the_decision_slot() {
    let mut lab = played(2, None).await;
    let sid = lab.sitting("2026-12-10").await;
    lab.say(
        &sid,
        APP,
        "The answer to menu_first.",
        json!({"fields": {"menu_first": "A menu we have not synced is an order for nothing, said Homer; Martin's hard-coded menu was turned down"}}),
    )
    .await;
    lab.ends("Phase 4", 4).await;
    let outcomes = lab.judge().await;
    let december = of_sitting(&outcomes, "Phase 4");
    assert!(december[11].0, "{}", saying(&outcomes));
}

/// **December's yes-or-no slot, answered in either capital.** `no` and `No` are
/// the same answer; `yes` and `maybe` are not.
async fn done_slot_after_writing(value: &str) -> (bool, String) {
    let mut lab = played(2, None).await;
    let sid = lab.sitting("2026-12-10").await;
    lab.say(
        &sid,
        CAMPAIGN,
        "The answer to spot_done.",
        json!({"fields": {"spot_done": value}}),
    )
    .await;
    lab.ends("Phase 4", 4).await;
    let outcomes = lab.judge().await;
    (of_sitting(&outcomes, "Phase 4")[7].0, saying(&outcomes))
}

#[tokio::test]
async fn december_done_as_a_capitalised_no_holds_the_done_slot() {
    let (held, said) = done_slot_after_writing("No").await;
    assert!(held, "{said}");
}

#[tokio::test]
async fn december_done_as_maybe_reds_the_done_slot() {
    let (held, said) = done_slot_after_writing("maybe").await;
    assert!(!held, "{said}");
}

/// **A suite linked as a handle reads in lower case.** May and December both
/// name it, and neither may need the camel case.
#[tokio::test]
async fn may_the_suite_written_as_a_handle_holds_the_suite_lock() {
    let (held, said) = held_after_writing(
        Some(0),
        "2026-05-20",
        "Phase 2",
        6,
        &[],
        &[(APP, "The loginflowsuite verified login.")],
    )
    .await;
    assert!(held, "{said}");
}

#[tokio::test]
async fn december_landed_with_the_suite_in_lower_case_holds_the_landed_slot() {
    let mut lab = played(2, None).await;
    let sid = lab.sitting("2026-12-10").await;
    lab.say(
        &sid,
        APP,
        "The answer to app_landed.",
        json!({"fields": {"app_landed": "login a1b2c3d, verified by the loginflowsuite; menu sync c0ffee1"}}),
    )
    .await;
    lab.ends("Phase 4", 4).await;
    let outcomes = lab.judge().await;
    assert!(
        of_sitting(&outcomes, "Phase 4")[14].0,
        "{}",
        saying(&outcomes)
    );
}

/// **May's offline requirement may be the operator's word.** The entry has the
/// operator say they worked it out, which is their word, backed as testimony.
/// The lock's question is whether the requirement points at the quote it was
/// worked out from, whatever the backing.
#[tokio::test]
async fn may_the_requirement_filed_as_the_operators_word_still_points_at_the_quote() {
    let mut lab = played(0, None).await;
    let sid = lab.sitting("2026-05-20").await;
    let quote = lab
        .say(
            &sid,
            APP,
            "Krusty said \"the basement location has terrible reception\".",
            json!({}),
        )
        .await;
    lab.say(
        &sid,
        APP,
        "Orders must work offline, because of the basement reception.",
        json!({"derived_from": quote}),
    )
    .await;
    lab.ends("Phase 2", 2).await;
    let outcomes = lab.judge().await;
    assert!(
        of_sitting(&outcomes, "Phase 2")[8].0,
        "{}",
        saying(&outcomes)
    );
}

/// **Martin's answer is attributed by a link, in any shape a link takes, and
/// not by a name in a sentence.**
async fn martin_answer_after_writing(content: &str, extra: Value) -> (bool, String) {
    let mut lab = played(0, None).await;
    let sid = lab.sitting("2026-05-20").await;
    lab.say(&sid, ORDERING, content, extra).await;
    lab.ends("Phase 2", 2).await;
    let outcomes = lab.judge().await;
    (of_sitting(&outcomes, "Phase 2")[9].0, saying(&outcomes))
}

#[tokio::test]
async fn may_the_answer_linking_martin_by_a_mention_holds_the_attribution_lock() {
    let (held, said) = martin_answer_after_writing(
        "@person:martin says splitting a bill needs payments, so it moves after payments.",
        json!({}),
    )
    .await;
    assert!(held, "{said}");
}

#[tokio::test]
async fn may_the_answer_holding_martin_in_a_key_holds_the_attribution_lock() {
    let (held, said) = martin_answer_after_writing(
        "Splitting a bill needs payments, so it moves after payments.",
        json!({"fields": {"answered_by": "person:martin"}}),
    )
    .await;
    assert!(held, "{said}");
}

#[tokio::test]
async fn may_the_answer_naming_martin_only_in_a_sentence_reds_the_attribution_lock() {
    let (held, said) = martin_answer_after_writing(
        "Martin said splitting a bill needs payments, so it moves after payments.",
        json!({}),
    )
    .await;
    assert!(!held, "{said}");
}

/// **January's question may be filed on somebody and linked to the phase.**
#[tokio::test]
async fn january_the_question_filed_on_krusty_and_linked_to_the_phase_holds_the_phase_lock() {
    let mut lab = lab().await;
    let sid = lab.sitting("2026-01-15").await;
    lab.add(&sid, "project", "visa", "The Ordering App", None)
        .await;
    lab.add(&sid, "work", "red-bike", "The ordering phase", Some(APP))
        .await;
    lab.say(
        &sid,
        "person:krusty",
        "Krusty asked whether the ordering screen can split one bill between guests.",
        json!({"shape": "about", "object": ORDERING}),
    )
    .await;
    lab.ends("Phase 1", 1).await;
    let outcomes = lab.judge().await;
    assert!(
        of_sitting(&outcomes, "Phase 1")[12].0,
        "{}",
        saying(&outcomes)
    );
}

/// **December's third format is "third" or "3rd".** A needle that needed the
/// word failed a model that wrote the numeral.
#[tokio::test]
async fn december_the_third_format_as_a_numeral_holds_the_undelivered_slot() {
    let mut lab = played(2, None).await;
    let sid = lab.sitting("2026-12-10").await;
    lab.say(
        &sid,
        CAMPAIGN,
        "The answer to undelivered.",
        json!({"fields": {"undelivered": "the Beta disclaimer, the 3rd station format and the print ads"}}),
    )
    .await;
    lab.ends("Phase 4", 4).await;
    let outcomes = lab.judge().await;
    assert!(
        of_sitting(&outcomes, "Phase 4")[9].0,
        "{}",
        saying(&outcomes)
    );
}

/// **A failing lock names every needle it missed**, so a reader fixing one
/// does not meet the next on the following run.
#[tokio::test]
async fn december_an_undelivered_slot_missing_two_things_says_both() {
    let mut lab = played(2, None).await;
    let sid = lab.sitting("2026-12-10").await;
    lab.say(
        &sid,
        CAMPAIGN,
        "The answer to undelivered.",
        json!({"fields": {"undelivered": "the Beta disclaimer"}}),
    )
    .await;
    lab.ends("Phase 4", 4).await;
    let outcomes = lab.judge().await;
    let slot = &of_sitting(&outcomes, "Phase 4")[9];
    assert!(!slot.0, "{}", saying(&outcomes));
    assert_eq!(
        slot.1.matches("is not in what came back").count(),
        2,
        "the failure names one missing needle and not both: {}",
        slot.1
    );
}

// ── December: an answer given in the reply and written nowhere ───────────────

/// **A December that answers in the reply alone fails every slot.** The earlier
/// sittings are played right and December writes nothing, which is what an
/// occupant does that labels its reply with the names it was given. Every
/// slot lock reds, and none holds on what the earlier sittings stored — the
/// half an untouched room cannot say.
#[tokio::test]
async fn december_answered_in_the_reply_alone_reds_every_slot() {
    let mut lab = played(2, None).await;
    let _sid = lab.sitting("2026-12-10").await;
    lab.ends("Phase 4", 4).await;
    let outcomes = lab.judge().await;
    let december = of_sitting(&outcomes, "Phase 4");
    assert_eq!(december.len(), LOCKS[3], "{}", saying(&outcomes));
    assert!(
        december.iter().all(|(held, _)| !held),
        "a December that wrote nothing held a slot: {}",
        saying(&outcomes),
    );
    for earlier in ["Phase 1", "Phase 2", "Phase 3"] {
        assert!(
            of_sitting(&outcomes, earlier).iter().all(|(held, _)| *held),
            "{earlier} did not hold, so the reds above are not December's: {}",
            saying(&outcomes),
        );
    }
}

/// 🚨 **No lock here rests on a needle that matches somewhere else.**
#[tokio::test]
async fn no_lock_here_rests_on_a_needle_that_matches_somewhere_else() {
    let lab = played(3, None).await;
    let summary = jojobot_exercise::lock::needle_summary(
        &lab.surface,
        &jojobot_exercise::lock::locks_of(AGENCY_ROOM),
        &lab.boundaries,
    )
    .await;
    assert!(
        summary.nowhere.is_empty(),
        "a needle matched nowhere: {:?}",
        summary.nowhere
    );
    assert!(
        summary.findings.is_empty(),
        "a lock rests on a needle that matches somewhere else: {:?}",
        summary.findings
    );
}
