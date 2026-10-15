//! "Can a helper give itself a right by writing the record that grants it?"
//!
//! **A voucher is issued by the engine and is never written by a caller.** A
//! well-shaped record that a caller captures is not a grant, for the reason a
//! caller cannot write a role's holder: a hand-written grant is a right taken
//! without being given. So every route a caller has to a voucher is turned
//! back, and the refusal says vouchers are issued, not written.
//!
//! **Every refusal sits beside the positive it depends on**: the same keys on a
//! thing that is not a voucher land, so a build that refused every write
//! carrying them would not pass. Nothing is gated and nothing a bot does
//! changes: this slice ships the kind and the guard only.

use jojobot_domain::memory::{EntityId, Guarded, NewEntity, NewFact, guard};
use serde_json::json;

use super::dsl::Story;

/// **A voucher issued the way the engine issues one**: through the store, not
/// through a verb. Returns the address of its claim.
async fn issue_through_the_store(story: &Story, voucher: &EntityId) -> String {
    let today: jiff::civil::Date = "2026-10-09".parse().expect("a day");
    let new = NewEntity::new(
        voucher.clone(),
        format!("Grant {}", voucher.slug()),
        "jojobot",
    );
    let first = jojobot_domain::memory::testing::contract::state(NewFact {
        fields: [
            ("right", "boot-as"),
            ("target", "bot:patty"),
            ("holder", "abcd"),
            ("issuer", "wxyz"),
            ("state", "issued"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect(),
        ..NewFact::about(voucher.clone(), "a voucher for boot-as on patty", today)
    });
    let memory = story.memory_store();
    let mut issued = memory
        .add_entity_with_first_claim(new.clone(), first.clone())
        .await
        .expect("the engine path writes it");
    // **A near miss on the name alone is lifted as the engine lifts it**
    // (session/voucher_write.rs): with the guard's own token and a reason. Two
    // vouchers of one right share the name's words, so the second one meets it.
    if let Guarded::Blocked {
        attempted,
        candidates,
    } = &issued
    {
        let lifted = NewEntity {
            override_token: Some(guard::override_token(attempted, candidates)),
            override_reason: Some("an engine-written record is named for its holder".into()),
            ..new
        };
        issued = memory
            .add_entity_with_first_claim(lifted, first)
            .await
            .expect("the engine path writes it");
    }
    let Guarded::Written((_, claim)) = issued else {
        panic!("the engine path was blocked");
    };
    story.the_store_changed_underneath().await;
    claim.address().to_string()
}

#[tokio::test]
async fn a_bot_that_writes_a_voucher_is_refused_and_nothing_is_stored() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:patty", "Patty").await;
    s.add("topic:widgets", "Widgets").await;
    let marge = story.as_bot("bot:marge").await;

    // ── creating a voucher by hand is refused ───────────────────────────────
    marge
        .refused(
            "add_entity",
            json!({
                "kind": "voucher", "handle": "voucher:boot-as-patty",
                "name": "Boot as Patty", "source": "user-named",
            }),
        )
        .await
        .says("\"wrote\":false")
        .says("issued");

    // ── and so is creating one that carries its keys ────────────────────────
    marge
        .refused(
            "add_entity",
            json!({
                "kind": "voucher", "handle": "voucher:boot-as-patty",
                "name": "Boot as Patty", "source": "user-named",
                "sets": {"right": "boot-as", "target": "bot:patty", "state": "held"},
            }),
        )
        .await
        .says("issued");

    // ── a capture about a voucher is refused ────────────────────────────────
    marge
        .refused(
            "capture",
            json!({
                "subject": "voucher:boot-as-patty", "content": "a grant to boot as patty",
                "provenance": "inference",
                "fields": {"right": "boot-as", "target": "bot:patty", "state": "held"},
            }),
        )
        .await
        .says("\"wrote\":false")
        .says("issued");

    // ── the same keys on a thing that is not a voucher are ordinary ─────────
    marge
        .call(
            "capture",
            json!({
                "subject": "topic:widgets", "content": "widgets are issued by the factory",
                "provenance": "inference",
                "sets": {"right": "boot-as", "target": "bot:patty", "state": "held"},
            }),
        )
        .await
        .says("topic:widgets");
    s.call("recall", json!({"subject": "topic:widgets"}))
        .await
        .says("\"state\":\"held\"");

    // ── nothing was stored under the kind ───────────────────────────────────
    s.list("voucher").await.number("/count", 0);

    story.finish().await;
}

/// "A voucher is refused. How does a session get one?"
///
/// **The refusal names the door that issues one, and the door answers.** A bot
/// that writes a voucher, by any verb, is told that `start_here` with
/// `ask_for_code` is the way, and that call, made, answers with a reference to a
/// challenge and never the code.
#[tokio::test]
async fn a_refused_voucher_write_names_the_door_that_issues_one() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    an_instance_with_an_operator(&s).await;
    let marge = story.as_bot("bot:marge").await;

    for refused in [
        marge
            .refused(
                "add_entity",
                json!({
                    "kind": "voucher", "handle": "voucher:boot-as-patty",
                    "name": "Boot as Patty", "source": "user-named",
                }),
            )
            .await,
        marge
            .refused(
                "capture",
                json!({
                    "subject": "voucher:boot-as-patty", "content": "a grant to boot as patty",
                    "provenance": "inference",
                }),
            )
            .await,
    ] {
        refused.says("start_here").says("ask_for_code");
    }

    // The door the refusal names, called: a reference to a challenge comes back.
    let (asked, none) = story
        .call("start_here", json!({"bot": "patty", "ask_for_code": true}))
        .await;
    assert!(none.is_none(), "asking for a code starts no session");
    asked.says("\"challenge\"");
}

