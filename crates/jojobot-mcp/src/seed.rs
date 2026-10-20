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
//! It does not write a charter, and the identity still has one. **The text
//! ships in the binary as a provision** — see [`crate::orientation::charter`] —
//! and is resolved into a read underneath every verb, so what an instance
//! writes for itself is the only charter text in the store.
//!
//! Writing it here would freeze an instance on the build that seeded it: a
//! later version improves the text and no existing instance would ever see it.
//! The bot exists, it can be booted as, it can write, and it can say what it is
//! for.
//!
//! It writes no record of either type, and nothing here computes anything from
//! one. A vocabulary is what a writer fills in; what to do about a rhythm that
//! has gone quiet is a separate capability.

use std::sync::Arc;

use jojobot_domain::clock::Clock;
use jojobot_domain::mailbox::{MailboxName, Mailboxes};
use jojobot_domain::memory::kinds;
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use jojobot_domain::memory::{
    EntityId, EntityKind, Guarded, INSTANCE_RECORD, Memory, MemoryError, NewEntity, NewFact,
};

/// The identity every instance has, **named by its handle**.
///
/// **One literal with two readers**: the entity is addressed by this, and the
/// mailbox is named for the slug inside it, read back through [`EntityId`]
/// rather than by cutting the string here. Two constants would be two copies
/// of one name with nothing holding them in step.
///
/// **The handle is written whole because the bright-line gate reads
/// handle-shaped text.** A bare slug is a name no gate compares against the
/// fictional roster, so an address written as a kind and a slug apart reaches
/// the repository unchecked (rules 164, 45).
pub const DEFAULT_BOT: &str = jojobot_domain::memory::ASSISTANT;

/// **The organize stage**, shipped beside the default identity (card 2547).
pub const DEFRAG_BOT: &str = "bot:defrag";

/// **Every bot a fresh instance arrives holding**, with the name it is created
/// under. One list and one existence-only check cover every entry: a bot added
/// here is seeded the way the assistant is, with no second path (rule 498).
pub const SHIPPED_BOTS: [(&str, &str); 2] = [(DEFAULT_BOT, "Assistant"), (DEFRAG_BOT, "Defrag")];

/// The default identity's address.
pub(crate) fn default_bot() -> EntityId {
    EntityId(DEFAULT_BOT.to_string())
}

