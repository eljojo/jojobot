//! **The listener is open before the first release note is sent.** Mailing the
//! notes writes to every bot's box one at a time, and a start that waits for it
//! keeps the operator and every bot out until it ends. The order of two log
//! lines is only visible in the real binary, against a real store that holds a
//! bot with a box and no record of any note told to it.

use std::process::Stdio;
use std::sync::Arc;

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::mailboxes::DoltMailboxes;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::owners::MemoryOwners;
use jojobot_adapters::testing::free_port;
use jojobot_domain::mailbox::{MailboxName, Mailboxes};
use jojobot_domain::memory::{EntityId, Memory, NewEntity, kinds};

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The event the sender logs for a bot it has mailed.
const SENT: &str = "release notes sent";

#[tokio::test]
async fn the_real_server_binary_logs_serving_before_it_mails_the_first_release_note() {
    let state_dir = jojobot_adapters::testing::scratch_path("jojobot-serve-first");
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let scratch = Scratch(state_dir.clone());
    let db_dir = state_dir.join("db");
    let store_port = free_port();

    // **A bot with a box and nothing told to it**, written by an earlier process:
    // the state of every bot on the first start of a build that ships a note.
    let mut seeding_server = Dolt::start(&db_dir, store_port)
        .await
        .expect("the store comes up");
    migrate::run(seeding_server.pool())
        .await
        .expect("the schema applies");
    let memory: Arc<dyn Memory> = Arc::new(DoltMemory::open(seeding_server.pool().clone()));
    kinds::seed(&DoltMemory::open(seeding_server.pool().clone()))
        .await
        .expect("the kinds are declared");
    let owners = Arc::new(MemoryOwners::new(memory.clone()));
    let mailboxes = DoltMailboxes::open(seeding_server.pool().clone(), owners);
    let bot = EntityId("bot:milhouse".into());
    memory
        .add_entity(NewEntity::new(bot.clone(), "Milhouse", "test"))
        .await
        .expect("the bot is written")
        .written()
        .expect("an empty board blocks nothing");
    mailboxes
        .create_mailbox(&MailboxName("milhouse".into()), &bot, None)
        .await
        .expect("the box opens");
    seeding_server.stop().await;
    kinds::load_shipped();

    let binary = jojobot_exercise::room::server_binary().expect("a jojobot binary to run");
    let http_port = free_port();
    let mut child = tokio::process::Command::new(&binary)
        .env("STATE_DIRECTORY", &state_dir)
        .env("JOJOBOT_STORE_PORT", store_port.to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{http_port}"))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env_remove("JOJOBOT_ISSUER")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("the binary runs");

    let serving = jojobot_adapters::testing::wait_for_serving_line(
        &mut child,
        jojobot_exercise::room::READY_TIMEOUT,
        "serving http://",
    )
    .await
    .unwrap_or_else(|failure| panic!("the real server {failure}"));
    let before: Vec<&String> = serving
        .seen
        .iter()
        .filter(|line| line.contains(SENT))
        .collect();
    assert!(
        before.is_empty(),
        "a note was mailed before the serving line: {before:?}"
    );

    // **The pairing that makes the absence mean something**: the same start does
    // mail the note, after the listener, to the bot that was owed it.
    let mut lines = serving.stdout;
    let after = tokio::time::timeout(jojobot_exercise::room::READY_TIMEOUT, async {
        while let Ok(Some(line)) = lines.next_line().await {
            if line.contains(SENT) && line.contains("bot:milhouse") {
                return Some(line);
            }
        }
        None
    })
    .await
    .unwrap_or(None);
    let _ = child.start_kill();
    let _ = child.wait().await;
    assert!(
        after.is_some(),
        "the bot owed the note is sent it once the listener is open"
    );

    drop(scratch);
}
