//! "I am running a launch and an app for a client, and I keep losing why."
//!
//! A project's work is dates that move, decisions made in a room, questions
//! nobody has asked yet and things somebody worked out from what was said.
//! **Each has one route the engine accepts and a convenient wrong one**, and
//! the `projects` skill names the routes. A session that was told nothing
//! finds the skill from the boot's index and from one line on its first write
//! about a project, and every tip in it is a call that lands here.
//!
//! The roster names are the cast: a campaign and an app, three people, one
//! piece of work.

use serde_json::json;

use super::dsl::Story;

/// Whether an answer carries a one-line teaching that names the skill. Read from
/// the answer's own `teaching` list, which is where a teaching rides.
fn names_the_skill(answer: &super::dsl::Answer) -> bool {
    answer.json()["teaching"].as_array().is_some_and(|lines| {
        lines
            .iter()
            .any(|l| l.as_str().is_some_and(|l| l.contains("projects")))
    })
}

/// **The skill is listed with what it is for, ships no body in the index, and
/// is fetched by name.** What it says names routes this surface really takes.
#[tokio::test]
async fn a_session_told_nothing_finds_the_projects_skill_from_the_boot() {
    let story = Story::begin("bot:gamma").await;
    let (booted, _s) = story.full_boot().await;

    let skills = booted["skills"].as_array().expect("the boot lists skills");
    let projects = skills
        .iter()
        .find(|s| s["name"] == "projects")
        .unwrap_or_else(|| panic!("a cold session is told this skill exists: {booted}"));
    let when = projects["when_to_use"].as_str().expect("what it is for");
    for named in ["project", "decisions", "questions"] {
        assert!(
            when.contains(named),
            "the index says what a session can SEE it is doing ({named}): {when}",
        );
    }
    assert!(
        projects.get("body").is_none(),
        "the index ships no bodies, so knowing it exists is free",
    );

    let body = story.skill("projects").await.to_string();
    for route in [
        "history",
        "history_most",
        "derived_from",
        "promised_by",
        "regarding",
    ] {
        assert!(
            body.contains(route),
            "the procedure does not name {route}, which a tip rests on: {body}",
        );
    }

    story.finish().await;
}

/// **The first write about a project names the skill, in one line, once.**
///
/// A write about a person does not spend it, so the line lands on the call
/// that raised the question. The second write about a project is silent, and a
/// different session is told again — the ledger is per session.
#[tokio::test]
async fn the_first_write_about_a_project_names_the_skill_once_per_session() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    // Something that is not a project's work does not spend it.
    s.add("person:milhouse", "Milhouse").await;
    let about_a_person = s
        .call(
            "capture",
            json!({"subject": "person:milhouse", "content": "owns the film",
                   "provenance": "testimony"}),
        )
        .await;
    assert!(
        !names_the_skill(&about_a_person),
        "a write about a person named the projects skill: {}",
        about_a_person.raw(),
    );

    // The creation of a project is the first write on one.
    let created = s
        .call(
            "add_entity",
            json!({"kind": "project", "handle": "atlas", "name": "The Campaign",
                   "source": "user-named"}),
        )
        .await;
    assert!(
        names_the_skill(&created),
        "the first write about a project did not name the skill: {}",
        created.raw()
    );

    // So the next write about a project is silent: one line per session.
    let again = s
        .call(
            "capture",
            json!({"subject": "project:atlas", "content": "launch is fixed",
                   "provenance": "testimony"}),
        )
        .await;
    assert!(
        !names_the_skill(&again),
        "the skill was named a second time in one session: {}",
        again.raw(),
    );
    // A second session is told again, on a capture this time: the ledger is
    // per session, and a capture about a project is a write about one.
    let later = story.session_in("UTC", Some("new")).await;
    let first = later
        .call(
            "capture",
            json!({"subject": "project:atlas", "content": "needs a cut",
                   "provenance": "testimony"}),
        )
        .await;
    assert!(names_the_skill(&first), "{}", first.raw());
    story.finish().await;
}

