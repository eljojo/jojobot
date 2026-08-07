//! **The identity a jojobot arrives with.**
//!
//! There is never a jojobot with no bot. `assistant` is the default identity
//! and it exists on a fresh instance — not assumed to exist, actually there.
//!
//! Every memory write requires an identity and an identity IS a bot, so a
//! server with zero bots is a closed loop: the first bot is a write, a write
//! needs a `sid`, a `sid` comes from booting a bot. Rather than carving an
//! exception into the write rule, the state that would need one does not
//! exist. This runs once as the software starts — no request produces it and
//! no verb triggers it.
//!
//! # What it deliberately does NOT do
//!
//! It does not write a charter. What this identity is TOLD to be is a separate
//! question with the operator's name on it, and a shipped charter would put a
//! voice they have not approved into every future instance. The bot exists, it
//! can be booted as, and it can write. That is all.

use std::sync::Arc;

use jojobot_domain::mailbox::{MailboxName, Mailboxes};
use jojobot_domain::memory::{EntityId, EntityKind, Memory, NewEntity};

/// The identity every instance has.
pub const DEFAULT_BOT: &str = "assistant";

/// What a seeding attempt did, for the caller to log. Nothing here is an error
/// a caller should act on: a store that cannot be reached at startup is a
/// condition the rest of the boot already reports.
#[derive(Debug, PartialEq, Eq)]
pub enum Seeded {
    /// It was not there, and now it is.
    Created,
    /// It was already there and was left exactly as it was.
    AlreadyThere,
    /// The store could not be reached. Nothing was written.
    Unreachable(String),
}

/// Make sure the default identity exists, with its mailbox.
///
/// **Idempotent, and it never touches an existing `assistant`.** A live
/// instance has one with facts on it — rules somebody wrote — so a seed that
/// overwrote would be a data-loss bug wearing a setup step's clothes. The check
/// is existence, and existence alone: if the bot is there, this returns and
/// writes nothing at all.
pub async fn ensure_default_identity(
    memory: &Arc<dyn Memory>,
    mailboxes: &Arc<dyn Mailboxes>,
) -> Seeded {
    let id = EntityId::new(EntityKind::Bot, DEFAULT_BOT);

    match memory.list_entities(Some(EntityKind::Bot)).await {
        Ok(bots) if bots.iter().any(|b| b.id == id) => return Seeded::AlreadyThere,
        Ok(_) => {}
        Err(e) => return Seeded::Unreachable(e.to_string()),
    }

    // The box opens with the bot, inside the same act — the one place a
    // mailbox comes into being, and the reason this cannot be two calls a
    // caller could interleave.
    if let Err(e) = memory
        .add_entity(NewEntity::new(id.clone(), "Assistant", "jojobot"))
        .await
    {
        return Seeded::Unreachable(e.to_string());
    }
    if let Err(e) = mailboxes
        .create_mailbox(&MailboxName(DEFAULT_BOT.to_string()), &id, None)
        .await
    {
        // The bot landed and its box did not. Not silently: this is the state
        // the boot's own repair exists for, and it heals the next time this
        // identity boots.
        return Seeded::Unreachable(e.to_string());
    }
    Seeded::Created
}

#[cfg(test)]
mod tests {
    use super::*;
    use jojobot_domain::mailbox::testing::InMemoryMailboxes;
    use jojobot_domain::memory::testing::InMemoryMemory;

