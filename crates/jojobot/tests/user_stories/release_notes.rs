//! "I was away for a deploy. Did anyone tell me what changed?"
//!
//! **jojobot tells each bot itself.** A change bots can use ships with a note, and
//! when the server starts it leaves ONE message in each bot's box listing the
//! notes that box has not been sent, from the system sender and not from a bot. A
//! restart sends nothing twice; an archived bot is told nothing; a bot with a
//! manager is told who to ask, and a bot without one is not told of one.
//!
//! **What a bot was told is kept on the bot**, so a note the bot quarantines is
//! still told and never sent again, and a bot made now is told every note this
//! build ships and sent none: a note is about a change it did not live through.
//! The bots that lived through one are written into the store under the server,
//! as a store an older build filled holds them.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

/// **The notes this build ships**, by id. A note added to a `release-notes/deploy-N/` folder is
/// added here, and a story that finds one missing or sent twice fails.
const SHIPPED_NOTES: &[&str] = &[
    "defrag-ships-a-charter",
    "list-read-pages-and-counts-ticks",
    "many-entries-or-ticks-in-one-call",
    "release-notes",
    "rhythm-on-the-calendar",
    "trip-is-a-kind",
];

/// The ids the last line of a message names, in the order it names them.
fn ids_named(body: &str) -> Vec<String> {
    body.trim_end()
        .lines()
        .last()
        .and_then(|line| line.strip_prefix("Notes: "))
        .map(|ids| ids.split(", ").map(String::from).collect())
        .unwrap_or_default()
}

/// **A message holds every shipped note once, in the shape a note has.** One
/// title line, one `For:` line and the three sections for each note, and the
/// last line names each of them once. The words of a note are not pinned: a note
/// may be reworded, and the shape and the ids may not move.
fn holds_every_shipped_note_once(who: &str, body: &str) {
    let named = ids_named(body);
    let mut sorted = named.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        named.len(),
        "a note is named twice in {who}'s message: {body}"
    );
    let mut shipped: Vec<String> = SHIPPED_NOTES.iter().map(|id| id.to_string()).collect();
    shipped.sort();
    assert_eq!(
        sorted, shipped,
        "{who}'s message names the notes this build ships: {body}"
    );
    let lines = |prefix: &str| body.lines().filter(|line| line.starts_with(prefix)).count();
    assert_eq!(
        lines("# "),
        named.len(),
        "a title for each note in {who}'s message"
    );
    assert_eq!(
        lines("For: "),
        named.len(),
        "a For line for each note in {who}'s message"
    );
    assert_eq!(
        lines("## "),
        3 * named.len(),
        "three sections for each note in {who}'s message"
    );
}

/// The messages in a delivery, and the delivery's own answer for a failure to quote.
fn delivered(answer: &Value) -> &Vec<Value> {
    answer["messages"]
        .as_array()
        .unwrap_or_else(|| panic!("a delivery holds messages: {answer}"))
}

/// **A bot an older build made**, with its box, written into the stores under the
/// server: it holds no record of what it was told.
async fn an_old_bot(story: &Story, slug: &str, name: &str) {
    use jojobot_domain::mailbox::{MailboxName, Mailboxes};
    use jojobot_domain::memory::{EntityId, NewEntity};
    let id = EntityId(format!("bot:{slug}"));
    story
        .memory_store()
        .add_entity(NewEntity::new(id.clone(), name, "user-named"))
        .await
        .expect("the bot is written")
        .written()
        .expect("nothing blocks a roster bot");
    story
        .mail_store()
        .create_mailbox(&MailboxName(slug.to_string()), &id, None)
        .await
        .expect("its box opens");
    story.the_store_changed_underneath().await;
}

