//! "I made the call the skill printed, word for word."
//!
//! A worked call in a skill is a contract: a session copies it, and it has to
//! do what the sentence around it says. Each story below sends the call as the
//! skill prints it, through the served surface as a real client does, and then
//! asks what the call was supposed to leave behind.

use serde_json::{Value, json};

use super::dsl::Story;

/// The body of a shipped skill, as a session fetches it.
async fn body_of(story: &Story, skill: &str) -> String {
    story.skill(skill).await["skill"]["body"]
        .as_str()
        .unwrap_or_else(|| panic!("the {skill} skill serves a body"))
        .to_string()
}

/// **A call as the skill prints it, read out of the skill and not typed in the
/// story.** A printed call is a verb and then `name: value` pairs where each
/// value is JSON. `prefix` picks the call by how it starts; the arguments are
/// whatever the skill prints after it, up to the end of the line or the closing
/// backtick. A skill whose example changes changes the call this sends.
fn printed(body: &str, prefix: &str) -> (String, Value) {
    let at = body
        .find(prefix)
        .unwrap_or_else(|| panic!("the skill prints no call starting {prefix:?}: {body}"));
    assert_eq!(
        body.matches(prefix).count(),
        1,
        "the prefix {prefix:?} picks one call"
    );
    let span = body[at..]
        .split(['\n', '`'])
        .next()
        .unwrap_or_default()
        .trim();
    let (verb, mut rest) = span
        .split_once(char::is_whitespace)
        .unwrap_or_else(|| panic!("a call is a verb and arguments: {span}"));
    let mut args = serde_json::Map::new();
    loop {
        rest = rest.trim_start();
        let Some((name, after)) = rest.split_once(':') else {
            break;
        };
        let after = after.trim_start();
        let mut values = serde_json::Deserializer::from_str(after).into_iter::<Value>();
        let Some(Ok(value)) = values.next() else {
            panic!("the value of {name:?} in {span:?} is not JSON");
        };
        let used = values.byte_offset();
        args.insert(name.trim().to_string(), value);
        rest = &after[used..];
    }
    (verb.to_string(), Value::Object(args))
}

/// **The asking skill's calls, as printed.** A view with one thing added
/// changes the answer; the people in a place are a walk inward from the place,
/// and a kind plus a `follow` is not that question; a visit costing more than
/// fifty is a record-scoped comparison on a key declared a number; and `keys`
/// narrows what each object brings back.
#[tokio::test]
async fn the_asking_skills_calls_do_what_the_skill_says() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let asking = body_of(&story, "asking").await;

    // ── a view with one thing added ─────────────────────────────────────────
    s.add("bot:delta", "Delta").await;
    let delta = story.as_bot("bot:delta").await;
    delta
        .call(
            "set_charter",
            json!({ "bot": "bot:gamma", "prose": "Holds the plan and does not implement." }),
        )
        .await;
    // The skill prints the shipped question with one thing added; the question
    // without it is the same call with that argument taken off.
    let (verb, with_charter) = printed(&asking, "recall  view: \"colleagues\"  charter");
    let mut plain = with_charter.clone();
    assert!(
        plain
            .as_object_mut()
            .and_then(|args| args.remove("charter"))
            .is_some(),
        "the printed call adds the charter: {with_charter}"
    );
    s.call(&verb, plain).await.never_says("Holds the plan");
    // **The pair**: the added argument is what brings the charter.
    s.call(&verb, with_charter).await.says("Holds the plan");

    // ── the people in a place ───────────────────────────────────────────────
    s.add("place:springfield", "Springfield").await;
    s.add("place:shelbyville", "Shelbyville").await;
    s.add("person:homer", "Homer").await;
    s.add("person:bart", "Bart").await;
    for (who, place) in [
        ("person:homer", "place:springfield"),
        ("person:bart", "place:shelbyville"),
    ] {
        s.call(
            "capture",
            json!({
                "subject": who, "content": "lives there", "provenance": "testimony",
                "shape": "location", "object": place,
            }),
        )
        .await;
    }
    let (verb, walk) = printed(&asking, "recall  subject: \"place:springfield\"");
    let walked = s.call(&verb, walk).await;
    walked.says("person:homer").never_says("person:bart");
    // **And the form the skill used to print is not that question**: a kind
    // plus a follow lists everybody of the kind.
    let everyone = s
        .call(
            "recall",
            json!({
                "kind": "person",
                "follow": { "shape": "location", "direction": "in" },
            }),
        )
        .await;
    everyone.says("person:homer").says("person:bart");

    // ── a key compared on the record, and what comes back of each ───────────
    // **The costs are picked so that text and number disagree**: as text "100"
    // sorts before "50" and "9" after it, so a comparison that read the values
    // as text keeps the cheap visit and drops the dear one.
    s.add("thing:the-couch", "The Couch").await;
    s.add("thing:floor-pump", "The Floor Pump").await;
    s.event_with(
        "thing:the-couch",
        "a dear visit",
        json!({ "cost": "100", "colour": "green" }),
        &[],
    )
    .await;
    s.event_with(
        "thing:floor-pump",
        "a cheap visit",
        json!({ "cost": "9" }),
        &[],
    )
    .await;
    let (_, ask) = printed(&asking, "recall  fields: [{\"key\": \"cost\"");
    let (declare, declaration) = printed(&asking, "declare_type  name: \"visit\"");
    // **The paired negative**: nothing declared the key a number yet, so the
    // comparison is refused and names what licenses it.
    s.refused("recall", ask.clone()).await.says("declare_type");
    s.call(&declare, declaration).await;
    let dear = s.call("recall", ask).await;
    let found: Vec<String> = dear.json()["objects"]
        .as_array()
        .expect("the answer lists objects")
        .iter()
        .map(|object| object["id"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        found,
        ["thing:the-couch"],
        "100 is more than 50 and 9 is not, as numbers: {}",
        dear.raw()
    );
    let narrowed = s
        .call(
            "recall",
            json!({ "subject": "thing:the-couch", "keys": ["cost"] }),
        )
        .await;
    narrowed.holds_at("/objects/0/fields/cost", "100");
    assert!(
        narrowed.json()["objects"][0]["fields"]
            .get("colour")
            .is_none(),
        "`keys` narrows what each object brings back: {}",
        narrowed.raw()
    );

    story.finish().await;
}