    fn ports() -> (Arc<dyn Memory>, Arc<dyn Mailboxes>) {
        (
            Arc::new(InMemoryMemory::new()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
        )
    }

    /// **A fresh instance has an identity**, and it can be written to.
    #[tokio::test]
    async fn a_fresh_instance_arrives_with_the_default_identity() {
        let (memory, mailboxes) = ports();
        assert_eq!(
            ensure_default_identity(&memory, &mailboxes).await,
            Seeded::Created
        );

        let bots = memory
            .list_entities(Some(EntityKind::Bot))
            .await
            .expect("list ok");
        assert_eq!(
            bots.iter().map(|b| b.id.to_string()).collect::<Vec<_>>(),
            vec!["bot:assistant".to_string()],
            "exactly the one identity, and nothing else invented alongside it"
        );

        // An identity that cannot be written to is not one.
        let boxes = mailboxes.list_mailboxes().await.expect("list ok");
        assert!(
            boxes.iter().any(|b| b.name.0 == DEFAULT_BOT),
            "the default identity has its box: {boxes:?}"
        );
    }

    /// **A near miss on the board does not deny the default identity its box.**
    ///
    /// The seed sends no override token, and needs none: the box name IS the
    /// owner's handle, which the entity screen adjudicated in the same act.
    /// Re-running a similarity screen here would refuse `assistant` a box on any
    /// instance that happens to hold a box one letter away from that name — and
    /// an identity that cannot be written to is not one, so the loop this module
    /// exists to close would stay open on exactly the instances that already
    /// have mail.
    #[tokio::test]
    async fn a_near_miss_on_the_board_does_not_deny_the_default_identity_its_box() {
        let (memory, mailboxes) = ports();
        let other = EntityId::new(EntityKind::Bot, "gamma");
        // One letter off `assistant` — a near miss by the mailbox guard's own
        // budget.
        mailboxes
            .create_mailbox(&MailboxName("assistan".into()), &other, None)
            .await
            .expect("create ok")
            .written()
            .expect("an empty board blocks nothing");

        // The positive the verdict rests on: that board really is hostile to
        // this name. Without it, the assertion below passes on a build where
        // `assistan` was never a near miss and the screen never fired at all.
        assert!(
            matches!(
                mailboxes
                    .create_mailbox(&MailboxName(DEFAULT_BOT.into()), &other, None)
                    .await
                    .expect("a blocked create is a result, not a failure"),
                jojobot_domain::mailbox::Guarded::Blocked { .. }
            ),
            "that name is refused to any other owner, so the screen is live"
        );

        assert_eq!(
            ensure_default_identity(&memory, &mailboxes).await,
            Seeded::Created
        );
        let boxes = mailboxes.list_mailboxes().await.expect("list ok");
        assert!(
            boxes.iter().any(|b| b.name.0 == DEFAULT_BOT),
            "the default identity has a box of its own, not the near miss: {boxes:?}"
        );
    }

    /// **Running it twice writes nothing the second time**, and running it
    /// against an instance that already has an `assistant` leaves that one
    /// exactly as it was — facts and all. A seed that overwrote would be data
    /// loss, not setup.
    #[tokio::test]
    async fn seeding_never_touches_an_assistant_that_already_exists() {
        let (memory, mailboxes) = ports();
        ensure_default_identity(&memory, &mailboxes).await;

        // Somebody's real instance: the identity has been renamed and carries
        // a rule. Both must survive.
        let id = EntityId::new(EntityKind::Bot, DEFAULT_BOT);
        memory
            .capture(jojobot_domain::memory::NewFact::about(
                id.clone(),
                "answers in one line unless asked otherwise",
                jiff::civil::date(2026, 7, 1),
            ))
            .await
            .expect("capture ok");

        assert_eq!(
            ensure_default_identity(&memory, &mailboxes).await,
            Seeded::AlreadyThere,
            "a second seed recognises the identity rather than remaking it"
        );

        // Paired: the identity is still there AND its facts are untouched.
        let facts = memory.recall(&id).await.expect("recall ok");
        assert_eq!(
            facts.len(),
            1,
            "the rule somebody wrote on this identity survived the seed: {facts:?}"
        );
        assert!(facts[0].content.contains("answers in one line"));
        assert_eq!(
            memory
                .list_entities(Some(EntityKind::Bot))
                .await
                .expect("list ok")
                .len(),
            1,
            "and no second assistant was created beside it"
        );
    }
}
