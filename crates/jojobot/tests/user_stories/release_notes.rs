//! "I was away for a deploy. Did anyone tell me what changed?"
//!
//! **jojobot tells each bot itself.** A change bots can use ships with a note, and
//! when the server starts it leaves ONE message in each bot's box listing the
//! notes that box has not been sent, from the system sender and not from a bot. A
//! restart sends nothing twice; an archived bot is told nothing; a bot with a
//! manager is told who to ask, and a bot without one is not told of one.

use serde_json::{Value, json};

use super::dsl::Story;

/// **The notes this build ships**, by id. A note added to `release-notes/` is
/// added here, and a story that finds one missing or sent twice fails.
const SHIPPED_NOTES: &[&str] = &[
    "amend-journal-keeps-jojobot-beats",
    "decisions-lead-a-container-read",
    "defrag-ships-a-charter",
    "empty-list-entry-refused",
    "held-key-fold-refused",
    "leftovers-come-as-counts",
    "list-read-pages-and-counts-ticks",
    "list-runs-counts-calls",
    "many-entries-or-ticks-in-one-call",
    "mark-processed-own-box",
    "misspelt-parent-refused",
    "numbered-siblings-pass-screen",
    "over-capacity-room-says-archive",
    "recall-says-prose-left-out",
    "receipt-needs-settled",
    "release-notes",
    "rhythm-on-the-calendar",
    "seats-dropped-is-a-list",
    "ticks-judged-in-order",
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

#[tokio::test]
async fn a_bot_holds_one_message_from_the_system_listing_what_shipped_and_who_to_ask() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:moe", "Moe").await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:bart", "Bart").await;
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
