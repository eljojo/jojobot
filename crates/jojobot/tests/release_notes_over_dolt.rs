//! **The release notes are sent once over the real mailbox store.** What a bot
//! was told is kept on the bot, in a record of its own that holds the notes'
//! ids; a box that holds a system message listing them is the older ledger, and
//! still counts for a bot that has no record. A fake that stores a body verbatim
//! cannot show what a real store hands back, so the cases run over a Dolt store:
//! a first start sends and writes the record, a second start sends nothing, a
//! stored message whose body ends with a newline still counts, and a bot whose
//! box holds no message but whose record lists the notes is sent nothing.

use std::sync::Arc;

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::mailboxes::DoltMailboxes;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::owners::MemoryOwners;
use jojobot_adapters::testing::{free_port, migrated_database, start_unhurried};
use jojobot_domain::clock::Clock;
use jojobot_domain::mailbox::{MailboxName, Mailboxes, Message, NewMessage};
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};
use jojobot_mcp::release_notes::{SYSTEM_SENDER, send_unsent_notes};

/// A real store of this case's own, migrated and seeded with the kinds.
async fn a_real_store() -> (Dolt, sqlx::MySqlPool, std::path::PathBuf) {
    let dir = jojobot_adapters::testing::scratch_path("jojobot-release-notes");
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let store = start_unhurried(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "notes")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    (store, pool, dir)
}

/// What a start wires over the store: the memory and the mailboxes, opened anew.
fn a_start(pool: &sqlx::MySqlPool) -> (Arc<dyn Memory>, Arc<dyn Mailboxes>) {
    let memory: Arc<dyn Memory> = Arc::new(DoltMemory::open(pool.clone()));
    let owners = Arc::new(MemoryOwners::new(memory.clone()));
    let mailboxes: Arc<dyn Mailboxes> = Arc::new(DoltMailboxes::open(pool.clone(), owners));
    (memory, mailboxes)
}

/// The messages the system sender left in one box, oldest first.
async fn from_the_system(mailboxes: &Arc<dyn Mailboxes>, box_name: &str) -> Vec<Message> {
    mailboxes
        .sent_by(&[SYSTEM_SENDER])
        .await
        .expect("the sent mail reads")
        .into_iter()
        .filter(|message| message.mailbox.as_str() == box_name)
        .collect()
}

