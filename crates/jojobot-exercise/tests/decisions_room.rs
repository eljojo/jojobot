use jojobot_exercise::expectations;
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Observed, Outcome};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[test]
fn the_decision_room_delivers_the_whole_register_across_cold_sittings() {
    let name = "rooms/decisions.md";
    assert!(
        expectations::shipped_rooms().any(|room| room == name),
        "the decision room is not registered"
    );
    let room = Playbook::read(&expectations::room_document(name)).expect("the room reads");
    assert_eq!(room.phases.len(), 16);
    assert!(room.phases.iter().all(|phase| phase.fresh_session));
    assert!(room.phases.iter().all(|phase| phase.day.is_some()));

    let filings = &room.phases[..9];
    let rows: Vec<&str> = filings
        .iter()
        .flat_map(|phase| phase.prompt.lines())
        .filter(|line| line.starts_with("Rule "))
        .collect();
    assert_eq!(rows.len(), 225, "a selected rule was omitted or duplicated");
    assert!(rows.iter().any(|line| line.starts_with("Rule axiom:")));
    assert!(rows.iter().any(|line| line.starts_with("Rule 58:")));
    assert!(rows.iter().any(|line| line.starts_with("Rule 288:")));

    let questions = &room.phases[9..];
    assert_eq!(questions.len(), 7);
    assert!(
        questions
            .iter()
            .all(|phase| phase.prompt.contains("Record your answer"))
    );
}

#[tokio::test]
async fn the_decision_questions_have_separate_answer_addresses() {
    let (_room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    expectations::seed_for("rooms/decisions.md")
        .expect("the starting world builds")
        .furnish(&surface)
        .await
        .expect("the starting world applies");
    for handle in [
        "work:pm-state",
        "work:conditional-rules",
        "work:loop-check-in",
        "work:bot-capacity",
        "work:rule-58-replacement",
        "work:image-attachments",
        "work:shipped-kind-fields",
    ] {
        let found = surface.call("recall", json!({"subject": handle})).await;
        let read: serde_json::Value = serde_json::from_str(&found).expect("a JSON read");
        assert_eq!(
            read["objects"][0]["id"], handle,
            "{handle} is missing: {found}"
        );
    }
}

#[test]
fn the_decision_room_asserts_the_filing_and_all_seven_answers() {
    let locks = expectations::for_playbook("rooms/decisions.md").expect("room locks");
    assert!(
        locks.len() >= 15,
        "the filing and seven answer questions need separate checks"
    );
}

async fn fresh_sitting(surface: &Surface, day: &str) -> String {
    let booted = surface
        .must(
            "start_here",
            json!({"bot": "assistant", "brief": true, "today": day}),
        )
        .await
        .expect("the sitting boots");
    if let Some(sid) = booted["session"]["sid"].as_str() {
        return sid.to_string();
    }
    let answered = surface
        .must(
            "start_here",
            json!({"bot": "assistant", "brief": true, "today": day, "resume": "new"}),
        )
        .await
        .expect("a new sitting starts");
    answered["session"]["sid"]
        .as_str()
        .expect("the sitting has a session id")
        .to_string()
}

async fn create_threads(surface: &Surface, playbook: &Playbook) -> String {
    let first = &playbook.phases[0];
    let subjects = first
        .prompt
        .lines()
        .find_map(|line| {
            line.strip_prefix("These are the subjects the operator uses for design rules: ")
        })
        .expect("the subjects were said once")
        .trim_end_matches('.')
        .split(", ");
    let first_sid = fresh_sitting(surface, first.day.as_deref().expect("a day")).await;
    for subject in subjects {
        let handle = subject.replace(' ', "-");
        let mut args = json!({"kind": "topic", "handle": subject, "name": subject,
                              "source": "user-named", "sid": first_sid});
        args["handle"] = json!(handle);
        let first = surface.call("add_entity", args.clone()).await;
        let result: serde_json::Value = serde_json::from_str(&first).expect("an entity result");
        if result["status"] == "blocked" {
            let instruction = result["how_to_proceed"]
                .as_str()
                .expect("the refusal's route");
            let token = instruction
                .split("override_token: \"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .unwrap_or_else(|| {
                    panic!("the distinct subject {subject} has no override: {first}")
                });
            args["override_token"] = json!(token);
            surface
                .must("add_entity", args)
                .await
                .unwrap_or_else(|e| panic!("the distinct subject {subject} is filed: {e:#}"));
        } else {
            assert_eq!(result["id"], format!("topic:{handle}"), "{first}");
        }
    }
    first_sid
}