#[tokio::test]
async fn a_bot_holds_one_message_from_the_system_listing_what_shipped_and_who_to_ask() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    an_old_bot(&story, "moe", "Moe").await;
    an_old_bot(&story, "marge", "Marge").await;
    an_old_bot(&story, "bart", "Bart").await;
    let marge_first = story.as_bot("bot:marge").await;
    marge_first
        .call(
            "capture",
            json!({
                "subject": "bot:moe", "content": "bot:moe reports to bot:marge",
                "provenance": "testimony", "sets": {"reports_to": "bot:marge"},
            }),
        )
        .await;
    s.call(
        "archive_entity",
        json!({"handle": "bot:bart", "reason": "retired"}),
    )
    .await;

    // ── the server starts: the passes run over the board ────────────────────
    let later = story.restarted().await;
    let moe = later.as_bot("bot:moe").await;
    let first = moe.call("read_mailbox", json!({})).await.json();
    let messages = delivered(&first);
    assert_eq!(messages.len(), 1, "exactly one message: {first}");
    let note = &messages[0];
    assert_eq!(
        note["sender"], "jojobot",
        "from the system, not a bot: {note}"
    );
    let moes = note["body"].as_str().expect("a body");
    holds_every_shipped_note_once("Moe", moes);
    assert!(
        moes.contains("@bot:marge"),
        "Moe is told the name of Moe's manager, as a mention: {moes}"
    );
    moe.call(
        "mark_processed",
        json!({"message_id": note["id"], "notes": "read"}),
    )
    .await;

    // ── Marge has no manager and so no line naming one; she is sent her own ──
    let marge = later.as_bot("bot:marge").await;
    let marges_box = marge.call("read_mailbox", json!({})).await.json();
    let messages = delivered(&marges_box);
    assert_eq!(messages.len(), 1, "Marge's own: {marges_box}");
    assert_eq!(messages[0]["sender"], "jojobot");
    let marges = messages[0]["body"]
        .as_str()
        .unwrap_or_else(|| panic!("Marge's message has a body: {}", messages[0]));
    holds_every_shipped_note_once("Marge", marges);
    assert!(
        !marges.contains("@bot:"),
        "Marge has no manager, so no mention of one: {marges}"
    );

    // ── an archived bot is told nothing ─────────────────────────────────────
    assert_eq!(later.mail_held_by("bot:bart").await, (0, 0, 0));

    // ── the message is an ordinary one to every reader ──────────────────────
    let found = moe
        .call("search", json!({"query": "release", "include_mail": true}))
        .await
        .json();
    assert!(
        found
            .to_string()
            .contains(note["id"].as_str().unwrap_or_default()),
        "search finds the note by its id: {found}"
    );

    // ── a reply has nowhere to go: jojobot has no box ───────────────────────
    moe.refused(
        "post_message",
        json!({"to": "jojobot", "body": "thanks", "in_reply_to": note["id"]}),
    )
    .await;
    let replied = moe
        .call(
            "post_message",
            json!({"to": "marge", "body": "did you read it?", "in_reply_to": note["id"]}),
        )
        .await
        .json();
    assert_eq!(replied["state"], "new", "a reply breaks nothing: {replied}");

    // ── list_sent is Moe's own mail: the reply, and not the note Moe was sent ──
    let sent = moe.call("list_sent", json!({})).await.json();
    let sent = sent["messages"]
        .as_array()
        .unwrap_or_else(|| panic!("list_sent holds messages: {sent}"));
    assert_eq!(sent.len(), 1, "only the reply Moe sent: {sent:?}");
    assert_eq!(sent[0]["id"], replied["id"], "{sent:?}");

    // ── another start sends nothing more ────────────────────────────────────
    let again = later.restarted().await;
    assert_eq!(
        again.mail_held_by("bot:moe").await,
        (0, 0, 1),
        "Moe's note is processed and nothing new arrived"
    );
    let after_two_starts = again.restarted().await;
    assert_eq!(
        after_two_starts.mail_held_by("bot:marge").await,
        (1, 1, 0),
        "Marge holds the note Marge read and Moe's reply, and nothing arrived at either start"
    );
    after_two_starts.finish().await;
}