/// "A voucher exists. Can a bot edit it, rename it or take it back?"
///
/// **Once the engine has issued a voucher, no caller changes it by any verb.**
/// The voucher is staged through the store, the way the engine writes one; the
/// bot then tries each verb that edits a record, and the voucher reads back as
/// it was issued.
#[tokio::test]
async fn a_voucher_the_engine_issued_is_not_edited_renamed_or_taken_back_by_a_bot() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:patty", "Patty").await;
    let marge = story.as_bot("bot:marge").await;

    // ── the engine issues one, through the store ────────────────────────────
    let voucher = EntityId("voucher:boot-as-patty".into());
    let address = issue_through_the_store(&story, &voucher).await;
    s.call("recall", json!({"subject": "voucher:boot-as-patty"}))
        .await
        .says("\"state\":\"issued\"")
        .says("\"right\":\"boot-as\"");

    // ── editing the claim's keys is refused ─────────────────────────────────
    marge
        .refused(
            "update_fact",
            json!({"address": address, "fields": {"state": "held"}}),
        )
        .await
        .says("\"wrote\":false")
        .says("issued");

    // ── editing the thing is refused ────────────────────────────────────────
    marge
        .refused(
            "update_entity",
            json!({"handle": "voucher:boot-as-patty", "name": "Boot as anybody"}),
        )
        .await
        .says("issued");

    // ── taking the claim back is refused ────────────────────────────────────
    marge
        .refused(
            "retract",
            json!({"address": address, "reason": "taking it back"}),
        )
        .await
        .says("issued");

    // ── and it reads back exactly as it was issued ──────────────────────────
    s.call(
        "recall",
        json!({"subject": "voucher:boot-as-patty", "facts": true}),
    )
    .await
    .says("\"state\":\"issued\"")
    .says("a voucher for boot-as on patty")
    .never_says("\"state\":\"held\"")
    .never_says("Boot as anybody");

    // ── the same verbs still work on a thing that is not a voucher ──────────
    marge
        .call(
            "update_entity",
            json!({"handle": "bot:patty", "name": "Patty the second"}),
        )
        .await;

    story.finish().await;
}

