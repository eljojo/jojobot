//! **What a jojobot arrives with** — the identity, and the types the software
//! ships.
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
//! The types come with it for the same reason the method does (rules 82 and
//! 94): a vocabulary a caller has to declare before it works is one every
//! instance re-invents differently, and a type ships complete with its fields.
//!
//! # What it deliberately does NOT do
//!
//! It does not write a charter. What this identity is TOLD to be is a separate
//! question with the operator's name on it, and a shipped charter would put a
//! voice they have not approved into every future instance. The bot exists, it
//! can be booted as, and it can write. That is all.
//!
//! It writes no record of either type, and nothing here computes anything from
//! one. A vocabulary is what a writer fills in; what to do about a rhythm that
//! has gone quiet is a separate capability.

use std::sync::Arc;

use jojobot_domain::mailbox::{MailboxName, Mailboxes};
use jojobot_domain::memory::kinds;
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use jojobot_domain::memory::{EntityId, EntityKind, Memory, MemoryError, NewEntity};

/// The identity every instance has.
pub const DEFAULT_BOT: &str = "assistant";

/// **The types this build ships**, complete with their keys.
///
/// Two things make the list safe to write on every boot. A caller cannot
/// declare over a shipped name, so nothing under one of these names is ever a
/// caller's work; and a declaration is replaced whole, so what a later build
/// adds to one of these types reaches an existing instance without anybody
/// running anything.
///
/// The keys are the vocabulary and their spelling is the schema: renaming one
/// is a new type that reaches no record already written under the old spelling.
pub fn shipped_types() -> Vec<DeclaredType> {
    // **Every key of these two is REQUIRED, which is what they already meant.**
    // Fitting was once every key a declaration named, so a shipped key was one
    // nobody could leave out; saying so keeps these types answering exactly the
    // things they answered before optional keys existed. The required set of
    // each is the re-spec's to shrink, and shrinking it is a change to what the
    // type IS rather than a change to the model.
    vec![
        // **A cyclical thing, and the question it answers is what has gone
        // quiet.** A cadence is always TIME: the time prompts the check, and
        // what the check measures is a field on the check-in rather than a
        // unit of the schedule. So there is no distance and no unit key.
        // **When am I next away, and when was I last there.** The two places
        // are references rather than text, which is what makes a trip walkable
        // from either end.
        DeclaredType::shipped(
            "trip",
            vec![
                Field::required("departs_from", ValueType::Reference),
                Field::required("arrives_at", ValueType::Reference),
                Field::required("leaves_on", ValueType::Date),
                Field::required("returns_on", ValueType::Date),
            ],
        ),
    ]
}

/// Declare the types this build ships. **Every boot, unconditionally.**
///
/// There is no version stamp and no seed-once flag, and each of those would
/// buy the opposite bug: a build that adds a key to a shipped type has to
/// reach the instances already running, and an instance that skipped the write
/// because it had been seeded once would serve a vocabulary the software no
/// longer has. Overwriting is safe because a shipped name is closed to
/// callers, so the only declaration this can replace is a previous build's.
///
/// Returns how many types are now declared, or why the store could not take
/// them. A store that cannot be reached at startup is reported, never fatal:
/// the rest of the boot says the same thing about the same store.
/// **Write the shipped kinds, then load what the store holds.** Every boot,
/// unconditionally, exactly as the shipped types are written.
///
/// **The order is the point, and it is write-then-read rather than read.** A
/// seed that read first could not tell an instance whose kinds are missing
/// from one that has them, and an instance that never had them would serve a
/// set the software says exists and the store does not hold. Writing first
/// makes the two the same instance.
///
/// **What is loaded is the store's answer, not the list above it.** So a kind
/// a caller declared is parsed by this process, and a shipped kind the store
/// somehow lost stops being parsed rather than being kept alive by the code
/// that wrote it. The two refusals in [`kinds`] then mean what they say: an
/// empty set is a store that answered with nothing, and an unknown token is a
/// kind nobody declared.
pub async fn ensure_kinds(memory: &Arc<dyn Memory>) -> Result<usize, MemoryError> {
    kinds::seed(memory.as_ref()).await
}