/// **A slip is a new due day, and how many times it moved is a history read.**
#[tokio::test]
async fn how_many_times_a_milestone_moved_is_read_from_the_keys_history() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:atlas", "The Campaign").await;
    for day in ["2026-03-01", "2026-03-15", "2026-04-06"] {
        s.event_with(
            "project:atlas",
            &format!("the copy is now due {day}"),
            json!({"copy_due": day}),
            &[],
        )
        .await;
    }

    let history = s
        .call(
            "recall",
            json!({"subject": "project:atlas", "history": "copy_due", "facts": false}),
        )
        .await;
    for day in ["2026-03-01", "2026-03-15", "2026-04-06"] {
        history.says(day);
    }
    story.finish().await;
}

/// **A decision is the operator's words, and the option turned down is a claim
/// of its own that names whose it was.**
#[tokio::test]
async fn a_decision_is_kept_in_the_operators_words_with_the_option_turned_down() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:visa", "The App").await;
    s.add("person:homer", "Homer").await;
    s.add("person:martin", "Martin").await;

    s.event(
        "project:visa",
        "Homer: we sync the menu first, an order for a menu we have not synced is an order for nothing",
    )
    .await;
    s.event_with(
        "project:visa",
        "a hard-coded menu was turned down; it was @person:martin's option",
        json!({"state": "turned down"}),
        &[],
    )
    .await;

    // A guess of the assistant's own about the same menu: not the operator's
    // word, so the testimony filter must leave it out.
    s.guess("project:visa", "pagination will matter once the menu grows")
        .await;

    // Backed as the operator's word, not as a guess.
    let backed = s
        .call(
            "search",
            json!({"query": "menu", "provenance": "testimony", "limit": 20}),
        )
        .await;
    backed
        .says("sync the menu first")
        .says("hard-coded")
        .never_says("pagination");
    // The same query without the filter finds the guess, so its absence above
    // is the filter's doing and not an empty result.
    s.call("search", json!({"query": "menu", "limit": 20}))
        .await
        .says("pagination");

    // The option turned down is read back by its state, asked of the record: the
    // answer carries the records that answered, and the operator's other words
    // do not.
    let turned_down = s
        .call(
            "recall",
            json!({"fields": [{"key": "state", "value": "turned down", "scope": "record"}],
                   "facts": true}),
        )
        .await;
    // And the option names whose it was, in the handle the words were written
    // with.
    turned_down
        .says("hard-coded")
        .says("@person:martin")
        .never_says("sync the menu first");
    story.finish().await;
}

/// **Your own conclusion is two records**: what they said, and what you worked
/// out from it, pointing back.
#[tokio::test]
async fn a_conclusion_from_a_quote_is_an_inference_that_points_at_the_quote() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:visa", "The App").await;
    let quote = s
        .event(
            "project:visa",
            "the basement location has terrible reception",
        )
        .await;
    let conclusion = s
        .guess_from(
            "project:visa",
            "orders must work offline, because of the reception",
            &quote,
        )
        .await;
    // A conclusion worked out from a different quote: it must not come back as
    // built on the first.
    let other = s
        .event("project:visa", "the staff want a printed receipt")
        .await;
    s.guess_from(
        "project:visa",
        "the till must print a slip, because of the staff",
        &other,
    )
    .await;

    // The lineage reads both ways: from the quote, what was built on it.
    let built = s
        .call("recall", json!({"built_on": quote, "facts": true}))
        .await;
    built.says("orders must work offline");
    // The lineage block holds the one conclusion, not the other. The answer
    // also lists the project's every claim further down, so the needle is read
    // from the block alone.
    built.number("/built_on/count", 1);
    assert!(
        !built.json()["built_on"].to_string().contains("slip"),
        "a conclusion worked out from another quote came back as built on this one: {}",
        built.raw()
    );
    // And from the conclusion, the quote it was worked out from.
    s.call("recall", json!({"subject": "project:visa", "facts": true}))
        .await
        .claim(&conclusion)
        .says(&quote);
    story.finish().await;
}