/// "A voucher exists. Can a bot rename it, fold it into something, archive it,
/// or make one by renaming?"
///
/// **Rename, merge and archive are turned back like every other write to a
/// voucher**, on either side of a rename or a merge: a voucher moved onto
/// another handle, or another thing renamed onto a voucher's kind, is a grant
/// taken without being given. **Each refusal sits beside the positive it
/// depends on**: the same verbs land on a bot.
#[tokio::test]
async fn a_bot_cannot_rename_merge_or_archive_a_voucher_nor_rename_into_one() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:patty", "Patty").await;
    s.add("bot:nelson", "Nelson").await;
    let marge = story.as_bot("bot:marge").await;
    let first = EntityId("voucher:boot-as-patty".into());
    let second = EntityId("voucher:boot-as-nelson".into());
    issue_through_the_store(&story, &first).await;
    issue_through_the_store(&story, &second).await;

    // ── renaming a voucher is refused ───────────────────────────────────────
    marge
        .refused(
            "rename_entity",
            json!({"handle": "voucher:boot-as-patty", "to": "voucher:boot-as-anybody"}),
        )
        .await
        .says("\"wrote\":false")
        .says("issued");
    marge
        .refused(
            "rename_entity",
            json!({"handle": "voucher:boot-as-patty", "to": "topic:widgets"}),
        )
        .await
        .says("issued");

    // ── and so is renaming anything onto a voucher's kind ───────────────────
    marge
        .refused(
            "rename_entity",
            json!({"handle": "bot:nelson", "to": "voucher:made-by-hand"}),
        )
        .await
        .says("issued");

    // ── a merge is refused with the voucher on either side ──────────────────
    marge
        .refused(
            "merge_entities",
            json!({"duplicate": "voucher:boot-as-patty", "survivor": "bot:patty"}),
        )
        .await
        .says("issued");
    marge
        .refused(
            "merge_entities",
            json!({"duplicate": "bot:nelson", "survivor": "voucher:boot-as-nelson"}),
        )
        .await
        .says("issued");

    // ── archiving and restoring are refused ─────────────────────────────────
    marge
        .refused(
            "archive_entity",
            json!({"handle": "voucher:boot-as-patty", "reason": "tidying"}),
        )
        .await
        .says("issued");
    marge
        .refused(
            "archive_entity",
            json!({"handle": "voucher:boot-as-patty", "reason": "back", "restore": true}),
        )
        .await
        .says("issued");

    // ── both vouchers are exactly where they were, live, and nothing new ────
    let listed = s.list("voucher").await;
    listed
        .number("/count", 2)
        .says("voucher:boot-as-patty")
        .says("voucher:boot-as-nelson")
        .never_says("anybody")
        .never_says("made-by-hand");

    // ── the same verbs still land on a bot ──────────────────────────────────
    marge
        .call(
            "rename_entity",
            json!({"handle": "bot:nelson", "to": "bot:moe"}),
        )
        .await;
    marge
        .call(
            "archive_entity",
            json!({"handle": "bot:patty", "reason": "tidying"}),
        )
        .await;

    story.finish().await;
}

// ── the operator's code ──────────────────────────────────────────────────────

/// An instance whose operator is named, with the colleagues a story needs. The
/// bot that sets it up is `otto`, who is never a party to a code.
async fn an_instance_with_an_operator(s: &super::dsl::Session) {
    s.add("person:lisa", "Lisa").await;
    s.event_with(
        "topic:instance",
        "who the operator is",
        json!({"operator": "person:lisa"}),
        &[],
    )
    .await;
    for bot in ["marge", "patty", "nelson"] {
        s.add(&format!("bot:{bot}"), bot).await;
    }
}

