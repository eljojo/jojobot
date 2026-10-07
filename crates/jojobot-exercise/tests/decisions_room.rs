use jojobot_exercise::expectations;
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{
    Boundary, Observed, Outcome, boundary, boundary_asking, boundary_names, phase_end_queries,
};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::PathBuf;

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

/// **The short pointer a "bears on" citation files, never the rule's own
/// text again.** A citation is a thought: an active, connection-edged claim
/// on the rule's own thread, and a thought is enforceable only because its
/// body is too short to restate what it points at
/// (`pm/feature-briefs/carried-state.md`). The rule's full text is filed
/// once, on its own thread, by the ordinary claim just above this one in
/// `file_register`; a citation that repeated it would be exactly the
/// restatement the cap exists to refuse. This names the rule by number and
/// says where the substance already lives.
fn bears_on_pointer(number: &str, thread: &str) -> String {
    // **`Rule {number}: `, not restated after the colon.** `numbers_in`
    // (this file's own walker) reads a rule's number off exactly this
    // prefix on every fact it visits, citations included — the same shape
    // the primary filing already carries, kept here so the walk still
    // finds a citation without needing the rule's own text to do it.
    format!("Rule {number}: see topic:{thread} for the rule itself.")
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
                                "content": bears_on_pointer(number, &thread),
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

/// 🚨 **Every "bears on" citation the worked solution files is a pointer,
/// never the rule's own text again.** The default thought cap is 200
/// characters (`jojobot_domain::memory::DEFAULT_THOUGHT_BODY_CAP`, mirrored
/// here rather than depended on — this crate tests only the served
/// surface); a citation over that would be refused the moment a real run
/// tried to file it, so this proves the worked solution never asks the
/// server to do that in the first place.
#[tokio::test]
async fn every_bears_on_citation_the_worked_solution_files_is_under_the_body_cap() {
    const THOUGHT_BODY_CAP: usize = 200;

    let playbook =
        Playbook::read(&expectations::room_document("rooms/decisions.md")).expect("the room reads");
    let seed = filed_register_seed().await;
    let (_room, surface) =
        Room::open_with_client_from(&server_binary().expect("a jojobot binary"), &seed)
            .await
            .expect("a room");

    // The same rule rows `file_register` reads, narrowed to the threads that
    // carry at least one "bears on" citation — every topic a citation could
    // possibly have landed on.
    let mut threads_with_citations = BTreeSet::new();
    for phase in &playbook.phases[..9] {
        for row in phase
            .prompt
            .lines()
            .filter(|line| line.starts_with("Rule "))
        {
            let (_, rest) = row
                .strip_prefix("Rule ")
                .expect("a rule")
                .split_once(": ")
                .expect("a numbered rule");
            if rest.contains(" This also bears on ") {
                let thread = rest
                    .rsplit_once(" Thread: ")
                    .and_then(|(_, tail)| tail.split('.').next())
                    .unwrap_or("what jojobot is")
                    .replace(' ', "-");
                threads_with_citations.insert(thread);
            }
        }
    }
    assert!(
        !threads_with_citations.is_empty(),
        "the room's own fixture carries no bears-on citations — this case would prove nothing"
    );

    let mut checked = 0;
    for thread in &threads_with_citations {
        let read = surface
            .call(
                "recall",
                json!({"subject": format!("topic:{thread}"), "facts": true}),
            )
            .await;
        let parsed: Value = serde_json::from_str(&read).expect("an answer read");
        let facts = parsed["objects"][0]["facts"]
            .as_array()
            .unwrap_or_else(|| panic!("topic:{thread} answered with no facts array: {read}"));
        for fact in facts {
            if fact["edge"]["type"] == "relatedTo" {
                let content = fact["content"].as_str().expect("a citation's own content");
                assert!(
                    content.chars().count() <= THOUGHT_BODY_CAP,
                    "a bears-on citation on topic:{thread} is {} characters, over the default \
                     cap of {THOUGHT_BODY_CAP} — the worked solution must file a pointer, not \
                     the rule's own text again: {content:?}",
                    content.chars().count()
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked > 0,
        "no citation was found to check — the recall query or the edge filter is wrong"
    );
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

static FILED_REGISTER: tokio::sync::OnceCell<PathBuf> = tokio::sync::OnceCell::const_new();
static FILED_PROSE: tokio::sync::OnceCell<PathBuf> = tokio::sync::OnceCell::const_new();

/// **The store `file_register` leaves behind, built once per test binary.**
/// Three tests below need "the register already filed" as their starting
/// state and do not test the filing itself — this is what lets each start
/// from a copy of it instead of redoing several hundred sequential writes.
async fn filed_register_seed() -> PathBuf {
    FILED_REGISTER
        .get_or_try_init(|| async {
            let binary = server_binary().expect("a jojobot binary");
            Room::snapshot_after(&binary, |surface| async move {
                expectations::seed_for("rooms/decisions.md")?
                    .furnish(&surface)
                    .await?;
                let playbook = Playbook::read(&expectations::room_document("rooms/decisions.md"))?;
                file_register(&surface, &playbook).await;
                Ok(())
            })
            .await
        })
        .await
        .expect("the filed-register seed builds")
        .clone()
}

/// The store `file_as_one_prose` leaves behind, built the same way.
async fn filed_prose_seed() -> PathBuf {
    FILED_PROSE
        .get_or_try_init(|| async {
            let binary = server_binary().expect("a jojobot binary");
            Room::snapshot_after(&binary, |surface| async move {
                expectations::seed_for("rooms/decisions.md")?
                    .furnish(&surface)
                    .await?;
                let playbook = Playbook::read(&expectations::room_document("rooms/decisions.md"))?;
                file_as_one_prose(&surface, &playbook).await;
                Ok(())
            })
            .await
        })
        .await
        .expect("the filed-prose seed builds")
        .clone()
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

async fn judge(surface: &Surface, boundaries: &[Boundary]) -> Vec<Outcome> {
    let observed = Observed {
        room: surface,
        boundaries,
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
    let outcomes = judge(&surface, &[]).await;
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
    // **The filing locks are read as the filing sittings left the room**, so
    // the boundary is taken after the ninth filing and before any question
    // sitting answers.
    let names = boundary_names(&playbook);
    let locks = expectations::for_playbook("rooms/decisions.md").expect("room locks");
    let before = boundary(&surface, &names[8]).await;
    file_register(&surface, &playbook).await;
    let filed = boundary_asking(&surface, &names[9], &phase_end_queries(&locks, "Phase 9")).await;
    record_answers(&surface, &playbook).await;
    let outcomes = judge(&surface, &[before, filed]).await;
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

/// 🚨 **The one guard nothing else here can be.** A copy of the filed
/// register reads identically to a live filing by design — proven above —
/// so no read against the store, however this test's own assertions are
/// worded, can tell whether `the_decision_room_is_solvable_through_the_served_surface`
/// actually drove the filing itself or borrowed the cached seed a different
/// test built. Swapping its own `Room::open_with_client` and `file_register`
/// for `filed_register_seed`/`open_with_client_from` was tried by hand: the
/// test still passed, because the content is the same either way. The only
/// place that swap is visible is the source, so this reads it.
///
/// **Read once, checked twice.** The one call this test must keep proves it
/// still drives the filing; the one call it must never gain proves it has
/// not started borrowing the seed instead.
#[test]
fn the_solvable_test_still_files_live_rather_than_borrowing_the_cached_seed() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/decisions_room.rs");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("this test's own file at {} reads: {e}", path.display()));
    let marker = "async fn the_decision_room_is_solvable_through_the_served_surface";
    let start = source
        .find(marker)
        .expect("the solvable test is still named this — update the marker if it was renamed");
    let body = &source[start..];
    let end = body
        .find("\n}\n")
        .expect("the function has a closing brace on its own line");
    let body = &body[..end];
    assert!(
        body.contains("file_register(&surface, &playbook)"),
        "the solvable test no longer files live through the served surface, so it proves \
         nothing beyond what the cached seed already proved once when some other test built it"
    );
    assert!(
        !body.contains("filed_register_seed"),
        "the solvable test now borrows the cached seed instead of filing live"
    );
}

/// **A filing lock reads the room as the filing sittings left it.** The locks
/// that ask whether a rule was filed are searches, and a search of the
/// finished room also reaches the answers the question sittings wrote — so a
/// sitting that quotes a rule in an answer opens the lock for a rule nobody
/// filed.
///
/// Nothing is filed here. A later sitting quotes the rules in its answer, which
/// is the only thing that can hold these locks open. Paired with the solvable
/// case, where the same locks hold on a room that WAS filed.
#[tokio::test]
async fn a_filing_lock_is_not_opened_by_a_later_answer_that_quotes_the_rule() {
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
    let names = boundary_names(&playbook);
    let locks = expectations::for_playbook("rooms/decisions.md").expect("room locks");
    let before = boundary(&surface, &names[8]).await;
    let ended = boundary_asking(&surface, &names[9], &phase_end_queries(&locks, "Phase 9")).await;

    // A question sitting answers by quoting the rules whose filing the locks
    // ask about.
    let sid = fresh_sitting(&surface, "2026-11-01").await;
    surface
        .must(
            "capture",
            json!({"subject":"work:pm-state","provenance":"testimony","sid":sid,
                "content":"Real data waits on the paid tests, paid tests; hard capacity it cannot \
                           raise, capacity; Capacity may be borrowed once, repaying; check-in ran, \
                           cycle is consumed; cadence is always time, measurement; the label still \
                           layered on top; claim written wrong in this session, visible correction"}),
        )
        .await
        .expect("the answer quotes the rules");
    // …and a retired answer quotes the one whose filing is asked of as
    // archived, which a search for an active claim never reaches.
    let retired = surface
        .must(
            "capture",
            json!({"subject":"work:pm-state","provenance":"testimony","sid":sid,
                "content":"Killed at the operator's word, 288 replaces it"}),
        )
        .await
        .expect("the retired answer quotes the rule");
    surface
        .must(
            "update_fact",
            json!({"address":retired["address"],"status":"archived","sid":sid}),
        )
        .await
        .expect("the answer is retired");

    let outcomes = judge(&surface, &[before, ended]).await;
    let filing: Vec<&Outcome> = outcomes
        .iter()
        .filter(|outcome| outcome.name.contains("was not filed as"))
        .collect();
    assert_eq!(
        filing.len(),
        8,
        "the eight filing locks: {:?}",
        outcomes.iter().map(|o| &o.name).collect::<Vec<_>>()
    );
    for outcome in filing {
        assert!(
            !outcome.held,
            "a rule nobody filed read as filed because a later answer quotes it: {} — {}",
            outcome.name, outcome.saying,
        );
    }
}

#[tokio::test]
async fn one_subject_of_prose_does_not_answer_the_working_state_or_capacity_questions() {
    let playbook =
        Playbook::read(&expectations::room_document("rooms/decisions.md")).expect("the room reads");
    let seed = filed_prose_seed().await;
    let (_room, surface) =
        Room::open_with_client_from(&server_binary().expect("a jojobot binary"), &seed)
            .await
            .expect("a room");
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
    let outcomes = judge(&surface, &[]).await;
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
    let seed = filed_register_seed().await;
    let (_room, surface) =
        Room::open_with_client_from(&server_binary().expect("a jojobot binary"), &seed)
            .await
            .expect("a room");
    let summary = jojobot_exercise::lock::needle_summary(
        &surface,
        &jojobot_exercise::lock::locks_of(expectations::DECISIONS_ROOM),
        &[],
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

/// 🚨 **A copy of the filed register is the same room, not merely a room with
/// the same schema.** Never served directly: the cached seed is shared by
/// every test in this binary, so serving it live here and copying it
/// elsewhere at the same time would race a live server's own writes against
/// a plain filesystem copy. Two independent copies of the one seed prove the
/// same property — if a copy were not faithful, two of them would not read
/// alike either — without ever touching the shared original.
///
/// **Three reads, the ones a precondition-only test actually depends on.** A
/// recall of one filed thing, a search over what filing wrote, and — the one
/// a schema-only proof would never need — the session list, because filing
/// leaves sessions behind and a copy carries them. Booting on each copy uses
/// `fresh_sitting`, the same call the filing itself makes, so a leftover
/// active session from the filing run is resumed past rather than met as an
/// ambiguous choice — proving directly that no test which boots a bot on a
/// copy meets that leftover and reads it wrong: this copy's own list of
/// prior runs matches the other copy's, and only the freshly booted entry
/// (excluded from the comparison below) differs, in its `sid` alone.
#[tokio::test]
async fn a_copy_of_the_filed_register_reads_the_same_as_another_copy() {
    let binary = server_binary().expect("a jojobot binary");
    let seed = filed_register_seed().await;
    let (_room_a, a) = Room::open_with_client_from(&binary, &seed)
        .await
        .expect("the first copy opens");
    let (_room_b, b) = Room::open_with_client_from(&binary, &seed)
        .await
        .expect("the second copy opens");

    let recall_a = a
        .call("recall", json!({"subject":"work:pm-state","facts":true}))
        .await;
    let recall_b = b
        .call("recall", json!({"subject":"work:pm-state","facts":true}))
        .await;
    assert_eq!(
        recall_a, recall_b,
        "a recall of a filed thing reads differently between two copies of the same seed"
    );

    let search_a = a
        .call(
            "search",
            json!({"query":"jojobot tracks what each session is doing"}),
        )
        .await;
    let search_b = b
        .call(
            "search",
            json!({"query":"jojobot tracks what each session is doing"}),
        )
        .await;
    assert_eq!(
        search_a, search_b,
        "a search over what filing wrote reads differently between two copies of the same seed"
    );

    let playbook =
        Playbook::read(&expectations::room_document("rooms/decisions.md")).expect("the room reads");
    let day = playbook.phases[0]
        .day
        .as_deref()
        .expect("phase 1 names a day");
    let sid_a = fresh_sitting(&a, day).await;
    let sid_b = fresh_sitting(&b, day).await;

    let runs_a = a
        .must("list_runs", json!({"sid": sid_a}))
        .await
        .expect("this copy's runs");
    let runs_b = b
        .must("list_runs", json!({"sid": sid_b}))
        .await
        .expect("the other copy's runs");
    let without_sid = |runs: &Value| -> Vec<(Value, Value)> {
        runs["runs"]
            .as_array()
            .expect("a runs array")
            .iter()
            .map(|run| (run["state"].clone(), run["working_on"].clone()))
            .collect()
    };
    assert_eq!(
        without_sid(&runs_a),
        without_sid(&runs_b),
        "booting on a copy met a leftover session from the filing run and read it differently \
         than the other copy did — leftover run lists: {runs_a} / {runs_b}"
    );
}