/// **A pause until a day, on something with no loop, is a promise regarding
/// it** — owed from the day and not before.
#[tokio::test]
async fn a_pause_on_a_thing_with_no_loop_is_a_promise_regarding_it() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:atlas", "The Campaign").await;
    s.add("work:handcart", "The Film").await;
    s.call(
        "add_entity",
        json!({"kind": "promise", "handle": "return-the-wrench",
               "name": "Leave the film alone", "source": "user-named",
               "parent": "project:atlas"}),
    )
    .await;
    s.event_with(
        "promise:return-the-wrench",
        "no questions about the film until the day",
        json!({"promised_by": "2026-11-20", "regarding": "work:handcart"}),
        &[],
    )
    .await;

    let owed = |day: &'static str| {
        s.call(
            "recall",
            json!({"fields": [{"key": "due_on"}], "overdue": {"as_of": day}}),
        )
    };
    owed("2026-11-12").await.never_says("work:handcart");
    owed("2026-11-21").await.says("work:handcart");
    story.finish().await;
}

/// **A piece of work that is done is not owed, and the day it was due stays
/// on it.** The wiring had to be decided by the first of January; it was left
/// at `now` and is late, then it is finished and nothing is late.
#[tokio::test]
async fn a_piece_of_work_that_is_done_is_no_longer_owed() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:atlas", "The Campaign").await;
    s.add_under("project:atlas", "work:phi", "The Wiring").await;
    s.event_with(
        "work:phi",
        "the wiring is decided by the first of January",
        json!({"decide_by": "2026-01-01", "status": "now"}),
        &[],
    )
    .await;

    let owed = || {
        s.call(
            "recall",
            json!({"fields": [{"key": "due_on"}], "overdue": {"as_of": "2026-06-01"}}),
        )
    };
    owed().await.says("work:phi");
    // The moment the read above works from is stored on the work itself.
    s.call("recall", json!({"subject": "work:phi"}))
        .await
        .says("\"due_on\":\"2026-01-01\"");

    s.event_with(
        "work:phi",
        "the wiring is finished",
        json!({"status": "done"}),
        &[],
    )
    .await;
    owed().await.never_says("work:phi");
    // And finishing the work takes the stored moment off it, whichever verb
    // wrote the status.
    s.call("recall", json!({"subject": "work:phi"}))
        .await
        .says("\"decide_by\":\"2026-01-01\"")
        .never_says("\"due_on\"");
    // The day it was due is still on the record, so why it was due is not lost.
    s.call("recall", json!({"subject": "work:phi"}))
        .await
        .says("\"decide_by\":\"2026-01-01\"");
    story.finish().await;
}

/// Where an open question is filed: a `work` thing of its own, so that it folds
/// its own `state` and a key filter on `state` finds every question whatever
/// the others say.
fn home_of(question: &str) -> String {
    format!("work:{question}")
}

/// **An open question is a piece of work of its own, under its project, and it
/// carries its state and the thing it blocks**, so a later read finds every
/// question drafted and never asked, beside one asked after it on the same
/// project.
#[tokio::test]
async fn an_open_question_carries_its_state_and_what_it_blocks() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:visa", "The App").await;
    s.add("work:handcart", "The Film").await;
    // The film is next work, the one status read a question stays out of.
    s.event_with(
        "work:handcart",
        "the film is next",
        json!({"status": "next"}),
        &[],
    )
    .await;
    // Two questions on the SAME project: the older drafted, the newer asked.
    for (question, name, words, state) in [
        (
            "phi",
            "Payment provider",
            "which payment provider do we use? drafted, not yet asked",
            "drafted",
        ),
        (
            "sigma",
            "Refund rule",
            "which refund rule do we apply? asked, answer pending",
            "asked",
        ),
    ] {
        s.add_under("project:visa", &home_of(question), name).await;
        s.event_with(
            &home_of(question),
            words,
            json!({"state": state, "blocks": "work:handcart"}),
            &[],
        )
        .await;
    }

    let drafted = s
        .call(
            "recall",
            json!({"fields": [{"key": "state", "value": "drafted"}], "facts": true}),
        )
        .await;
    drafted
        .says("payment provider")
        .says("work:handcart")
        .never_says("refund");
    // The asked one is there to be found by its own state, so its absence
    // above is the filter's doing.
    let asked = s
        .call(
            "recall",
            json!({"fields": [{"key": "state", "value": "asked"}], "facts": true}),
        )
        .await;
    asked.says("refund").never_says("payment provider");
    // A question is not next work: it carries no `status`, so the read for what
    // is next finds the film and neither question.
    let next = s
        .call(
            "recall",
            json!({"kind": "work", "fields": [{"key": "status", "value": "next"}]}),
        )
        .await;
    next.says("work:handcart")
        .never_says("work:phi")
        .never_says("work:sigma");
    story.finish().await;
}