/// **Ask for a code as a hand-started session does**, and read the code the way
/// the operator does, from the operator's own box. Returns the challenge's
/// handle and the code. The code is in the box and nowhere in the answer.
async fn ask_and_read(story: &Story, bot: &str) -> (String, String) {
    let before = story.operators_mail("person:lisa").await.len();
    let (asked, session) = story
        .call("start_here", json!({"bot": bot, "ask_for_code": true}))
        .await;
    assert!(session.is_none(), "asking for a code starts no session");
    let handle = asked.json()["challenge"]["handle"]
        .as_str()
        .expect("the answer names the challenge")
        .to_string();
    let mail = story.operators_mail("person:lisa").await;
    assert_eq!(mail.len(), before + 1, "one message reached the operator");
    let (_, body) = mail.last().expect("the operator's box holds it").clone();
    let code = body
        .split("Code: ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_else(|| panic!("the mail carries a code: {body}"))
        .trim_end_matches('.')
        .to_string();
    asked.never_says(&code);
    (handle, code)
}

/// "I started a helper by hand. How does it get the right to be that bot?"
///
/// **The session asks for a code for the bot it names; the code goes to the
/// operator's own box and nowhere else.** The session is handed a reference to
/// the challenge, never the code. The operator types the code into the session,
/// and the session presents it on boot. A boot as any bot but the assistant is
/// given that bot's ordinary voucher, held by the new session and issued by the
/// operator. The code is spent on first use.
#[tokio::test]
async fn a_code_goes_to_the_operator_alone_and_a_boot_that_presents_it_is_given_a_voucher() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    an_instance_with_an_operator(&s).await;

    let (challenge, code) = ask_and_read(&story, "marge").await;

    // ── the code is in the operator's box and in no bot's reach ─────────────
    // **The positive the count of zero depends on**: the same search, run with
    // mail on, finds a word that sits in a bot's box. Without it, zero would
    // read the same on a search that reached no mail at all.
    s.call(
        "post_message",
        json!({"to": "marge", "subject": "a note for marge",
               "body": "the ladder is behind the kestrelnine shed"}),
    )
    .await;
    s.find_including_mail("kestrelnine")
        .await
        .number("/count", 1)
        .says("kestrelnine");
    s.find_including_mail(&code).await.number("/count", 0);
    s.call("recall", json!({"subject": challenge, "facts": true}))
        .await
        .says("\"state\":\"issued\"")
        .says("\"bot\":\"bot:marge\"")
        .says("code_hash")
        .never_says(&code);

    // ── before any root, the gate is open and the boot says so ──────────────
    let (booted, session) = story
        .call(
            "start_here",
            json!({"bot": "marge", "challenge": challenge, "code": code}),
        )
        .await;
    let session = session.expect("a presented code boots a session");
    booted
        .never_says(&code)
        .says("\"gate\":\"ungated\"")
        .says("\"right\":\"boot-as\"")
        .says("\"target\":\"bot:marge\"");

    // ── the code travelled in the operator's box and in no bot's outbox ─────
    session
        .call("list_sent", json!({}))
        .await
        .says("A code to start a session as bot:marge")
        .never_says(&code);

    // ── the voucher is the new session's, issued by the operator ────────────
    let vouchers = s.list("voucher").await;
    vouchers.number("/count", 1);
    let voucher = booted.json()["session"]["trust"]["redeemed"]["voucher"]
        .as_str()
        .expect("the boot names the voucher")
        .to_string();
    s.call("recall", json!({"subject": voucher}))
        .await
        .says("\"state\":\"held\"")
        .says("\"right\":\"boot-as\"")
        .says(&format!("\"holder\":\"{}\"", session.run()))
        .never_says(session.sid())
        .says("\"issuer\":\"person:lisa\"")
        .never_says("\"parent\"");
    s.call("recall", json!({"subject": challenge}))
        .await
        .says("\"state\":\"redeemed\"");

    // ── the code is spent: the same code is refused, and nothing more is made ─
    story
        .refused_at_the_door(
            "start_here",
            json!({"bot": "patty", "challenge": challenge, "code": code}),
        )
        .await
        .says("\"wrote\":false")
        .never_says(&code);
    story
        .refused_at_the_door(
            "start_here",
            json!({"bot": "marge", "challenge": challenge, "code": code, "resume": "new"}),
        )
        .await
        .says("already");
    s.list("voucher").await.number("/count", 1);

    story.finish().await;
}