/// **The keys the build declares as describing their own record**, read from the
/// shipped `record-labels` type so a seventh is named wherever this is used with
/// no edit. These never fold onto the thing a claim is about.
pub fn describing_keys() -> Vec<String> {
    shipped_types()
        .into_iter()
        .find(|declared| declared.name == "record-labels")
        .map(|declared| declared.fields.into_iter().map(|field| field.key).collect())
        .unwrap_or_default()
}

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
        // **What somebody holds that gets them in.** It is a record about the
        // HOLDER, pointing at the thing it admits to: whether a thing offers a
        // tier at all is a claim about that thing and lives there, and holding
        // one is a claim about a person with its own provenance and its own
        // window. Collapsing the two gives a record that reads correct and
        // answers neither question.
        //
        // **`admits` names no kind on purpose, and it must stay that way.** A
        // declared reference may name the kind it points at; this one declines,
        // so what a pass gets you into can be an event today and something else
        // later without the declaration being rewritten. That openness is a
        // decision somebody took, not an omission to tighten.
        //
        // **Only `admits` is required, and that is the model rather than
        // leniency**: a record is an entitlement because it admits somebody to
        // something. The window is optional because a record with no dates is
        // live rather than expired — an entitlement nobody dated — and the tier
        // is a free label the values-in-use read answers for.
        DeclaredType::shipped(
            "entitlement",
            vec![
                Field::required("admits", ValueType::Reference),
                Field::new("valid_from", ValueType::Date),
                Field::new("valid_until", ValueType::Date),
                Field::new("tier", ValueType::Text),
            ],
        ),
        // **The same keys the `RunsOut` and `DecideBy` carriers already
        // match structurally** (see `jojobot_domain::attention`), named here
        // so `answers_type`/`fits_type` can ask for them the way any other
        // declared type is asked for. Declaring buys the name; the carrier
        // itself is what makes a thing carrying the key answer `overdue`.
        DeclaredType::shipped(
            "runs-out",
            vec![Field::new(
                jojobot_domain::attention::RUNS_OUT,
                ValueType::Date,
            )],
        ),
        DeclaredType::shipped(
            "decide-by",
            vec![Field::new(
                jojobot_domain::attention::DECIDE_BY,
                ValueType::Date,
            )],
        ),
        // **The three days jojobot works out for a loop and writes on it**: when
        // it next falls due, the day of its last check-in and the day its next
        // cycle counts from. They are dates the software computes, so the
        // software declares them, and a question that names no kind can order
        // them (`due_on <= today+7d`) instead of being refused for want of a
        // declaration. The fourth computed key, `outcome`, is a word out of
        // ran, skipped and snoozed rather than a day, so it is not here.
        //
        // **Every key is optional**: a thing carries whichever of them its
        // schedule has produced, and none is needed for the others to mean
        // anything. The kinds that declare their own copies of two of these
        // keep them; a type is the vocabulary for a question over all kinds.
        DeclaredType::shipped(
            "computed-dates",
            vec![
                Field::new(jojobot_domain::attention::DUE_ON, ValueType::Date),
                Field::new(jojobot_domain::attention::LAST_CHECK_IN, ValueType::Date),
                Field::new(jojobot_domain::attention::COUNTS_FROM, ValueType::Date),
            ],
        ),
        // **Six keys that describe the claim they ride on and never the thing
        // it is about** (rule 360: only the build's own vocabulary attaches
        // behaviour, and rule 106: declared on the key, not listed in the
        // fold). A bot's rule carries `starred`, `subject` and `purpose`; a
        // claim read out of a system of record carries `read_from` and
        // `read_ref`; `recorded_by` says who wrote it. Folded onto the bot or
        // the loop they read as the thing's own properties. The claim keeps
        // every one and a read of the claim still sees it.
        //
        // **None of them is a key the due moment is computed from**, so the
        // stored projection never read them and has nothing to stop reading.
        DeclaredType::shipped(
            "record-labels",
            jojobot_domain::memory::types::DESCRIBING_KEYS
                .into_iter()
                .map(Field::describing)
                .collect(),
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
        memory.declare_type(declared.clone(), None).await?;
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
    /// The bot was already there without its box, and now the box is there.
    BoxOpened,
    /// The store could not be reached. Nothing was written.
    Unreachable(String),
}

/// Make sure every shipped bot exists, with its mailbox. One answer per bot,
/// in the order of [`SHIPPED_BOTS`].
pub async fn ensure_shipped_identities(
    memory: &Arc<dyn Memory>,
    mailboxes: &Arc<dyn Mailboxes>,
    clock: &Clock,
) -> Vec<(&'static str, Seeded)> {
    let mut seeded = Vec::new();
    for (handle, name) in SHIPPED_BOTS {
        seeded.push((
            handle,
            ensure_identity(memory, mailboxes, clock, handle, name).await,
        ));
    }
    seeded
}

/// Make sure the default identity exists, with its mailbox.
pub async fn ensure_default_identity(
    memory: &Arc<dyn Memory>,
    mailboxes: &Arc<dyn Mailboxes>,
) -> Seeded {
    ensure_identity(
        memory,
        mailboxes,
        &Clock::default(),
        DEFAULT_BOT,
        "Assistant",
    )
    .await
}

/// **Idempotent, and it never changes an existing bot.** A live instance has
/// one with facts on it — rules somebody wrote — so a seed that overwrote
/// would be a data-loss bug wearing a setup step's clothes. The one thing it
/// adds to a bot that exists is the box the bot lacks.
///
/// **A shipped handle is the software's own, so it is created past the name
/// screen.** A thing whose name resembles the bot must not keep the bot from
/// existing, and the seed sends no caller's override: it hands back the token
/// the screen's own refusal minted, with a reason, and the reason is kept on
/// the bot as `told_apart_by`.
async fn ensure_identity(
    memory: &Arc<dyn Memory>,
    mailboxes: &Arc<dyn Mailboxes>,
    clock: &Clock,
    handle: &str,
    name: &str,
) -> Seeded {
    let id = EntityId(handle.to_string());
    let box_name = MailboxName(id.slug().to_string());

    match memory.list_entities(Some(EntityKind::BOT)).await {
        // A bot folded into another is no identity (rule 466): it keeps its
        // handle, so nothing is created, and no box is opened for it.
        Ok(bots) if bots.iter().any(|b| b.id == id && b.merged_into.is_some()) => {
            return Seeded::AlreadyThere;
        }
        Ok(bots) if bots.iter().any(|b| b.id == id) => {
            return match mailboxes.list_mailboxes().await {
                Ok(boxes) if boxes.iter().any(|b| b.name == box_name) => Seeded::AlreadyThere,
                Ok(_) => open_box(mailboxes, &box_name, &id, Seeded::BoxOpened).await,
                Err(e) => Seeded::Unreachable(e.to_string()),
            };
        }
        Ok(_) => {}
        Err(e) => return Seeded::Unreachable(e.to_string()),
    }

    // The box opens with the bot, inside the same act — the one place a
    // mailbox comes into being, and the reason this cannot be two calls a
    // caller could interleave.
    match memory
        .add_entity(NewEntity::new(id.clone(), name, "jojobot"))
        .await
    {
        Ok(Guarded::Written(_)) => {}
        Ok(Guarded::Blocked { candidates, .. }) => {
            if let Err(why) = create_past_the_screen(memory, clock, &id, name, &candidates).await {
                return Seeded::Unreachable(why);
            }
        }
        Err(e) => return Seeded::Unreachable(e.to_string()),
    }
    open_box(mailboxes, &box_name, &id, Seeded::Created).await
}