/// **Where a piece of work stands is kept on the keys the skill names**, so
/// what is next, what waits on whom and what stands on what are each a read of
/// one key, and a status outside the project's list lands with the list named.
#[tokio::test]
async fn where_a_piece_of_work_stands_is_kept_on_the_keys_the_skill_names() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:atlas", "The Campaign").await;
    s.add_under("project:atlas", "work:phi", "Phi").await;
    s.add_under("project:atlas", "work:first-mix", "First mix")
        .await;
    s.add_under("project:atlas", "work:sigma", "Sigma").await;
    s.add("person:lisa", "Lisa").await;
    s.event_with(
        "project:atlas",
        "the board's columns",
        json!({"columns": "inbox, someday, next, now, waiting, done"}),
        &[],
    )
    .await;
    s.event_with(
        "work:phi",
        "ready to start",
        json!({"status": "next", "owner": "person:lisa"}),
        &[],
    )
    .await;
    s.event_with("work:first-mix", "under way", json!({"status": "now"}), &[])
        .await;
    s.event_with(
        "work:sigma",
        "cannot start yet",
        json!({
            "status": "waiting", "waiting_on": "person:lisa",
            "depends_on": "work:phi, work:first-mix",
        }),
        &[],
    )
    .await;

    let next = s
        .call(
            "recall",
            json!({"kind": "work", "fields": [{"key": "status", "value": "next"}]}),
        )
        .await;
    next.says("work:phi");
    next.never_says("work:sigma");
    next.never_says("work:first-mix");

    let waiting = s
        .call(
            "recall",
            json!({"kind": "work", "fields": [{"key": "waiting_on"}]}),
        )
        .await;
    waiting.says("work:sigma").says("person:lisa");
    // Only sigma holds the key. Its dependency list names the first mix, so the
    // handle is in the answer, but the first mix is not an object in it.
    assert_eq!(waiting.json()["count"], 1, "{}", waiting.json());

    let stands_on_phi = s
        .call(
            "recall",
            json!({
                "subject": "work:phi",
                "follow": {"relation": "depends_on", "direction": "in"},
            }),
        )
        .await;
    stands_on_phi.says("work:sigma");

    let landed = s
        .call(
            "capture",
            json!({
                "subject": "work:phi", "content": "where it stands",
                "provenance": "testimony", "fields": {"status": "backlog"},
            }),
        )
        .await;
    landed.says("inbox");
    s.call("recall", json!({"subject": "work:phi"}))
        .await
        .says("\"status\":\"backlog\"");
    story.finish().await;
}