/// "I started the assistant by hand. Does the same code do for it?"
///
/// **A boot as the assistant is given the root**, which names the assistant, is
/// issued by the operator and has no parent. The first root turns the gate on,
/// and a code for any other bot never did. A root lasts one session: wrapping
/// voids it, and reopening the wrapped run with its wrap code restores none.
#[tokio::test]
async fn the_assistants_code_grants_the_root_which_turns_the_gate_on_and_dies_with_its_session() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    an_instance_with_an_operator(&s).await;

    // ── a coded boot of another bot leaves the gate open ────────────────────
    let (challenge, code) = ask_and_read(&story, "marge").await;
    story
        .call(
            "start_here",
            json!({"bot": "marge", "challenge": challenge, "code": code}),
        )
        .await
        .0
        .says("\"gate\":\"ungated\"");
    let (probe, _) = story.call("start_here", json!({"bot": "nelson"})).await;
    probe.says("\"gate\":\"ungated\"");

    // ── the assistant's code: the shipped identity already has a run in flight,
    //    so the boot is handed a choice, and the code is not spent by it ──────
    let (challenge, code) = ask_and_read(&story, "assistant").await;
    let (offered, none) = story
        .call(
            "start_here",
            json!({"bot": "assistant", "challenge": challenge, "code": code}),
        )
        .await;
    assert!(
        none.is_none(),
        "a choice carries no handle: {}",
        offered.raw()
    );
    s.call("recall", json!({"subject": challenge}))
        .await
        .says("\"state\":\"issued\"");
    s.list("voucher").await.number("/count", 1);

    // ── answering the choice spends the code and grants the root ────────────
    let (booted, assistant) = story
        .call(
            "start_here",
            json!({
                "bot": "assistant", "challenge": challenge, "code": code, "resume": "new",
            }),
        )
        .await;
    let assistant = assistant.expect("the answered choice boots a session");
    booted
        .never_says(&code)
        .says("\"right\":\"root\"")
        .says("\"target\":\"bot:assistant\"");
    let root = booted.json()["session"]["trust"]["redeemed"]["voucher"]
        .as_str()
        .expect("the boot names the root")
        .to_string();
    s.call("recall", json!({"subject": root}))
        .await
        .says("\"state\":\"held\"")
        .says("\"right\":\"root\"")
        .says("\"issuer\":\"person:lisa\"")
        .says(&format!("\"holder\":\"{}\"", assistant.run()))
        .never_says(assistant.sid())
        .never_says("\"parent\"");

    // ── the first root turned the gate on, for every named boot after it ────
    let (gated, _) = story.call("start_here", json!({"bot": "patty"})).await;
    gated.says("\"gate\":\"gated\"");

    // ── wrapping voids the root, and the wrap code restores nothing ─────────
    let wrapped = assistant
        .call("wrap_session", json!({"story": "the assistant's sitting"}))
        .await;
    let wrap_code = wrapped.field("wrap_code");
    wrapped.says("\"vouchers_voided\":1");
    s.call("recall", json!({"subject": root}))
        .await
        .says("\"state\":\"void\"");
    let (reopened, again) = story
        .call(
            "start_here",
            json!({"bot": "assistant", "resume": wrap_code}),
        )
        .await;
    assert!(again.is_some(), "the wrap code reopens the run");
    reopened.never_says("\"redeemed\"");
    s.call("recall", json!({"subject": root}))
        .await
        .says("\"state\":\"void\"")
        .never_says("\"state\":\"held\"");
    s.call(
        "recall",
        json!({"kind": "voucher", "fields": [{"key": "state", "value": "held"}]}),
    )
    .await
    .number("/count", 1);
    let (still, _) = story.call("start_here", json!({"bot": "patty"})).await;
    still.says("\"gate\":\"gated\"");

    story.finish().await;
}

