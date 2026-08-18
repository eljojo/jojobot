//! **What a caller meets on a real rail when nothing seeded the process.**
//!
//! The set of kinds is written and loaded by the boot. A process that never
//! took that step cannot read a handle at all — and the refusal it gives has to
//! say so, rather than blaming the handle it was sent.
//!
//! This is the case that would have caught the defect it exists for. A mail
//! rail validates the owner's handle and never touches a memory store, so
//! nothing on that path loads the set: the suite passed only because some other
//! case running beside it had loaded one, and alone it refused `bot:gamma`
//! while listing `bot` among the kinds it accepted.
//!
//! **A test binary of its own, because the set is process-wide.** A case
//! running beside this one would load it, and then this one would be asserting
//! about a state it is not in.

use std::path::PathBuf;
use std::sync::Arc;

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::mailboxes::DoltMailboxes;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::free_port;
use jojobot_domain::mailbox::{MailboxError, MailboxName, Mailboxes, OwnerIndex, OwnerLookup};
use jojobot_domain::memory::{EntityId, EntityKind};

/// A directory of this run's own, removed when it is done.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An owner index that answers for anybody, so the refusal under test is the
/// kind set's and not this.
struct AnyOwner;

#[async_trait::async_trait]
impl OwnerIndex for AnyOwner {
    async fn look_up(&self, _: &EntityId) -> Result<OwnerLookup, MailboxError> {
        Ok(OwnerLookup::Known)
    }
}

#[tokio::test]
async fn an_unseeded_process_says_so_rather_than_blaming_the_handle() {
    let path = std::env::temp_dir().join(format!("jojobot-unseeded-{}", std::process::id()));
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let mut store = Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    migrate::run(store.pool()).await.expect("the schema");
    // **And no seed.** This is the state a process is in before a boot writes
    // the kinds, which is the state the mail rail was silently relying on
    // somebody else to leave.

    let mail = DoltMailboxes::open(store.pool().clone(), Arc::new(AnyOwner));
    let refused = mail
        .create_mailbox(
            &MailboxName("inbox".into()),
            &EntityId::new(EntityKind::BOT, "gamma"),
            None,
        )
        .await
        .expect_err("a process that cannot read a handle cannot open a box for one");

    let said = refused.to_string();
    assert!(
        said.contains("never loaded"),
        "the refusal names the failure a caller can act on — nothing seeded this process: {said}",
    );
    // **And it does not recite the kinds**, which is the sentence that made the
    // old refusal unreadable: it listed `bot` among the kinds it accepted while
    // refusing a handle whose kind was `bot`.
    assert!(
        !said.contains("no kind is named"),
        "an unseeded process does not report this as an unknown kind: {said}",
    );

    store.stop().await;
    drop(scratch);
}