/// **A note the bot quarantines is still told, and the next start sends it no
/// copy.** The paired positive is a box that never had the note, which a start
/// still sends one to.
#[tokio::test]
async fn a_quarantined_note_is_told_and_never_sent_again() {
    let story = Story::begin("bot:otto").await;
    an_old_bot(&story, "gamma", "Gamma").await;
    let later = story.restarted().await;
    let gamma = later.as_bot("bot:gamma").await;
    let delivered = gamma.call("read_mailbox", json!({})).await.json();
    let messages = delivered["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 1, "gamma is sent the note: {delivered}");
    gamma
        .call(
            "mark_processed",
            json!({"message_id": messages[0]["id"], "quarantine": "not for me"}),
        )
        .await;
    an_old_bot(&later, "delta", "Delta").await;

    let again = later.restarted().await;
    assert_eq!(
        again.mail_held_by("bot:gamma").await,
        (0, 0, 0),
        "no new copy for the bot that quarantined its note"
    );
    assert_eq!(
        again.mail_held_by("bot:delta").await,
        (1, 0, 0),
        "a box that never had the note is sent one"
    );
    again.finish().await;
}

/// **The ids of every note this build ships**, from the list this file keeps: the
/// told set is compared to it and not to a constant the code under test reads.
fn shipped_note_ids() -> Vec<String> {
    SHIPPED_NOTES.iter().map(|id| id.to_string()).collect()
}

/// The ids a bot's ledger holds, read the way a caller reads them.
async fn told_ids(session: &Session, handle: &str) -> Vec<String> {
    let held = session
        .call(
            "recall",
            json!({"subject": handle, "keys": ["release_notes_told"]}),
        )
        .await
        .json();
    held["objects"][0]["fields"]["release_notes_told"]
        .as_str()
        .map(|ids| ids.split(", ").map(str::to_string).collect())
        .unwrap_or_default()
}

/// **A bot made now is told every note this build ships and sent none.** What it
/// was told is every note on disk, and its box holds nothing from the system. The
/// paired positive is the old bot beside it, which is sent the note.
#[tokio::test]
async fn a_bot_made_now_is_told_every_shipped_note_and_sent_none() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:delta", "Delta").await;
    an_old_bot(&story, "gamma", "Gamma").await;

    let later = story.restarted().await;
    assert_eq!(later.mail_held_by("bot:delta").await, (0, 0, 0));
    assert_eq!(later.mail_held_by("bot:gamma").await, (1, 0, 0));
    let delta = later.as_bot("bot:delta").await;
    assert_eq!(
        told_ids(&delta, "bot:delta").await,
        shipped_note_ids(),
        "the bot is told every note this build ships"
    );
    later.finish().await;
}

/// **A note shipped after a bot was made reaches it at the next start, and it is
/// sent that note alone.** The bot is made now, so it is told every note; the
/// ledger is then taken back one note underneath, as the store of a build that
/// shipped one fewer holds it. A start sends the bot exactly that note, and the
/// bot is told them all again. The paired positive is a bot told every note,
/// which a start sends nothing.
#[tokio::test]
async fn a_note_shipped_after_a_bot_was_made_reaches_it_at_the_next_start_alone() {
    use jojobot_domain::memory::review::is_release_notes_record;
    use jojobot_domain::memory::{EntityId, FactPatch};
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:delta", "Delta").await;
    s.add("bot:gamma", "Gamma").await;

    let mut ids = shipped_note_ids();
    let newest = ids.pop().expect("the build ships notes");
    let delta = EntityId("bot:delta".into());
    let memory = story.memory_store();
    let ledger = memory
        .recall(&delta)
        .await
        .expect("delta reads")
        .into_iter()
        .find(|fact| is_release_notes_record(&fact.fields))
        .expect("a bot made now holds the record of what it was told");
    memory
        .update_fact(
            &ledger.address(),
            FactPatch {
                fields: [("release_notes_told".to_string(), ids.join(", "))].into(),
                sets: ["release_notes_told".to_string()].into(),
                ..Default::default()
            },
            &delta,
        )
        .await
        .expect("the ledger is taken back one note");
    story.the_store_changed_underneath().await;

    let later = story.restarted().await;
    assert_eq!(
        later.mail_held_by("bot:delta").await,
        (1, 0, 0),
        "delta is sent the one note shipped after it was told"
    );
    assert_eq!(
        later.mail_held_by("bot:gamma").await,
        (0, 0, 0),
        "gamma, told every note, is sent nothing"
    );
    let delivered = later
        .as_bot("bot:delta")
        .await
        .call("read_mailbox", json!({}))
        .await
        .json();
    let body = delivered["messages"][0]["body"].as_str().expect("a body");
    assert_eq!(
        body.lines().last().map(str::trim),
        Some(format!("Notes: {newest}").as_str()),
        "the message carries that note alone: {delivered}"
    );
    assert_eq!(
        told_ids(&later.as_bot("bot:delta").await, "bot:delta").await,
        shipped_note_ids(),
        "and the bot is told every note again"
    );
    later.finish().await;
}