pub async fn ensure_shipped_types(memory: &Arc<dyn Memory>) -> Result<usize, MemoryError> {
    let types = shipped_types();
    for declared in &types {
        memory.declare_type(declared.clone()).await?;
    }
    Ok(types.len())
}

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
    let id = EntityId::new(EntityKind::BOT, DEFAULT_BOT);

    match memory.list_entities(Some(EntityKind::BOT)).await {
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
    use jojobot_domain::memory::types::Origin;

    fn ports() -> (Arc<dyn Memory>, Arc<dyn Mailboxes>) {
        (
            Arc::new(InMemoryMemory::booted()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
        )
    }

    /// One type out of the store, by name.
    async fn stored(memory: &Arc<dyn Memory>, name: &str) -> DeclaredType {
        memory
            .declared_types()
            .await
            .expect("the roster reads")
            .into_iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("the store must hold '{name}'"))
    }

    /// The keys of a type, in the order it declares them, each with what it
    /// holds — which is the whole of what a type is.
    fn keys(declared: &DeclaredType) -> Vec<(&str, &str)> {
        declared
            .fields
            .iter()
            .map(|f| (f.key.as_str(), f.holds.as_token()))
            .collect()
    }

    /// **A fresh instance arrives holding the types the software ships**, whole
    /// — every key, in order, holding what it was declared to hold.
    ///
    /// Before this, the constructor that mints a shipped type was reached from
    /// tests alone: the protection over shipped names guarded nothing, and no
    /// caller could see an origin that was not its own.
    #[tokio::test]
    async fn a_fresh_instance_arrives_with_the_types_the_software_ships() {
        let (memory, _) = ports();
        assert_eq!(
            ensure_shipped_types(&memory)
                .await
                .expect("the store takes"),
            1
        );

        // The two places are references, which is what makes a trip walkable
        // from either end rather than a pair of strings.
        let trip = stored(&memory, "trip").await;
        assert_eq!(
            keys(&trip),
            vec![
                ("departs_from", "reference"),
                ("arrives_at", "reference"),
                ("leaves_on", "date"),
                ("returns_on", "date"),
            ],
        );
        assert_eq!(trip.origin, Origin::Shipped);
    }

    /// **The seed is unconditional, and what that buys is the key a later build
    /// adds.**
    ///
    /// The second boot writes the same declaration again rather than finding
    /// one under the name and leaving it. This is the case a "seed once" flag
    /// would break: an instance that had booted an older build would keep that
    /// build's keys for ever, and the software would serve a vocabulary it no
    /// longer has.
    #[tokio::test]
    async fn a_later_build_moves_a_shipped_type_on_an_instance_already_running() {
        let (memory, _) = ports();
        // What an older build shipped: the same name, three keys short.
        memory
            .declare_type(DeclaredType::shipped(
                "trip",
                vec![Field::required("departs_from", ValueType::Reference)],
            ))
            .await
            .expect("the software declares its own types");

        ensure_shipped_types(&memory)
            .await
            .expect("the store takes");

        assert_eq!(
            keys(&stored(&memory, "trip").await),
            vec![
                ("departs_from", "reference"),
                ("arrives_at", "reference"),
                ("leaves_on", "date"),
                ("returns_on", "date"),
            ],
            "this build's keys reached an instance that was already running",
        );
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
            .list_entities(Some(EntityKind::BOT))
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
        let other = EntityId::new(EntityKind::BOT, "gamma");
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
        let id = EntityId::new(EntityKind::BOT, DEFAULT_BOT);
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
                .list_entities(Some(EntityKind::BOT))
                .await
                .expect("list ok")
                .len(),
            1,
            "and no second assistant was created beside it"
        );
    }
}