/// The last line of a message, which names the notes it carries.
/// **The ids of the notes on disk**, from the folders the build ships them from.
fn shipped_note_ids_on_disk() -> Vec<String> {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../jojobot-mcp/release-notes");
    let mut ids = Vec::new();
    for folder in std::fs::read_dir(&root).expect("the release-notes folder reads") {
        let folder = folder.expect("a folder entry").path();
        for file in std::fs::read_dir(&folder).expect("a deploy folder reads") {
            let file = file.expect("a file entry").path();
            ids.push(
                file.file_stem()
                    .expect("a file name")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    ids.sort();
    ids
}

fn ledger_line(body: &str) -> &str {
    body.trim_end()
        .lines()
        .last()
        .expect("a body has a last line")
}

#[tokio::test]
async fn over_the_real_store_a_start_sends_the_notes_and_the_next_start_sends_nothing() {
    let (mut store, pool, dir) = a_real_store().await;
    let (memory, mailboxes) = a_start(&pool);
    for slug in ["homer", "marge", "lisa"] {
        let id = EntityId(format!("bot:{slug}"));
        memory
            .add_entity(NewEntity::new(id.clone(), slug, "user-named"))
            .await
            .expect("add ok")
            .written()
            .expect("an empty board blocks nothing");
        mailboxes
            .create_mailbox(&MailboxName(slug.to_string()), &id, None)
            .await
            .expect("the box opens");
    }

    // ── the first start sends every note to each box ────────────────────────
    // The notes together are more than one message holds, so a start sends as
    // many messages as it takes; each names its notes on its last line and the
    // ledger is every one of those lines together.
    send_unsent_notes(&memory, &mailboxes, &Clock::default()).await;
    let first = from_the_system(&mailboxes, "homer").await;
    assert!(
        first.len() > 1,
        "the shipped notes are more than one message holds, so a start sends several: {first:?}"
    );
    // **No note is cut between two messages**: in each, the notes the last line
    // names are the notes standing in it, and together the messages name every
    // note on disk once.
    let mut named: Vec<String> = Vec::new();
    for message in &first {
        let ids: Vec<&str> = ledger_line(&message.body)
            .strip_prefix("Notes: ")
            .expect("a message names its notes")
            .split(", ")
            .collect();
        let lines = |prefix: &str| {
            message
                .body
                .lines()
                .filter(|line| line.starts_with(prefix))
                .count()
        };
        assert_eq!(lines("# "), ids.len(), "a title for each named note");
        assert_eq!(lines("For: "), ids.len(), "a For line for each named note");
        assert_eq!(lines("## "), 3 * ids.len(), "three sections for each note");
        named.extend(ids.iter().map(|id| id.to_string()));
    }
    named.sort();
    assert_eq!(named, shipped_note_ids_on_disk(), "every note on disk once");
    for message in &first {
        let line = ledger_line(&message.body);
        assert!(
            line.starts_with("Notes: ") && line.len() > "Notes: ".len(),
            "the message names its notes on its last line: {}",
            message.body
        );
    }
    let ledger = format!(
        "Notes: {}",
        first
            .iter()
            .filter_map(|message| ledger_line(&message.body).strip_prefix("Notes: "))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let messages_each = first.len();
    for slug in ["marge", "lisa"] {
        assert_eq!(
            from_the_system(&mailboxes, slug).await.len(),
            messages_each,
            "{slug}"
        );
    }
    // **The record on the bot, read back from the real store**: the key a start
    // wrote with `sets` is held by the thing, and lists the notes the message
    // lists. A store that lost the key would send every note again at the next
    // start, over a box that no longer held the message.
    let told_on_the_bot = memory
        .fields(&EntityId("bot:marge".into()))
        .await
        .expect("the bot's fields read");
    let ids = |list: &str| {
        let mut ids: Vec<String> = list.split(", ").map(str::to_string).collect();
        ids.sort();
        ids
    };
    assert_eq!(
        ids(told_on_the_bot
            .get("release_notes_told")
            .map(String::as_str)
            .unwrap_or_default()),
        ids(ledger.strip_prefix("Notes: ").unwrap_or_default()),
        "the bot holds the notes it was told: {told_on_the_bot:?}"
    );

    // ── a second start, over stores opened anew, sends nothing ──────────────
    // A restart ends the old writer before opening its successor.
    drop((memory, mailboxes));
    let (memory, mailboxes) = a_start(&pool);
    send_unsent_notes(&memory, &mailboxes, &Clock::default()).await;
    for slug in ["homer", "marge", "lisa"] {
        assert_eq!(
            from_the_system(&mailboxes, slug).await.len(),
            messages_each,
            "{slug} was sent nothing again"
        );
    }

    // ── a stored ledger posted with a newline at its end still counts ───────
    let bart = EntityId("bot:bart".into());
    memory
        .add_entity(NewEntity::new(bart.clone(), "bart", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("an empty board blocks nothing");
    mailboxes
        .create_mailbox(&MailboxName("bart".into()), &bart, None)
        .await
        .expect("the box opens");
    mailboxes
        .post_message(NewMessage {
            mailbox: MailboxName("bart".into()),
            body: format!("Notes sent before.\n\n{ledger}\n\n"),
            subject: None,
            sender: SYSTEM_SENDER.to_string(),
            sent_at: Clock::default().now(),
            in_reply_to: None,
            supersedes: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
        })
        .await
        .expect("the post lands");
    let stored = from_the_system(&mailboxes, "bart").await;
    assert_eq!(
        stored[0].body.trim_end(),
        stored[0].body,
        "the real store hands a body back without the whitespace at its end"
    );
    send_unsent_notes(&memory, &mailboxes, &Clock::default()).await;
    assert_eq!(
        from_the_system(&mailboxes, "bart").await.len(),
        1,
        "the notes the stored message lists are not sent again"
    );

    // ── a bot with the record and no message is sent nothing ────────────────
    // The record alone is the ledger: the box holds no system message, so the
    // older ledger says nothing, and only the record read back from the real
    // store keeps the notes from going out again.
    let milhouse = EntityId("bot:milhouse".into());
    memory
        .add_entity(NewEntity::new(milhouse.clone(), "milhouse", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("an empty board blocks nothing");
    mailboxes
        .create_mailbox(&MailboxName("milhouse".into()), &milhouse, None)
        .await
        .expect("the box opens");
    // A record an older build wrote, spelled out rather than read from the
    // constants the code reads it with.
    memory
        .capture(NewFact {
            fields: [
                ("engine_record".to_string(), "release_notes".to_string()),
                (
                    "release_notes_told".to_string(),
                    ledger
                        .strip_prefix("Notes: ")
                        .unwrap_or_default()
                        .to_string(),
                ),
            ]
            .into(),
            sets: ["release_notes_told".to_string()].into(),
            ..NewFact::about(
                milhouse,
                "kept for this case",
                "2026-10-01".parse().expect("a day"),
            )
        })
        .await
        .expect("the record is written");
    drop((memory, mailboxes));
    let (memory, mailboxes) = a_start(&pool);
    send_unsent_notes(&memory, &mailboxes, &Clock::default()).await;
    assert_eq!(
        from_the_system(&mailboxes, "milhouse").await.len(),
        0,
        "the record keeps the notes from being sent again, whatever the box holds"
    );

    store.stop().await;
    let _ = std::fs::remove_dir_all(dir);
}