/// **The record of what a bot was told is out of an ordinary read of the bot.**
/// `recall` leaves the key and the record out and counts the record, and brings
/// the record back when asked; the bot's boot carries no rule for it. The paired
/// positive is the bot's own ordinary record, which reads.
#[tokio::test]
async fn the_record_of_what_a_bot_was_told_is_out_of_an_ordinary_recall() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:delta", "Delta").await;
    let later = story.restarted().await;
    let delta = later.as_bot("bot:delta").await;
    delta
        .call(
            "capture",
            json!({"subject": "bot:delta", "content": "keeps the kiln lit",
                   "provenance": "testimony"}),
        )
        .await;
    let is_ledger = |fact: &serde_json::Value| fact["fields"]["engine_record"] == "release_notes";

    let plain = delta
        .call("recall", json!({"subject": "bot:delta", "facts": true}))
        .await
        .json();
    let object = &plain["objects"][0];
    assert!(
        object["fields"].get("release_notes_told").is_none(),
        "an ordinary read leaves the key out: {plain}"
    );
    let facts = object["facts"].as_array().expect("facts");
    assert!(!facts.iter().any(is_ledger), "and the record: {plain}");
    assert!(
        facts
            .iter()
            .any(|fact| fact["content"].as_str().is_some_and(|c| c.contains("kiln"))),
        "the bot's own record reads: {plain}"
    );
    assert_eq!(
        object["release_notes_record_left_out"]["count"], 1,
        "and the answer counts what it left out: {plain}"
    );

    let asked = delta
        .call(
            "recall",
            json!({"subject": "bot:delta", "facts": true, "review_marks": true}),
        )
        .await
        .json();
    assert!(
        asked["objects"][0]["facts"]
            .as_array()
            .is_some_and(|facts| facts.iter().any(is_ledger)),
        "asked for, the record comes back: {asked}"
    );

    later.finish().await;
}

/// **The record of what a bot was told is no rule of the bot's.** The bot's boot
/// carries no rule and lists none for it. The paired positive is the bot's own
/// record, which is its one rule.
#[tokio::test]
async fn the_record_of_what_a_bot_was_told_is_no_rule_of_the_bot() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:delta", "Delta").await;
    let later = story.restarted().await;
    later
        .as_bot("bot:delta")
        .await
        .call(
            "capture",
            json!({"subject": "bot:delta", "content": "keeps the kiln lit",
                   "provenance": "testimony"}),
        )
        .await;
    let booted = later
        .call(
            "start_here",
            json!({"bot": "delta", "brief": true, "resume": "new"}),
        )
        .await
        .0
        .json();
    let unseated = booted["identity"]["unseated_rules"]["listed"]
        .as_array()
        .map_or(0, Vec::len);
    let carried = booted["identity"]["rules"].as_array().map_or(0, Vec::len);
    assert_eq!(
        (carried, unseated),
        (0, 1),
        "the kiln record is the bot's one rule, unstarred, and the ledger is none: {booted}"
    );
    assert!(
        booted["identity"]["unseated_rules"]["listed"][0]["line"]
            .as_str()
            .is_some_and(|line| line.contains("kiln")),
        "and the one that is listed is the kiln record: {booted}"
    );
    later.finish().await;
}