async fn file_register(surface: &Surface, playbook: &Playbook) {
    let first_sid = create_threads(surface, playbook).await;
    let mut filed = 0;
    for (phase_index, phase) in playbook.phases[..9].iter().enumerate() {
        let sid = if phase_index == 0 {
            first_sid.clone()
        } else {
            fresh_sitting(surface, phase.day.as_deref().expect("a day")).await
        };
        for row in phase
            .prompt
            .lines()
            .filter(|line| line.starts_with("Rule "))
        {
            let (number, rest) = row
                .strip_prefix("Rule ")
                .expect("a rule")
                .split_once(": ")
                .expect("a numbered rule");
            let thread = rest
                .rsplit_once(" Thread: ")
                .and_then(|(_, tail)| tail.split('.').next())
                .unwrap_or("what jojobot is")
                .replace(' ', "-");
            let archived = rest.contains("Retired rule; keep it reachable as history");
            let conditional = rest.contains("Conditional rule:");
            let home = format!("topic:{thread}");
            let receipt = surface
                .must(
                    "capture",
                    json!({"subject": home, "content": row,
                           "provenance": "testimony", "sid": sid,
                           "standing": if conditional { "open" } else { "settled" },
                           "fields": {"rule_number": number}}),
                )
                .await
                .unwrap_or_else(|e| panic!("rule {number} is filed: {e:#}"));
            if archived {
                let address = receipt["address"].as_str().expect("the claim address");
                surface
                    .must(
                        "update_fact",
                        json!({"address": address, "status": "archived", "sid": sid}),
                    )
                    .await
                    .unwrap_or_else(|e| panic!("rule {number} is archived: {e:#}"));
            }
            if !archived {
                if let Some(edges) = rest.split(" This also bears on ").nth(1) {
                    let edges = edges
                        .split(". ")
                        .next()
                        .unwrap_or(edges)
                        .trim_end_matches('.');
                    for edge in edges.split("; ") {
                        let edge = edge.replace(' ', "-");
                        surface
                            .must(
                                "capture",
                                json!({"subject": format!("topic:{thread}"),
                                "content": row,
                            "provenance":"testimony", "shape":"connection",
                            "object":format!("topic:{edge}"), "sid":sid,
                            "standing": if conditional { "open" } else { "settled" },
                            "fields":{"rule_number":number}}),
                            )
                            .await
                            .unwrap_or_else(|e| panic!("rule {number} links to {edge}: {e:#}"));
                    }
                }
            }
            filed += 1;
        }
    }
    assert_eq!(filed, 225);
}

async fn file_as_one_prose(surface: &Surface, playbook: &Playbook) {
    let first_sid = create_threads(surface, playbook).await;
    surface
        .must(
            "add_entity",
            json!({"kind":"topic","handle":"all-rules",
        "name":"All rules","source":"user-named","sid":first_sid}),
        )
        .await
        .expect("the one prose subject exists");
    let mut filed = 0;
    for (phase_index, phase) in playbook.phases[..9].iter().enumerate() {
        let sid = if phase_index == 0 {
            first_sid.clone()
        } else {
            fresh_sitting(surface, phase.day.as_deref().expect("a day")).await
        };
        for row in phase
            .prompt
            .lines()
            .filter(|line| line.starts_with("Rule "))
        {
            let archived = row.contains("Retired rule; keep it reachable as history");
            let receipt = surface
                .must(
                    "capture",
                    json!({"subject":"topic:all-rules",
                "content":row,"provenance":"testimony","sid":sid}),
                )
                .await
                .unwrap_or_else(|e| panic!("the prose row is filed: {e:#}"));
            if archived {
                let address = receipt["address"].as_str().expect("the claim address");
                surface
                    .must(
                        "update_fact",
                        json!({"address":address,"status":"archived",
                    "sid":sid}),
                    )
                    .await
                    .expect("a retired row stays retired");
            }
            filed += 1;
        }
    }
    assert_eq!(filed, 225);
}