/// **The rhythms skill's calls, as printed.** A loop is written with its keys in
/// `sets`; a check-in on a loop that already ran is dated with `recorded_at`; a
/// promise cannot lose `promised_by` and is ended instead; a day on a thing
/// that carries `runs_out` is taken away by clearing that key; and a loop the
/// operator finds stressful is archived.
#[tokio::test]
async fn the_rhythms_skills_calls_do_what_the_skill_says() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:gravel-bike", "The Gravel Bike").await;

    // ── a loop, its keys in `sets`, and a check-in dated the day it ran ─────
    s.call(
        "add_entity",
        json!({
            "kind": "rhythm", "handle": "chain-check", "name": "Chain check",
            "source": "user-named", "parent": "thing:gravel-bike",
            "sets": { "cadence_days": "60", "advances_from": "due_date" },
        }),
    )
    .await;
    s.recall("rhythm:chain-check")
        .await
        .says("\"cadence_days\":\"60\"")
        .says("\"advances_from\":\"due_date\"");
    s.call(
        "capture",
        json!({
            "subject": "rhythm:chain-check", "content": "looked at the chain",
            "provenance": "testimony", "check_in": "ran", "recorded_at": "2026-09-20",
        }),
    )
    .await;
    s.recall("rhythm:chain-check")
        .await
        .says("\"last_check_in\":\"2026-09-20\"");

    // ── a promise is ended, not cleared ─────────────────────────────────────
    s.add("person:ned-flanders", "Ned").await;
    s.add_under(
        "person:ned-flanders",
        "promise:return-the-wrench",
        "Return the wrench",
    )
    .await;
    let promised = s
        .call(
            "capture",
            json!({
                "subject": "promise:return-the-wrench", "content": "back by July",
                "provenance": "testimony", "sets": { "promised_by": "2026-07-01" },
            }),
        )
        .await;
    let owed = || json!({ "fields": [{ "key": "due_on" }], "overdue": { "as_of": "2026-08-05" } });
    s.shape("what is owed", owed())
        .await
        .says("promise:return-the-wrench");
    s.refused(
        "update_fact",
        json!({ "address": promised.field("address"), "clear_fields": ["promised_by"] }),
    )
    .await
    .says("promised_by");
    s.event_with(
        "promise:return-the-wrench",
        "gave it back",
        json!({ "ended": "delivered" }),
        &[],
    )
    .await;
    s.shape("what is owed after it ended", owed())
        .await
        .never_says("promise:return-the-wrench");

    // ── a day that is not a promise's is taken away by clearing its key ─────
    s.add("thing:torque-wrench", "The Torque Wrench").await;
    let covered = s
        .call(
            "capture",
            json!({
                "subject": "thing:torque-wrench", "content": "the warranty runs out",
                "provenance": "testimony", "sets": { "runs_out": "2026-06-01" },
            }),
        )
        .await;
    s.shape("what is owed with the warranty", owed())
        .await
        .says("thing:torque-wrench");
    s.call(
        "update_fact",
        json!({ "address": covered.field("address"), "clear_fields": ["runs_out"] }),
    )
    .await;
    s.shape("what is owed without the warranty", owed())
        .await
        .never_says("thing:torque-wrench");

    // ── a loop the operator finds stressful is archived ─────────────────────
    s.call(
        "archive_entity",
        json!({ "handle": "rhythm:chain-check", "reason": "the operator finds it stressful" }),
    )
    .await;
    s.list("rhythm").await.never_says("rhythm:chain-check");

    story.finish().await;
}