/// **A search leaves the record of what a bot was told out, names it in
/// `left_out`, and returns it when `review_marks` asks.** The paired positive is
/// the same query with the argument.
#[tokio::test]
async fn a_search_leaves_the_record_of_what_a_bot_was_told_out_until_asked() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:delta", "Delta").await;
    let later = story.restarted().await;
    let delta = later.as_bot("bot:delta").await;
    let found = |answer: &serde_json::Value| -> bool {
        answer["results"]
            .as_array()
            .expect("results")
            .iter()
            .any(|hit| {
                hit["content"]
                    .as_str()
                    .is_some_and(|c| c.contains("keeps here"))
            })
    };

    let plain = delta
        .call("search", json!({"query": "keeps", "kind": "bot"}))
        .await
        .json();
    assert!(!found(&plain), "no record among the hits: {plain}");
    assert!(
        plain["left_out"]
            .as_array()
            .expect("left_out")
            .iter()
            .any(|line| line.as_str().is_some_and(|l| l.contains("release notes"))),
        "left_out names the record: {plain}"
    );

    let asked = delta
        .call(
            "search",
            json!({"query": "keeps", "kind": "bot", "review_marks": true}),
        )
        .await
        .json();
    assert!(found(&asked), "asked for, the record is a hit: {asked}");
    later.finish().await;
}

/// **No caller writes the record of what a bot was told, by any of the four write
/// verbs.** Each refusal names the key and writes nothing. The paired positive is
/// an ordinary key on the same bot, which each verb takes.
#[tokio::test]
async fn no_caller_writes_the_record_of_what_a_bot_was_told() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:delta", "Delta").await;
    let later = story.restarted().await;
    let delta = later.as_bot("bot:delta").await;
    let note = json!({"release_notes_told": "release-notes"});

    delta
        .refused(
            "capture",
            json!({"subject": "bot:delta", "content": "told nothing", "provenance": "testimony",
                   "sets": note}),
        )
        .await
        .says("release_notes_told")
        .says("\"wrote\":false");
    delta
        .refused(
            "add_entity",
            json!({"kind": "bot", "handle": "sigma", "name": "Sigma", "source": "user-named",
                   "sets": note}),
        )
        .await
        .says("release_notes_told");
    delta
        .refused(
            "update_entity",
            json!({"handle": "bot:delta", "sets": note}),
        )
        .await
        .says("release_notes_told");
    let kept = delta
        .call(
            "capture",
            json!({"subject": "bot:delta", "content": "an ordinary key", "provenance": "testimony",
                   "sets": {"favourite_colour": "green"}}),
        )
        .await
        .json();
    delta
        .refused(
            "update_fact",
            json!({"address": kept["address"], "fields": note}),
        )
        .await
        .says("release_notes_told");
    later.finish().await;
}