/// "I asked twice, and I typed it wrong."
///
/// **A pending code is not replaced**: a second request for the same bot while
/// the first is good is refused, and the refusal names when the first expires,
/// so a misfiring session cannot burn the operator's codes. The first code still
/// works. **A wrong code counts, and the third voids the challenge**, after
/// which even the right code is refused and the refusal says to ask for a new
/// one, and a new request then stands. A wrong code names no part of the right
/// one.
#[tokio::test]
async fn a_pending_code_is_not_replaced_and_three_wrong_codes_void_a_challenge() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    an_instance_with_an_operator(&s).await;

    // ── a second request is refused while the first is good ─────────────────
    let (second, second_code) = ask_and_read(&story, "marge").await;
    let mails = story.operators_mail("person:lisa").await.len();
    story
        .refused_at_the_door("start_here", json!({"bot": "marge", "ask_for_code": true}))
        .await
        .says("\"wrote\":false")
        .says("expires_at")
        .says(&second)
        .never_says(&second_code);
    assert_eq!(
        story.operators_mail("person:lisa").await.len(),
        mails,
        "a refused request sent the operator nothing"
    );
    s.list("challenge").await.number("/count", 1);
    s.call("recall", json!({"subject": second}))
        .await
        .says("\"state\":\"issued\"");
    s.list("voucher").await.number("/count", 0);

    // ── a request for another bot is never in the way ───────────────────────
    let (other, _) = ask_and_read(&story, "patty").await;
    s.call("recall", json!({"subject": second}))
        .await
        .says("\"state\":\"issued\"");
    s.call("recall", json!({"subject": other}))
        .await
        .says("\"state\":\"issued\"");

    // ── a code for one bot does not boot another ────────────────────────────
    story
        .refused_at_the_door(
            "start_here",
            json!({"bot": "patty", "challenge": second, "code": second_code}),
        )
        .await
        .says("\"wrote\":false");

    // ── two wrong codes count and leave the challenge standing ──────────────
    for _ in 0..2 {
        story
            .refused_at_the_door(
                "start_here",
                json!({"bot": "marge", "challenge": second, "code": "00000000"}),
            )
            .await
            .says("\"wrote\":true")
            .says("\"recorded\"")
            .never_says(&second_code);
    }
    s.call("recall", json!({"subject": second}))
        .await
        .says("\"state\":\"issued\"")
        .says("\"wrong_tries\":\"2\"");

    // ── the third wrong code voids it, and then the right code is refused ───
    story
        .refused_at_the_door(
            "start_here",
            json!({"bot": "marge", "challenge": second, "code": "00000000"}),
        )
        .await
        .says("ask for a new code")
        .says("\"challenge_state\":\"void\"")
        .says("\"wrong_tries\":3");
    s.call("recall", json!({"subject": second}))
        .await
        .says("\"state\":\"void\"");
    story
        .refused_at_the_door(
            "start_here",
            json!({"bot": "marge", "challenge": second, "code": second_code}),
        )
        .await
        .says("ask for a new code");
    s.list("voucher").await.number("/count", 0);

    // ── a fresh request starts clean, and the right code boots ──────────────
    let (third, third_code) = ask_and_read(&story, "marge").await;
    let (_, booted) = story
        .call(
            "start_here",
            json!({"bot": "marge", "challenge": third, "code": third_code}),
        )
        .await;
    assert!(booted.is_some(), "the fresh code boots");
    s.list("voucher").await.number("/count", 1);

    story.finish().await;
}

/// "I waited too long, and the server restarted."
///
/// **A code is good for ten minutes**, read on the server's clock: a server
/// serving a later day refuses it as expired, and an expired challenge counts
/// no wrong try. **A restart voids every outstanding challenge**, because the
/// key that hashes a code is drawn at startup and never stored: the right code
/// no longer matches, and the answer says to ask again.
#[tokio::test]
async fn a_code_is_dead_after_its_window_and_after_a_restart() {
    let story = Story::begin("bot:otto").await;
    let setup = story.session().await;
    an_instance_with_an_operator(&setup).await;

    // ── a later day: the window is over, and no try is counted ──────────────
    let (challenge, code) = ask_and_read(&story, "marge").await;
    let later = story.restarted_pretending_it_is("2027-01-01").await;
    let tomorrow = later.session().await;
    later
        .refused_at_the_door(
            "start_here",
            json!({"bot": "marge", "challenge": challenge, "code": code}),
        )
        .await
        .says("expired")
        .says("ask for a new code");
    tomorrow
        .call("recall", json!({"subject": challenge}))
        .await
        .says("\"wrong_tries\":\"0\"");
    tomorrow.list("voucher").await.number("/count", 0);

    // ── an expired code is not in the way: a new request replaces it ────────
    let (renewed, _) = ask_and_read(&later, "marge").await;
    tomorrow
        .call("recall", json!({"subject": challenge}))
        .await
        .says("\"state\":\"replaced\"");
    tomorrow
        .call("recall", json!({"subject": renewed}))
        .await
        .says("\"state\":\"issued\"");

    // ── a restart inside the window: the key is new, the old hash is dead ───
    let (challenge, code) = ask_and_read(&story, "patty").await;
    let restarted = story.restarted().await;
    restarted
        .refused_at_the_door(
            "start_here",
            json!({"bot": "patty", "challenge": challenge, "code": code}),
        )
        .await
        .says("ask for a new code");

    story.finish().await;
}