/// **The projects skill's call, as printed**: a project's `columns` list holds
/// the five shipped words and lands, and a list that drops one is refused,
/// naming the word it lacks.
#[tokio::test]
async fn the_projects_skills_columns_call_holds_every_shipped_word() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:atlas", "Atlas").await;
    let projects = body_of(&story, "projects").await;
    // The call as the skill prints it, with the words a capture needs beside it.
    let (verb, mut printed_columns) = printed(
        &projects,
        "capture  subject: \"project:atlas\"  sets: {\"columns\"",
    );
    let columns = printed_columns["sets"]["columns"]
        .as_str()
        .expect("the skill prints the columns")
        .to_string();
    printed_columns["content"] = json!("the board's columns");
    printed_columns["provenance"] = json!("testimony");
    s.call(&verb, printed_columns.clone()).await;
    s.recall("project:atlas").await.says(&columns);
    // The same list with one shipped word taken out is refused, naming the word.
    let mut shorter = printed_columns;
    shorter["sets"]["columns"] = json!(columns.replace("waiting, ", ""));
    s.refused(&verb, shorter).await.says("waiting");
    story.finish().await;
}

/// **The projects skill's checklist calls, as printed.** The list is made under
/// the work it is about, the first step lands, the second lands after it by
/// address, and `entries` reads them back in that order.
#[tokio::test]
async fn the_projects_skills_checklist_calls_do_what_the_skill_says() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let projects = body_of(&story, "projects").await;
    s.add("work:phi", "Phi").await;
    // Each call as the skill prints it: the list, then the first step, then the
    // second step after the first by the address the skill gives.
    let (verb, list) = printed(
        &projects,
        "add_entity  kind: \"list\"  handle: \"phi-checklist\"",
    );
    s.call(&verb, list).await;
    let (verb, first) = printed(
        &projects,
        "capture  subject: \"list:phi-checklist\"  content: \"book the room\"",
    );
    let first = s.call(&verb, first).await;
    let first_step = first.json()["address"]
        .as_str()
        .expect("a capture answers with the address")
        .to_string();
    let (verb, second) = printed(
        &projects,
        "capture  subject: \"list:phi-checklist\"  content: \"send the invites\"",
    );
    assert_eq!(
        second["after"],
        json!(first_step),
        "the address the skill gives for the first step is the address that step has"
    );
    s.call(&verb, second).await;
    s.call(
        "capture",
        json!({"subject": "list:phi-checklist", "content": "print the signs",
               "provenance": "testimony", "after": "first"}),
    )
    .await;

    let read = s
        .call("recall", json!({"subject": "list:phi-checklist"}))
        .await;
    let body = read.json();
    let texts: Vec<&str> = body["objects"][0]["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|e| e["text"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        texts,
        ["print the signs", "book the room", "send the invites"]
    );
    story.finish().await;
}