/// **A start that sends a bot a note leaves a bot defrag reviewed reviewed.** The
/// ledger is jojobot's own bookkeeping, so writing it is no change of the bot:
/// defrag marks delta reviewed, a start sends delta a note it was not sent and
/// rewrites its ledger, and delta is not in defrag's queue. The pair: a write of
/// another bot's on delta afterwards puts it back.
#[tokio::test]
async fn a_start_that_sends_a_note_leaves_a_reviewed_bot_reviewed() {
    use jojobot_domain::memory::review::is_release_notes_record;
    use jojobot_domain::memory::{EntityId, FactPatch};
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:delta", "Delta").await;
    let defrag = story.as_bot("bot:defrag").await;
    defrag
        .call(
            "update_entity",
            json!({"handle": "bot:delta", "reviewed": true}),
        )
        .await;
    let waiting = |answer: Value| -> Vec<String> {
        answer["objects"]
            .as_array()
            .map(|objects| {
                objects
                    .iter()
                    .filter_map(|o| o["id"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let queue = waiting(
        defrag
            .call("recall", json!({"unreviewed": true}))
            .await
            .json(),
    );
    assert!(
        !queue.contains(&"bot:delta".to_string()),
        "setup: {queue:?}"
    );

    // The ledger is taken back one note underneath, as the store of a build that
    // shipped one fewer holds it, so the start has a note to send and a ledger to
    // rewrite.
    let mut ids = shipped_note_ids();
    ids.pop().expect("the build ships notes");
    let delta = EntityId("bot:delta".into());
    let memory = story.memory_store();
    let ledger = memory
        .recall(&delta)
        .await
        .expect("delta reads")
        .into_iter()
        .find(|fact| is_release_notes_record(&fact.fields))
        .expect("a bot made now holds the record of what it was told");
    memory
        .update_fact(
            &ledger.address(),
            FactPatch {
                fields: [("release_notes_told".to_string(), ids.join(", "))].into(),
                sets: ["release_notes_told".to_string()].into(),
                ..Default::default()
            },
            &delta,
        )
        .await
        .expect("the ledger is taken back one note");
    story.the_store_changed_underneath().await;

    let later = story.restarted().await;
    assert_eq!(
        later.mail_held_by("bot:delta").await,
        (1, 0, 0),
        "the note was sent"
    );
    let defrag = later.as_bot("bot:defrag").await;
    let queue = waiting(
        defrag
            .call("recall", json!({"unreviewed": true}))
            .await
            .json(),
    );
    assert!(
        !queue.contains(&"bot:delta".to_string()),
        "the ledger write put delta back in the queue: {queue:?}"
    );

    // The pair: a real write on delta does.
    let delta_session = later.as_bot("bot:delta").await;
    delta_session
        .call(
            "capture",
            json!({"subject": "bot:delta", "content": "keeps the shelves",
                   "provenance": "testimony"}),
        )
        .await;
    let queue = waiting(
        defrag
            .call("recall", json!({"unreviewed": true}))
            .await
            .json(),
    );
    assert!(queue.contains(&"bot:delta".to_string()), "{queue:?}");
    later.finish().await;
}

/// **A bot and the record of what it was told land together.** A commit that
/// fails while the bot is made leaves either no bot or a bot that holds its
/// ledger, never a bot that the next start sends every note. The same call sent
/// again finishes the creation, and the bot is then sent nothing. The paired
/// positive is a healthy creation beside it, told every note at once.
#[tokio::test]
async fn a_bot_is_never_left_without_the_record_of_what_it_was_told() {
    for failing in ["entity", "box"] {
        let story = Story::begin("bot:otto").await;
        let s = story.session().await;
        let call = json!({"kind": "bot", "handle": "gamma", "name": "Gamma",
                          "source": "user-named", "request_id": "make-gamma"});
        story.fails_the_next_commit_of(failing);
        let first = s.refused("add_entity", call.clone()).await.json();
        assert_eq!(first["status"], "blocked", "{first}");

        let held = s
            .attempt(
                "recall",
                json!({"subject": "bot:gamma", "keys": ["release_notes_told"]}),
            )
            .await;
        let made = held["objects"][0]["id"] == "bot:gamma";
        if made {
            assert_eq!(
                told_ids(&s, "bot:gamma").await,
                shipped_note_ids(),
                "after a failed {failing} commit gamma exists and holds no ledger: {held}"
            );
        }

        let second = s.call("add_entity", call).await.json();
        assert_eq!(second["mailbox"], "gamma", "{second}");
        s.add("bot:delta", "Delta").await;
        // A bot made with settings carries its ledger in the same first claim.
        let omega = s
            .call(
                "add_entity",
                json!({"kind": "bot", "handle": "omega", "name": "Omega",
                       "source": "user-named", "sets": {"one_liner": "keeps the shelves"}}),
            )
            .await
            .json();
        // The receipt reports the caller's write, not jojobot's own ledger.
        assert!(
            !omega["changed"].to_string().contains("release_notes_told"),
            "{omega}"
        );
        let later = story.restarted().await;
        assert_eq!(later.mail_held_by("bot:gamma").await, (0, 0, 0));
        assert_eq!(later.mail_held_by("bot:delta").await, (0, 0, 0));
        assert_eq!(later.mail_held_by("bot:omega").await, (0, 0, 0));
        assert_eq!(
            told_ids(&later.as_bot("bot:omega").await, "bot:omega").await,
            shipped_note_ids()
        );
        assert_eq!(
            told_ids(&later.as_bot("bot:gamma").await, "bot:gamma").await,
            shipped_note_ids()
        );
        assert_eq!(
            told_ids(&later.as_bot("bot:delta").await, "bot:delta").await,
            shipped_note_ids()
        );
        later.finish().await;
    }
}