/// **"Considering" is the operator's word, and nothing refuses it.** On a fresh
/// project with no parent it lands, and the receipt names the words the project
/// holds, says the word counts as not finished, and names the one write that
/// binds it. After the instance maps it to a shipped word the receipt is quiet,
/// the thing still falls due, and `done` still finishes.
#[tokio::test]
async fn an_operators_own_status_word_lands_and_is_named_until_it_is_mapped() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:atlas", "The Campaign").await;

    // ── lands, and the receipt names what governs the word ──────────────────
    let landed = s
        .call(
            "capture",
            json!({
                "subject": "project:atlas", "content": "weighing it up",
                "provenance": "testimony",
                "fields": {"status": "considering", "decide_by": "2026-01-01"},
            }),
        )
        .await;
    let note = landed.json()["status_word_note"]
        .as_str()
        .unwrap_or_else(|| panic!("a word outside the set is named: {}", landed.json()))
        .to_string();
    for governing in ["someday", "next", "now", "waiting", "done"] {
        assert!(
            note.contains(governing),
            "the note names {governing}: {note}"
        );
    }
    assert!(
        note.contains("considering") && note.contains("columns") && note.contains("status_word/"),
        "the note names the word and the two writes that bind it: {note}"
    );
    s.call("recall", json!({"subject": "project:atlas"}))
        .await
        .says("\"status\":\"considering\"");

    // ── an unmapped word is not finished, so the thing is owed ──────────────
    let owed = || {
        s.call(
            "recall",
            json!({"fields": [{"key": "due_on"}], "overdue": {"as_of": "2026-06-01"}}),
        )
    };
    owed().await.says("project:atlas");

    // ── the instance maps it: the receipt goes quiet, the thing still falls due
    s.event_with(
        "topic:instance",
        "what the operator's words mean",
        json!({"status_word/considering": "someday", "status_word/closed": "done"}),
        &[],
    )
    .await;
    let mapped = s
        .call(
            "capture",
            json!({
                "subject": "project:atlas", "content": "still weighing it up",
                "provenance": "testimony", "fields": {"status": "considering"},
            }),
        )
        .await;
    assert!(
        mapped.json().get("status_word_note").is_none(),
        "a mapped word is named no more: {}",
        mapped.json()
    );
    owed().await.says("project:atlas");

    // ── a word mapped to done finishes it, and a shipped word still does ───
    s.event_with(
        "project:atlas",
        "closed out",
        json!({"status": "closed"}),
        &[],
    )
    .await;
    owed().await.never_says("project:atlas");
    // The word that means done takes the stored moment off, as `done` does.
    s.call("recall", json!({"subject": "project:atlas"}))
        .await
        .never_says("\"due_on\"");
    s.event_with(
        "project:atlas",
        "reopened",
        json!({"status": "considering"}),
        &[],
    )
    .await;
    owed().await.says("project:atlas");
    s.event_with("project:atlas", "over", json!({"status": "done"}), &[])
        .await;
    owed().await.never_says("project:atlas");

    // ── a mapping written AFTER the status finishes the thing when it is read ─
    //
    // The stored due moment is still on the project, because no write moved
    // it; only the read can leave the project out.
    s.event_with("project:atlas", "parked", json!({"status": "wrapped"}), &[])
        .await;
    owed().await.says("project:atlas");
    s.event_with(
        "topic:instance",
        "wrapped means done",
        json!({"status_word/wrapped": "done"}),
        &[],
    )
    .await;
    owed().await.never_says("project:atlas");
    story.finish().await;
}

/// **A word the instance lists is quiet even before it is mapped**, and a shipped
/// word cannot be given another meaning: the bad mapping fails, names the way
/// forward, leaves the instance as it was, and the good mapping still lands.
#[tokio::test]
async fn a_shipped_word_cannot_be_remapped_and_the_refusal_leaves_the_instance_alone() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:atlas", "The Campaign").await;
    s.event_with(
        "topic:instance",
        "the operator's words",
        json!({"columns": "considering"}),
        &[],
    )
    .await;

    // The listed word lands with no note: the accepted set holds it. The
    // instance's list names only the operator's word and does not repeat the
    // shipped five.
    let listed = s
        .call(
            "capture",
            json!({
                "subject": "project:atlas", "content": "weighing it up",
                "provenance": "testimony", "fields": {"status": "considering"},
            }),
        )
        .await;
    assert!(
        listed.json().get("status_word_note").is_none(),
        "a word the instance lists needs no note: {}",
        listed.json()
    );

    // A project's own list that drops a shipped word is still refused.
    s.refused(
        "capture",
        json!({
            "subject": "project:atlas", "content": "a shorter list",
            "provenance": "testimony", "fields": {"columns": "someday, next, now, done"},
        }),
    )
    .await
    .says("waiting")
    .says("\"wrote\":false");

    let before = s.recall("topic:instance").await.json();
    s.refused(
        "capture",
        json!({
            "subject": "topic:instance", "content": "done means something else",
            "provenance": "testimony", "fields": {"status_word/done": "someday"},
        }),
    )
    .await
    .says("status_word/")
    .says("done")
    .says("\"wrote\":false");
    assert_eq!(s.recall("topic:instance").await.json(), before);
    s.event_with(
        "topic:instance",
        "closed means done",
        json!({"status_word/closed": "done"}),
        &[],
    )
    .await;
    s.recall("topic:instance")
        .await
        .says("\"status_word/closed\":\"done\"");
    story.finish().await;
}