/// **The organizing skill is listed for every bot and each full container's
/// refusal names it (rule 593).** A bot's boot lists it; fetched by name it is
/// the shipped text. Full seats, a full room and a full thread each refuse a
/// write, and each refusal's way forward names the skill.
#[tokio::test]
async fn every_full_refusal_names_the_organizing_skill() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let (boot, _) = story
        .call("start_here", json!({"bot": "gamma", "brief": true}))
        .await;
    let boot = boot.json();
    let listed: Vec<&str> = boot["skills"]
        .as_array()
        .unwrap_or_else(|| panic!("the boot lists skills: {boot}"))
        .iter()
        .filter_map(|skill| skill["name"].as_str())
        .collect();
    assert!(listed.contains(&"organizing"), "{listed:?}");
    let organizing = body_of(&story, "organizing").await;
    assert!(organizing.starts_with("# organizing\n"), "{organizing}");
    assert_eq!(organizing.len(), 4888, "the shipped text, whole");

    let names_it = |refusal: &Value| {
        assert!(
            refusal["how_to_proceed"]
                .as_str()
                .is_some_and(|how| how.contains("organizing")),
            "the refusal names the skill: {refusal}"
        );
    };

    // ── full seats ──────────────────────────────────────────────────────────
    let gamma = story.as_bot("bot:gamma").await;
    for n in 0..5 {
        gamma
            .call(
                "capture",
                json!({"subject": "bot:gamma", "content": format!("gamma rule {n}"),
                       "provenance": "testimony", "fields": {"starred": "true"}}),
            )
            .await;
    }
    let seats = gamma
        .refused(
            "capture",
            json!({"subject": "bot:gamma", "content": "gamma rule five",
                   "provenance": "testimony", "fields": {"starred": "true"}}),
        )
        .await
        .json();
    assert!(
        seats["seated"].is_array(),
        "the seats are what refused: {seats}"
    );
    names_it(&seats);

    // ── a full room ─────────────────────────────────────────────────────────
    s.call(
        "capture",
        json!({"subject": "bot:gamma", "content": "gamma holds one thought",
               "sets": {"thought_capacity": "1"}}),
    )
    .await;
    s.add("thing:jukebox", "The Jukebox").await;
    s.add("thing:battery", "The Battery").await;
    gamma
        .call(
            "capture",
            json!({"subject": "bot:gamma", "content": "the jukebox needs a needle",
                   "shape": "connection", "object": "thing:jukebox"}),
        )
        .await;
    let room = gamma
        .refused(
            "capture",
            json!({"subject": "bot:gamma", "content": "the battery needs replacing",
                   "shape": "connection", "object": "thing:battery"}),
        )
        .await
        .json();
    assert!(room["room"].is_array(), "the room is what refused: {room}");
    names_it(&room);

    // ── a full thread ───────────────────────────────────────────────────────
    s.add("thread:milhouse-moves", "Milhouse moves").await;
    for n in 0..3 {
        s.call(
            "capture",
            json!({"subject": "thread:milhouse-moves", "content": format!("moving note {n}")}),
        )
        .await;
    }
    let thread = s
        .refused(
            "capture",
            json!({"subject": "thread:milhouse-moves", "content": "moving note three"}),
        )
        .await
        .json();
    assert_eq!(thread["status"], "blocked", "{thread}");
    assert!(
        thread.get("seated").is_none() && thread.get("room").is_none(),
        "{thread}"
    );
    names_it(&thread);
    story.finish().await;
}

/// **A bot that follows the organizing skill fits fifteen rules in five
/// seats.** Two rules that bind on every turn and one index rule are starred;
/// twelve are filed by purpose, each starting with its moment. The boot carries
/// the three, and the skill's own recall, as printed, returns the operator
/// branch whole.
#[tokio::test]
async fn a_bot_that_follows_the_organizing_skill_reaches_its_filed_rules() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let organizing = body_of(&story, "organizing").await;
    let gamma = story.as_bot("bot:gamma").await;

    // The filed rules: the skill's own example first, then five more on the
    // operator and six on work.
    let (verb, filed) = printed(
        &organizing,
        "capture  subject: \"bot:gamma\"  content: \"Before you write to the operator",
    );
    gamma.call(&verb, filed).await;
    let mut operator_rules = 1;
    for n in 0..5 {
        gamma
            .call(
                "capture",
                json!({"subject": "bot:gamma",
                       "content": format!("Before you write to the operator, check point {n}"),
                       "fields": {"purpose": "operator"}}),
            )
            .await;
        operator_rules += 1;
    }
    for n in 0..6 {
        gamma
            .call(
                "capture",
                json!({"subject": "bot:gamma",
                       "content": format!("Before you file or move work, check point {n}"),
                       "fields": {"purpose": "work"}}),
            )
            .await;
    }
    // The every-turn rules and the index rule are starred.
    for content in [
        "Answer in the language the operator wrote in.",
        "Never send money without the operator's word.",
        "Before you write to the operator, load my rules with purpose operator. Before you \
         file or move work, load purpose work. Load a purpose with recall bot:gamma, facts: \
         true, status: active, fields: [{key: purpose, value: the purpose, scope: record}].",
    ] {
        gamma
            .call(
                "capture",
                json!({"subject": "bot:gamma", "content": content,
                       "fields": {"starred": "true"}}),
            )
            .await;
    }

    let (boot, _) = story
        .call("start_here", json!({"bot": "gamma", "brief": true}))
        .await;
    let boot = boot.json();
    assert_eq!(
        boot["identity"]["rules"].as_array().map(Vec::len),
        Some(3),
        "{boot}"
    );

    let (verb, load) = printed(&organizing, "recall  subject: \"bot:gamma\"  facts: true");
    let loaded = gamma.call(&verb, load).await.json();
    let facts = loaded["objects"][0]["facts"]
        .as_array()
        .unwrap_or_else(|| panic!("the recall returns the bot's records: {loaded}"));
    assert_eq!(
        facts.len(),
        operator_rules,
        "the operator branch, whole: {loaded}"
    );
    assert!(
        facts
            .iter()
            .all(|fact| fact["fields"]["purpose"] == "operator"),
        "{loaded}"
    );
    story.finish().await;
}