/// The reason a shipped handle is created over a resemblance.
const SHIPPED_REASON: &str = "a handle the software ships, created by the software itself";

/// Create a shipped bot with the token its own refusal minted.
async fn create_past_the_screen(
    memory: &Arc<dyn Memory>,
    clock: &Clock,
    id: &EntityId,
    name: &str,
    candidates: &[jojobot_domain::memory::guard::EntityMatch],
) -> Result<(), String> {
    use jojobot_domain::memory::guard;
    let mut new = NewEntity::new(id.clone(), name, "jojobot");
    new.override_token = Some(guard::override_token(id, candidates));
    new.override_reason = Some(SHIPPED_REASON.to_string());
    let mut first = NewFact::about(id.clone(), name, clock.today_in(&jiff::tz::TimeZone::UTC));
    first
        .fields
        .insert(guard::TOLD_APART_BY.to_string(), SHIPPED_REASON.to_string());
    first.sets.insert(guard::TOLD_APART_BY.to_string());
    match memory.add_entity_with_first_claim(new, first).await {
        Ok(Guarded::Written(_)) => Ok(()),
        Ok(Guarded::Blocked { candidates, .. }) => Err(format!(
            "{id} was refused even with the screen's own token, and the screen named {}",
            candidates
                .iter()
                .map(|c| c.handle.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
        Err(e) => Err(e.to_string()),
    }
}

/// Open a bot's box. The bot may have landed without its box; the next boot
/// finds that and comes here again.
async fn open_box(
    mailboxes: &Arc<dyn Mailboxes>,
    box_name: &MailboxName,
    owner: &EntityId,
    done: Seeded,
) -> Seeded {
    match mailboxes.create_mailbox(box_name, owner, None).await {
        Ok(jojobot_domain::mailbox::Guarded::Written(_)) => done,
        Ok(_) => Seeded::Unreachable(format!("the box for {owner} was refused")),
        Err(e) => Seeded::Unreachable(e.to_string()),
    }
}

/// **Make sure the instance's own record exists**, so the first write onto it
/// (the operator, the zone, a setting) needs no `add_entity` that only exists
/// to make it.
///
/// **Idempotent, and it never touches an existing record.** An instance made
/// before this ran holds one with keys on it, and a seed that overwrote would
/// be a data-loss bug wearing a setup step's clothes. The check is existence,
/// and existence alone. The record is created holding no key: every key on it
/// is somebody's to write, and the guards on its keys apply to those writes
/// exactly as they did when a caller made the record.
///
/// A stored row rather than a supplied one: a supplied record is refused at
/// boot over a stored row at its address, and every instance that has one
/// would stop booting.
pub async fn ensure_instance_record(memory: &Arc<dyn Memory>) -> Seeded {
    let id = EntityId(INSTANCE_RECORD.to_string());

    match memory.list_entities(Some(EntityKind::TOPIC)).await {
        Ok(topics) if topics.iter().any(|t| t.id == id) => return Seeded::AlreadyThere,
        Ok(_) => {}
        Err(e) => return Seeded::Unreachable(e.to_string()),
    }
    match memory
        .add_entity(NewEntity::new(id, "This instance", "jojobot"))
        .await
    {
        Ok(Guarded::Written(_)) => Seeded::Created,
        Ok(Guarded::Blocked { candidates, .. }) => Seeded::Unreachable(format!(
            "the near-miss screen refused the record and named {}",
            candidates
                .iter()
                .map(|c| c.handle.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
        Err(e) => Seeded::Unreachable(e.to_string()),
    }
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

    /// Each key with the whole token it is declared as, which `keys` cannot
    /// show: it names the value type alone, so a reference narrowed to a kind
    /// reads as a plain one.
    fn spelled(declared: &DeclaredType) -> Vec<String> {
        declared
            .fields
            .iter()
            .map(|f| format!("{}: {}", f.key, f.holds_token()))
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
            5
        );

        // **`admits` holds a reference and names no kind**, so what a pass gets
        // somebody into is not fixed to one kind of thing. The two dates are
        // the window, and a record carrying neither is live.
        let entitlement = stored(&memory, "entitlement").await;
        assert_eq!(
            keys(&entitlement),
            vec![
                ("admits", "reference"),
                ("valid_from", "date"),
                ("valid_until", "date"),
                ("tier", "text"),
            ],
        );
        assert_eq!(entitlement.origin, Origin::Shipped);
        assert!(
            entitlement
                .fields
                .iter()
                .find(|f| f.key == "admits")
                .expect("the key it walks on")
                .points_at
                .is_none(),
            "the reference names a kind, which closes a question somebody left open: {entitlement:?}",
        );

        // **A trip is a kind, so no type holds the name.** Paired with the
        // entitlement above, which this same seed did declare.
        assert!(
            memory
                .declared_types()
                .await
                .expect("the roster reads")
                .iter()
                .all(|declared| declared.name != "trip"),
            "the seed declares no type called trip",
        );

        // The same keys `RunsOut` and `DecideBy` match structurally, now
        // nameable the way every other declared type is.
        let runs_out = stored(&memory, "runs-out").await;
        assert_eq!(
            keys(&runs_out),
            vec![(jojobot_domain::attention::RUNS_OUT, "date")],
        );
        assert_eq!(runs_out.origin, Origin::Shipped);

        // **Six keys that describe their record**, each declared so by the
        // build and by nothing a caller can send.
        let labels = stored(&memory, "record-labels").await;
        assert_eq!(labels.origin, Origin::Shipped);
        assert_eq!(
            labels
                .fields
                .iter()
                .map(|f| (f.key.as_str(), f.folds))
                .collect::<Vec<_>>(),
            [
                "read_from",
                "read_ref",
                "starred",
                "subject",
                "purpose",
                "recorded_by"
            ]
            .into_iter()
            .map(|key| (key, jojobot_domain::memory::types::Fold::Describes))
            .collect::<Vec<_>>(),
        );

        let decide_by = stored(&memory, "decide-by").await;
        assert_eq!(
            keys(&decide_by),
            vec![(jojobot_domain::attention::DECIDE_BY, "date")],
        );
        assert_eq!(decide_by.origin, Origin::Shipped);

        // **The three days jojobot works out for a loop and writes on it**, each a
        // date and none required. The spellings are pinned as literals because
        // they are stored in a fields bag and nothing outside this process
        // declares them; `outcome`, the fourth computed key, is a word and not here.
        let computed = stored(&memory, "computed-dates").await;
        assert_eq!(
            keys(&computed),
            vec![
                ("due_on", "date"),
                ("last_check_in", "date"),
                ("counts_from", "date"),
            ],
        );
        assert!(
            computed.fields.iter().all(|f| !f.required),
            "a loop carries whichever of its days it has produced: {computed:?}",
        );
        assert_eq!(computed.origin, Origin::Shipped);
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
            .declare_type(
                DeclaredType::shipped(
                    "entitlement",
                    vec![Field::required("admits", ValueType::Reference)],
                ),
                None,
            )
            .await
            .expect("the software declares its own types");

        ensure_shipped_types(&memory)
            .await
            .expect("the store takes");

        assert_eq!(
            spelled(&stored(&memory, "entitlement").await),
            vec![
                "admits: reference",
                "valid_from: date",
                "valid_until: date",
                "tier: text",
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
            boxes.iter().any(|b| b.name.0 == default_bot().slug()),
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
        let other = EntityId("bot:gamma".into());
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
                    .create_mailbox(&MailboxName(default_bot().slug().into()), &other, None)
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
            boxes.iter().any(|b| b.name.0 == default_bot().slug()),
            "the default identity has a box of its own, not the near miss: {boxes:?}"
        );
    }

    /// **A thing whose name resembles a shipped bot does not keep the bot from
    /// existing.** The seed creates a shipped handle past the name screen and
    /// records why on the bot.
    #[tokio::test]
    async fn a_resembling_thing_does_not_keep_a_shipped_bot_from_existing() {
        let (memory, mailboxes) = ports();
        let neighbour = EntityId("topic:rules-springfield".into());
        memory
            .add_entity(NewEntity::new(neighbour.clone(), "Defrag", "test"))
            .await
            .expect("add ok")
            .written()
            .expect("an empty store blocks nothing");

        // The positive the verdict rests on: the screen is live against this
        // neighbour. Without it the assertions below pass on a store that
        // never refused the bot.
        let shipped = EntityId(DEFRAG_BOT.into());
        assert!(
            matches!(
                memory
                    .add_entity(NewEntity::new(shipped.clone(), "Defrag", "jojobot"))
                    .await
                    .expect("a blocked create is a result, not a failure"),
                Guarded::Blocked { .. }
            ),
            "a plain create of the shipped handle is refused by the name screen"
        );

        let seeded = ensure_shipped_identities(&memory, &mailboxes, &Clock::default()).await;
        assert_eq!(
            seeded
                .iter()
                .find(|(handle, _)| *handle == DEFRAG_BOT)
                .map(|(_, seeded)| seeded),
            Some(&Seeded::Created),
            "{seeded:?}"
        );
        let bots = memory
            .list_entities(Some(EntityKind::BOT))
            .await
            .expect("list ok");
        assert!(bots.iter().any(|b| b.id == shipped), "the bot exists");
        let boxes = mailboxes.list_mailboxes().await.expect("list ok");
        assert!(
            boxes.iter().any(|b| b.name.0 == shipped.slug()),
            "the bot has its box: {boxes:?}"
        );
        let facts = memory.recall(&shipped).await.expect("recall ok");
        assert!(
            facts.iter().any(|f| f
                .fields
                .get(jojobot_domain::memory::guard::TOLD_APART_BY)
                .is_some_and(|why| !why.trim().is_empty())),
            "the reason the seed insisted is kept on the bot: {facts:?}"
        );
    }

    /// **A shipped bot that exists without its box gets the box at the next
    /// boot**, and a bot that has both is left alone.
    #[tokio::test]
    async fn a_shipped_bot_with_no_box_gets_its_box() {
        let (memory, mailboxes) = ports();
        let shipped = EntityId(DEFRAG_BOT.into());
        memory
            .add_entity(NewEntity::new(shipped.clone(), "Defrag", "jojobot"))
            .await
            .expect("add ok")
            .written()
            .expect("an empty store blocks nothing");
        assert!(
            mailboxes
                .list_mailboxes()
                .await
                .expect("list ok")
                .is_empty(),
            "the bot starts without a box"
        );

        let first = ensure_shipped_identities(&memory, &mailboxes, &Clock::default()).await;
        assert_eq!(
            first
                .iter()
                .find(|(handle, _)| *handle == DEFRAG_BOT)
                .map(|(_, seeded)| seeded),
            Some(&Seeded::BoxOpened),
            "{first:?}"
        );
        let boxes = mailboxes.list_mailboxes().await.expect("list ok");
        assert!(
            boxes.iter().any(|b| b.name.0 == shipped.slug()),
            "the box is there: {boxes:?}"
        );

        let again = ensure_shipped_identities(&memory, &mailboxes, &Clock::default()).await;
        assert!(
            again
                .iter()
                .all(|(_, seeded)| *seeded == Seeded::AlreadyThere),
            "a bot with its box is left alone: {again:?}"
        );
    }

    /// **A shipped bot folded into another is no identity, so the seed opens no
    /// box for it** — and an unfolded shipped bot with no box still gets one in
    /// the same pass, so an answer that opens nothing cannot pass.
    #[tokio::test]
    async fn a_folded_shipped_bot_gets_no_box_and_an_unfolded_one_does() {
        let (memory, mailboxes) = ports();
        let survivor = EntityId("bot:gamma".into());
        for (id, name) in [
            (EntityId(DEFAULT_BOT.into()), "Assistant"),
            (EntityId(DEFRAG_BOT.into()), "Defrag"),
            (survivor.clone(), "Gamma"),
        ] {
            memory
                .add_entity(NewEntity::new(id, name, "test"))
                .await
                .expect("add ok")
                .written()
                .expect("an empty store blocks nothing");
        }
        memory
            .merge(
                &EntityId(DEFAULT_BOT.into()),
                &survivor,
                None,
                jiff::civil::date(2026, 7, 1),
                &survivor,
                None,
            )
            .await
            .expect("merge ok");

        let seeded = ensure_shipped_identities(&memory, &mailboxes, &Clock::default()).await;
        let answer = |handle: &str| {
            seeded
                .iter()
                .find(|(h, _)| *h == handle)
                .map(|(_, seeded)| seeded)
        };
        assert_eq!(answer(DEFRAG_BOT), Some(&Seeded::BoxOpened), "{seeded:?}");
        assert_eq!(
            answer(DEFAULT_BOT),
            Some(&Seeded::AlreadyThere),
            "{seeded:?}"
        );
        let boxes = mailboxes.list_mailboxes().await.expect("list ok");
        assert!(
            boxes.iter().any(|b| b.name.0 == "defrag"),
            "the unfolded bot has its box: {boxes:?}"
        );
        assert!(
            boxes.iter().all(|b| b.name.0 != "assistant"),
            "the folded bot has none: {boxes:?}"
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
        let id = default_bot();
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
    /// **A fresh instance gets the record, holding no key.** Every key on it is
    /// somebody's to write, so the seed writes none.
    #[tokio::test]
    async fn a_fresh_instance_gets_its_record_holding_no_key() {
        let (memory, _) = ports();
        assert_eq!(ensure_instance_record(&memory).await, Seeded::Created);

        let id = EntityId(INSTANCE_RECORD.to_string());
        let held = memory
            .list_entities(Some(EntityKind::TOPIC))
            .await
            .expect("list ok");
        assert_eq!(held.iter().filter(|t| t.id == id).count(), 1, "{held:?}");
        assert!(
            memory.fields(&id).await.expect("fields read").is_empty(),
            "the record holds no key until somebody writes one"
        );
    }

    /// **A record somebody made already is left exactly as it is.** The seed
    /// recognises it by existence, so the operator and the zone written onto it
    /// survive every boot; and a second seed writes nothing.
    #[tokio::test]
    async fn seeding_never_touches_an_instance_record_that_already_exists() {
        let (memory, _) = ports();
        let id = EntityId(INSTANCE_RECORD.to_string());
        memory
            .add_entity(NewEntity::new(id.clone(), "Our instance", "user-named"))
            .await
            .expect("add ok")
            .written()
            .expect("an empty board blocks nothing");
        let mut fact = jojobot_domain::memory::NewFact::about(
            id.clone(),
            "who the operator is and where they live",
            jiff::civil::date(2026, 7, 1),
        );
        fact.fields.insert("timezone".into(), "Etc/GMT+12".into());
        fact.sets.insert("timezone".into());
        memory.capture(fact).await.expect("capture ok");
        let before = memory.fields(&id).await.expect("fields read");
        assert_eq!(
            before.get("timezone").map(String::as_str),
            Some("Etc/GMT+12")
        );

        assert_eq!(
            ensure_instance_record(&memory).await,
            Seeded::AlreadyThere,
            "a seed recognises the record rather than remaking it"
        );
        assert_eq!(
            memory.fields(&id).await.expect("fields read"),
            before,
            "what was written on the record survived the seed"
        );
        let topics = memory
            .list_entities(Some(EntityKind::TOPIC))
            .await
            .expect("list ok");
        assert_eq!(topics.len(), 1, "and no second record appeared: {topics:?}");
        assert_eq!(topics[0].name, "Our instance", "nor was it renamed");
    }

    /// **A near-miss the screen finds is reported, not swallowed.** The seed
    /// sends no override token, so a neighbouring record refuses the creation;
    /// the answer names it and writes nothing.
    #[tokio::test]
    async fn a_neighbouring_topic_that_refuses_the_record_is_named() {
        let (memory, _) = ports();
        memory
            .add_entity(NewEntity::new(
                EntityId("topic:instance-2".into()),
                "Instance two",
                "user-named",
            ))
            .await
            .expect("add ok")
            .written()
            .expect("an empty board blocks nothing");

        let Seeded::Unreachable(why) = ensure_instance_record(&memory).await else {
            panic!("the screen refuses a record one letter from another topic");
        };
        assert!(why.contains("topic:instance-2"), "{why}");
        assert_eq!(
            memory
                .list_entities(Some(EntityKind::TOPIC))
                .await
                .expect("list ok")
                .len(),
            1,
            "nothing was written"
        );
    }
}