/// **A long history read is elided, and raising the limit reaches the far
/// end** — so a retrospective is read to the end before it is concluded from.
#[tokio::test]
async fn a_retrospective_raises_the_limit_before_it_concludes() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:atlas", "The Campaign").await;
    for week in 1..=25 {
        s.event_with(
            "project:atlas",
            &format!("status for week {week}"),
            json!({"health": format!("week-{week:02}")}),
            &[],
        )
        .await;
    }

    let cut = s
        .call(
            "recall",
            json!({"subject": "project:atlas", "history": "health"}),
        )
        .await;
    cut.never_says("week-01");
    let whole = s
        .call(
            "recall",
            json!({"subject": "project:atlas", "history": "health", "history_most": 30}),
        )
        .await;
    whole.says("week-01").says("week-25");
    story.finish().await;
}

/// **An answer the operator asks to have kept is written down**, and a later
/// session reads it from the record and not from the conversation.
#[tokio::test]
async fn an_answer_the_operator_asked_to_keep_is_there_for_a_later_session() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:atlas", "The Campaign").await;
    s.event_with(
        "project:atlas",
        "the real budget was 150 thousand",
        json!({"budget_truth": "150"}),
        &[],
    )
    .await;

    let later = story.session_in("UTC", Some("new")).await;
    later
        .call(
            "recall",
            json!({"fields": [{"key": "budget_truth"}], "facts": true}),
        )
        .await
        .says("\"budget_truth\":\"150\"");
    story.finish().await;
}

/// **Every verb that can set a status names a word outside the set**, and none
/// of them names a word inside it: creating a work item with a status, editing
/// it through the entity, and editing the claim that holds it.
#[tokio::test]
async fn each_write_that_sets_a_status_names_a_word_outside_the_set_and_only_that() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:atlas", "The Campaign").await;

    // ── add_entity with a status ────────────────────────────────────────────
    let made = s
        .call(
            "add_entity",
            json!({
                "kind": "work", "handle": "phi", "name": "Phi", "source": "user-named",
                "parent": "project:atlas", "sets": {"status": "considering"},
            }),
        )
        .await
        .json();
    assert!(
        made["status_word_note"]
            .as_str()
            .is_some_and(|note| note.contains("considering")),
        "creating with an outside word names it: {made}"
    );

    // ── update_entity with a status ─────────────────────────────────────────
    let edited = s
        .call(
            "update_entity",
            json!({"handle": "work:phi", "sets": {"status": "pondering"}}),
        )
        .await
        .json();
    assert!(
        edited["status_word_note"]
            .as_str()
            .is_some_and(|note| note.contains("pondering")),
        "editing the entity names it: {edited}"
    );
    let inside = s
        .call(
            "update_entity",
            json!({"handle": "work:phi", "sets": {"status": "now"}}),
        )
        .await
        .json();
    assert!(
        inside.get("status_word_note").is_none(),
        "a word inside the set is not named: {inside}"
    );

    // ── update_fact on the claim that holds it ──────────────────────────────
    let address = s
        .event_with(
            "work:phi",
            "where it stands",
            json!({"status": "next"}),
            &[],
        )
        .await;
    let claim = s
        .call(
            "update_fact",
            json!({"address": address, "fields": {"status": "musing"}}),
        )
        .await
        .json();
    assert!(
        claim["status_word_note"]
            .as_str()
            .is_some_and(|note| note.contains("musing")),
        "editing the claim names it: {claim}"
    );
    let back = s
        .call(
            "update_fact",
            json!({"address": address, "fields": {"status": "waiting"}}),
        )
        .await
        .json();
    assert!(
        back.get("status_word_note").is_none(),
        "a word inside the set is not named: {back}"
    );
    story.finish().await;
}