fn numbers_in(value: &Value, conditional_only: bool, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            if object.get("address").is_some()
                && object.get("status").and_then(Value::as_str) == Some("active")
            {
                if let Some(content) = object.get("content").and_then(Value::as_str) {
                    if content.starts_with("Rule ")
                        && (!conditional_only || content.contains("Conditional rule:"))
                    {
                        if let Some(number) = content
                            .strip_prefix("Rule ")
                            .and_then(|rest| rest.split_once(':'))
                            .map(|(number, _)| number)
                        {
                            if number.chars().all(|ch| ch.is_ascii_digit()) {
                                out.insert(number.to_string());
                            }
                        }
                    }
                }
            }
            for nested in object.values() {
                numbers_in(nested, conditional_only, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                numbers_in(item, conditional_only, out);
            }
        }
        _ => {}
    }
}

async fn walked_numbers(surface: &Surface, thread: &str) -> BTreeSet<String> {
    let read = surface
        .call(
            "recall",
            json!({"subject": format!("topic:{thread}"),
        "follow":{"shape":"connection","direction":"in","depth":1}}),
        )
        .await;
    let parsed: Value = serde_json::from_str(&read).expect("a graph answer");
    assert!(
        parsed["objects"][0]["connected_left_out"].is_null(),
        "the walk omitted a thread: {read}"
    );
    let mut subjects = vec![format!("topic:{thread}")];
    for object in parsed["objects"][0]["connected"]
        .as_array()
        .expect("the inbound walk")
    {
        subjects.push(object["id"].as_str().expect("a reached handle").to_string());
    }
    subjects.sort();
    subjects.dedup();
    let mut numbers = BTreeSet::new();
    for subject in subjects {
        let whole = surface
            .call("recall", json!({"subject":subject,"facts":true}))
            .await;
        let parsed: Value = serde_json::from_str(&whole).expect("a whole thread read");
        let facts = parsed["objects"][0]["facts"]
            .as_array()
            .expect("the thread facts");
        for fact in facts {
            if subject == format!("topic:{thread}")
                || fact["edge"]["object"] == format!("topic:{thread}")
            {
                numbers_in(fact, false, &mut numbers);
            }
        }
    }
    numbers
}

async fn conditional_numbers(surface: &Surface) -> BTreeSet<String> {
    let mut numbers = BTreeSet::new();
    let read = surface
        .call(
            "search",
            json!({"standing":"open","kind":"topic","limit":200}),
        )
        .await;
    let parsed: Value = serde_json::from_str(&read).expect("a standing search");
    numbers_in(&parsed, false, &mut numbers);
    numbers
}

async fn record_answers(surface: &Surface, playbook: &Playbook) {
    let work = [
        "work:pm-state",
        "work:conditional-rules",
        "work:loop-check-in",
        "work:bot-capacity",
        "work:rule-58-replacement",
        "work:image-attachments",
        "work:shipped-kind-fields",
    ];
    let threads = [
        "carried-state",
        "",
        "rhythms",
        "budgets",
        "memory",
        "",
        "kinds",
    ];
    for (at, phase) in playbook.phases[9..].iter().enumerate() {
        let sid = fresh_sitting(surface, phase.day.as_deref().expect("a day")).await;
        let numbers = if at == 1 {
            conditional_numbers(surface).await
        } else if at == 5 {
            BTreeSet::new()
        } else {
            walked_numbers(surface, threads[at]).await
        };
        if numbers.is_empty() {
            surface.must("capture", json!({"subject":work[at],
                "content":format!("nothing was found; looked at {} and its inbound links", if at == 5 { "the rule threads for image attachments" } else { threads[at] }),
                "provenance":"testimony","sid":sid})).await.expect("an empty answer is recorded");
        } else {
            for number in numbers {
                surface
                    .must(
                        "capture",
                        json!({"subject":work[at],"content":number,
                    "provenance":"testimony","sid":sid}),
                    )
                    .await
                    .unwrap_or_else(|e| panic!("answer {at} records {number}: {e:#}"));
            }
        }
    }
}