/// "Can a code be asked for with nobody to send it to, or for nobody?"
///
/// **A code needs an operator to carry it and a bot to be for.** Without a named
/// operator nothing is made, and the refusal says how to name one. A bot that
/// does not exist gets no challenge. A bot cannot write a challenge itself,
/// whatever the verb.
#[tokio::test]
async fn no_code_is_made_without_an_operator_or_a_bot_and_no_bot_writes_a_challenge() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;

    // ── nobody is the operator yet: nothing is made ─────────────────────────
    story
        .refused_at_the_door("start_here", json!({"bot": "marge", "ask_for_code": true}))
        .await
        .says("topic:instance");
    s.list("challenge").await.number("/count", 0);

    // ── the operator is named; a bot that is nobody still gets no code ──────
    s.add("person:lisa", "Lisa").await;
    s.event_with(
        "topic:instance",
        "who the operator is",
        json!({"operator": "person:lisa"}),
        &[],
    )
    .await;
    story
        .refused_at_the_door("start_here", json!({"bot": "krusty", "ask_for_code": true}))
        .await
        .says("\"wrote\":false")
        .says("not a bot jojobot knows");
    s.list("challenge").await.number("/count", 0);
    assert!(story.operators_mail("person:lisa").await.is_empty());

    // ── a code is asked for a bot the caller names, and nothing else ────────
    story
        .refused_at_the_door("start_here", json!({"ask_for_code": true}))
        .await
        .says("bot");

    // ── and a bot cannot write a challenge by hand, by any verb ─────────────
    s.refused(
        "add_entity",
        json!({
            "kind": "challenge", "handle": "challenge:made-by-hand",
            "name": "A code", "source": "user-named",
        }),
    )
    .await
    .says("issued");
    s.refused(
        "capture",
        json!({
            "subject": "challenge:made-by-hand", "content": "a code", "provenance": "inference",
            "fields": {"bot": "bot:marge", "state": "issued"},
        }),
    )
    .await
    .says("issued");

    let (challenge, _) = ask_and_read(&story, "marge").await;
    let address = format!("{challenge}#f1");
    s.refused(
        "update_fact",
        json!({"address": address, "fields": {"state": "redeemed"}}),
    )
    .await
    .says("issued");
    s.refused(
        "rename_entity",
        json!({"handle": challenge, "to": "challenge:renamed"}),
    )
    .await
    .says("issued");
    s.call("recall", json!({"subject": challenge}))
        .await
        .says("\"state\":\"issued\"");

    story.finish().await;
}

/// "The assistant's run is wrapped. Can a code ride in on its wrap code?"
///
/// **A wrap code reopens a closed run for one last change, and takes no code.**
/// A code presented beside one is refused before anything is checked, so it is
/// not spent, and no root is given to a run that is already over.
#[tokio::test]
async fn a_wrap_code_reopening_takes_no_code_and_spends_none() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    an_instance_with_an_operator(&s).await;

    // ── marge boots and wraps; a wrap code comes back ───────────────────────
    let marge = story.as_bot("bot:marge").await;
    let wrap_code = marge
        .call("wrap_session", json!({"story": "a sitting"}))
        .await
        .field("wrap_code");

    // ── a code beside the wrap code is refused and nothing is spent ─────────
    let (challenge, code) = ask_and_read(&story, "marge").await;
    story
        .refused_at_the_door(
            "start_here",
            json!({
                "bot": "marge", "challenge": challenge, "code": code, "resume": wrap_code,
            }),
        )
        .await
        .says("wrap code")
        .says("\"wrote\":false")
        .never_says(&code);
    s.call("recall", json!({"subject": challenge}))
        .await
        .says("\"state\":\"issued\"")
        .says("\"wrong_tries\":\"0\"");
    s.list("voucher").await.number("/count", 0);

    // ── the wrap code alone still reopens the run, and the code still works ──
    let (reopened, again) = story
        .call("start_here", json!({"bot": "marge", "resume": wrap_code}))
        .await;
    assert!(
        again.is_some(),
        "the wrap code reopens the run: {}",
        reopened.raw()
    );
    let (_, booted) = story
        .call(
            "start_here",
            json!({"bot": "marge", "challenge": challenge, "code": code, "resume": "new"}),
        )
        .await;
    assert!(booted.is_some(), "the unspent code boots a new run");
    s.list("voucher").await.number("/count", 1);

    story.finish().await;
}