async fn judge(surface: &Surface) -> Vec<Outcome> {
    let boundaries = Vec::new();
    let observed = Observed {
        room: surface,
        boundaries: &boundaries,
    };
    let locks = expectations::for_playbook("rooms/decisions.md").expect("room locks");
    let mut outcomes = Vec::new();
    for lock in locks {
        outcomes.push(lock.check(&observed).await);
    }
    outcomes
}

#[tokio::test]
async fn every_check_fails_on_a_room_nobody_worked_in() {
    let (_room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    expectations::seed_for("rooms/decisions.md")
        .expect("the starting world builds")
        .furnish(&surface)
        .await
        .expect("the starting world applies");
    let outcomes = judge(&surface).await;
    assert!(
        !outcomes.is_empty(),
        "the room registered no checks, so a run would report a pass over an empty list",
    );
    for outcome in &outcomes {
        assert!(
            !outcome.held,
            "a furnished room nobody touched held a check: {} — {}",
            outcome.name, outcome.saying,
        );
    }
}

#[tokio::test]
async fn the_decision_room_is_solvable_through_the_served_surface() {
    let playbook =
        Playbook::read(&expectations::room_document("rooms/decisions.md")).expect("the room reads");
    let (_room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    expectations::seed_for("rooms/decisions.md")
        .expect("the starting world builds")
        .furnish(&surface)
        .await
        .expect("the starting world applies");
    file_register(&surface, &playbook).await;
    record_answers(&surface, &playbook).await;
    let outcomes = judge(&surface).await;
    let failed: Vec<String> = outcomes
        .iter()
        .filter(|outcome| !outcome.held)
        .map(|outcome| format!("{}: {}", outcome.name, outcome.saying))
        .collect();
    assert!(
        failed.is_empty(),
        "a correct filing did not open every lock: {failed:#?}"
    );
}

#[tokio::test]
async fn one_subject_of_prose_does_not_answer_the_working_state_or_capacity_questions() {
    let playbook =
        Playbook::read(&expectations::room_document("rooms/decisions.md")).expect("the room reads");
    let (_room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    expectations::seed_for("rooms/decisions.md")
        .expect("the starting world builds")
        .furnish(&surface)
        .await
        .expect("the starting world applies");
    file_as_one_prose(&surface, &playbook).await;
    let stored = surface
        .call(
            "search",
            json!({"query":"Real data waits", "subject":"topic:all-rules"}),
        )
        .await;
    assert!(
        stored.contains("Real data waits"),
        "the failed play must still have filed rule 241 on the single subject: {stored}"
    );
    record_answers(&surface, &playbook).await;
    let outcomes = judge(&surface).await;
    for question in ["question 1", "question 4"] {
        let answer = outcomes
            .iter()
            .find(|outcome| outcome.name.contains(question))
            .expect("the question has a lock");
        assert!(!answer.held, "{question} passed on one subject of prose");
    }
    for subject in ["work:pm-state", "work:bot-capacity"] {
        let read = surface
            .call("recall", json!({"subject":subject,"facts":true}))
            .await;
        let parsed: Value = serde_json::from_str(&read).expect("an answer read");
        assert!(
            parsed["objects"][0]["facts"][0]["content"].is_string(),
            "the negative play failed because it left no answer: {read}"
        );
    }
}

#[tokio::test]
async fn rule_answer_locks_read_details_across_records_and_reject_extra_numbers() {
    let (_room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    expectations::seed_for("rooms/decisions.md")
        .expect("the starting world builds")
        .furnish(&surface)
        .await
        .expect("the starting world applies");
    let lock = expectations::for_playbook("rooms/decisions.md")
        .expect("room locks")
        .into_iter()
        .find(|lock| lock.name().contains("question 1"))
        .expect("the first question has a lock");
    let boundaries = Vec::new();
    let observed = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    assert!(
        !lock.check(&observed).await.held,
        "timestamps supplied the answer"
    );
    let sid = fresh_sitting(&surface, "2026-11-01").await;
    surface.must("capture", json!({"subject":"work:pm-state", "content":"one answer", "details":"rule 241", "provenance":"testimony", "sid":sid})).await.expect("the first answer record");
    assert!(
        !lock.check(&observed).await.held,
        "one required rule was enough"
    );
    surface.must("capture", json!({"subject":"work:pm-state", "content":"275", "provenance":"testimony", "sid":sid})).await.expect("the second answer record");
    assert!(
        lock.check(&observed).await.held,
        "details and a second record did not count"
    );
    surface.must("capture", json!({"subject":"work:pm-state", "content":"241, 275, 129", "provenance":"testimony", "sid":sid})).await.expect("an overinclusive answer record");
    assert!(
        !lock.check(&observed).await.held,
        "an embedded unrelated number passed"
    );
}

#[tokio::test]
async fn no_lock_here_rests_on_a_needle_that_matches_somewhere_else() {
    let playbook =
        Playbook::read(&expectations::room_document("rooms/decisions.md")).expect("the room reads");
    let (_room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    expectations::seed_for("rooms/decisions.md")
        .expect("the starting world builds")
        .furnish(&surface)
        .await
        .expect("the starting world applies");
    file_register(&surface, &playbook).await;
    let summary = jojobot_exercise::lock::needle_summary(
        &surface,
        &jojobot_exercise::lock::locks_of(expectations::DECISIONS_ROOM),
    )
    .await;
    assert!(
        summary.nowhere.is_empty(),
        "a needle matched nowhere, so either its lock is failing or the walk could not read the \
         answer — and those are different: {:?}",
        summary.nowhere,
    );
    assert!(
        summary.findings.is_empty(),
        "a lock rests on a needle that matches somewhere else, with nothing else in that lock \
         only its own sitting could satisfy: {:?}",
        summary.findings,
    );
}

#[tokio::test]
async fn question_six_requires_an_answer_without_a_rule_reference() {
    let (_room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    expectations::seed_for("rooms/decisions.md")
        .expect("the starting world builds")
        .furnish(&surface)
        .await
        .expect("the starting world applies");
    let lock = expectations::for_playbook("rooms/decisions.md")
        .expect("room locks")
        .into_iter()
        .find(|lock| lock.name().contains("image-attachment question"))
        .expect("the sixth question has a lock");
    let boundaries = Vec::new();
    let observed = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    assert!(
        !lock.check(&observed).await.held,
        "an empty item was accepted"
    );
    let sid = fresh_sitting(&surface, "2026-11-11").await;
    surface
        .must(
            "capture",
            json!({"subject":"work:image-attachments",
                "content":"none found; reviewed all 26 threads on 2026-11-11",
                "provenance":"testimony","sid":sid}),
        )
        .await
        .expect("the absence is recorded");
    assert!(
        lock.check(&observed).await.held,
        "an answer with a thread count and date was refused"
    );
    surface
        .must(
            "capture",
            json!({"subject":"work:image-attachments","content":"rule 26 covers it",
                "provenance":"testimony","sid":sid}),
        )
        .await
        .expect("a wrong rule number is recorded");
    assert!(
        !lock.check(&observed).await.held,
        "a rule number beside the absence was accepted"
    );
}
