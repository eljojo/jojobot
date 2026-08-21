//! **The set of kinds, held in memory and filled from the store.**
//!
//! A kind is the namespace in a handle and the schema of the thing it names
//! (rule 213). The set of them was a closed enum compiled into this crate,
//! which made the nouns one person's life contains a fact about the software
//! — the violation this repository names everywhere else: nothing
//! instance-specific is compiled in.
//!
//! So the set is a **declaration in the store**, and this module is where a
//! process keeps the copy it parses against. **The set is loaded, never read
//! per parse**: a handle is parsed constantly, and a lookup that reached the
//! store on every parse would be a performance defect shipped as a feature.
//!
//! # What is NOT here
//!
//! **No keys and no caller-declared kinds.** A kind's schema and a caller's
//! way of adding one are later work; this holds the set and answers whether a
//! token is in it.

use std::collections::BTreeSet;
use std::sync::{OnceLock, RwLock};

use super::EntityKind;

/// **The kinds the software ships**, in the order they are seeded and listed.
///
/// They are here as tokens rather than as a type, because the point of the
/// change is that the set is data. What makes the shipped ones different from any
/// other kind is not the compiler: it is that a seed writes them at every
/// startup and a caller cannot redeclare one.
pub const SHIPPED: [&str; 13] = [
    "person", "project", "place", "event", "work", "thing", "org", "topic", "bot", "pet", "rhythm",
    "machine", "view",
];

/// **The keys a shipped kind carries**, and almost all of them carry none.
///
/// What a `person` or a `place` holds is not the software's to decide, and a
/// kind that names no key is coherent: its identity is its row. A kind names
/// keys only where the software knows the shape — where the thing exists
/// BECAUSE the software has a use for it.
///
/// **`rhythm` is the one, and it declares the WHOLE loop.** Every key the
/// check-in verb writes and the overdue read reads is named here: the two the
/// arithmetic needs, the policy that chooses between them, what the check-in
/// found, and the note. A key the machinery uses and the kind does not name is
/// a second vocabulary, and a session learns whichever it reads first.
///
/// **Its required set is two keys.** A loop is a name and the day somebody last
/// looked at it; everything else is what they had to hand at the time. A
/// required key is a refusal waiting to happen, and a loop nobody has written a
/// cadence for is still a loop.
pub fn keys_of(token: &str) -> Vec<super::types::Field> {
    use super::types::{Field, ValueType};
    match token {
        "rhythm" => vec![
            Field::required("name", ValueType::Text),
            // **The day of the last check-in**, which is what "when did I last
            // look at this" reads. It is not "the day it last ran": a check-in
            // records what was found, and a refusal is not a run — a key by
            // that name would be written by a skipped cycle and would say the
            // loop ran on a day the record says it did not.
            Field::required("last_check_in", ValueType::Date),
            // **The one thing to carry forward.** A loop that ran and left
            // something to remember is the ordinary case, not the exception.
            Field::new("note", ValueType::Text),
            // **Days between one turn and the next**, and the unit is in the
            // name on purpose: a cadence is always TIME. What a check-in
            // measures — a distance, a reading, a count — is a field on the
            // check-in rather than a unit of the schedule, and a bare
            // `cadence` invites the schedule to grow one.
            //
            // Optional because not every loop has one, and here rather than
            // dropped because a bill paid monthly needs it: an absent column
            // is not an absent requirement.
            Field::new("cadence_days", ValueType::Number),
            // **Which date the next cycle counts from when a check-in is
            // late** — the day it fell due, or the day the check-in happened.
            // The two diverge exactly when a check-in is late, which is
            // exactly when picking wrong stops being visible.
            //
            // **The set is the arithmetic's own vocabulary rather than a copy
            // of it**, for the reason the outcome key below reads its own: a
            // second list of the same tokens is two vocabularies that drift.
            // Free text here cost more than elsewhere — a mistyped value stored
            // silently is a value the arithmetic cannot read, and the loop then
            // reads overdue every day.
            Field::one_of(
                "advances_from",
                crate::attention::AdvancesFrom::ALL.map(crate::attention::AdvancesFrom::as_token),
            ),
            // **The date this cycle counts from**, and the loop is next due a
            // cadence after it. It is a second date rather than the same one
            // because what separates the three outcomes is whether the cycle
            // is consumed: a snooze moves this one nowhere while the check-in
            // above still records the contact.
            Field::new("counts_from", ValueType::Date),
            // **What the last check-in found**, and the set is the check-in's
            // own vocabulary rather than a copy of it. A second list of the
            // same three tokens is two vocabularies that drift, and a session
            // learns whichever it reads first.
            Field::one_of(
                "outcome",
                crate::attention::Outcome::ALL.map(crate::attention::Outcome::as_token),
            ),
        ],
        // **A question asked by name.** The keys are what a view IS: what it
        // selects over, and what it keeps. They are required because a view
        // missing either is a question nobody can ask — unlike a loop, where
        // an absent cadence still leaves a loop.
        "view" => vec![
            Field::required("selects", ValueType::Text),
            Field::new("where_key", ValueType::Text),
            Field::new("where_is", ValueType::Text),
            Field::new("follow", ValueType::Text),
            Field::new("asks", ValueType::Text),
        ],
        _ => Vec::new(),
    }
}

/// The set this process parses against. Empty until something loads it.
fn loaded() -> &'static RwLock<BTreeSet<String>> {
    static LOADED: OnceLock<RwLock<BTreeSet<String>>> = OnceLock::new();
    LOADED.get_or_init(|| RwLock::new(BTreeSet::new()))
}

/// **Fill the set from what the store holds.** Called once at startup, and
/// again by anything that has re-read the declarations.
///
/// It replaces rather than merges: a kind that has gone from the store has
/// gone, and a merge would keep it alive in a running process for as long as
/// the process lives.
pub fn load<I, S>(tokens: I)
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let filled: BTreeSet<String> = tokens.into_iter().map(Into::into).collect();
    *loaded().write().expect("the kind set is not poisoned") = filled;
}

/// Is this token a kind this process knows?
///
/// **An empty set answers no to everything**, and that is deliberate rather
/// than a gap: a process that has not loaded the set cannot tell a kind from a
/// typo, and answering yes would file a record under a noun nobody declared.
/// What stops that being a silent outage is the caller — a handle whose kind
/// is unknown is refused with the kinds that ARE known, so an empty set is
/// visible in the refusal rather than inferred from behaviour.
pub fn known(token: &str) -> bool {
    loaded()
        .read()
        .expect("the kind set is not poisoned")
        .contains(token)
}

/// **Why a token is not a kind here.** Two failures, two repairs, and they
/// must not arrive wearing one sentence: a process that never loaded the set
/// refuses `person` exactly as it refuses a typo, and a reader told "unknown
/// kind" about the most ordinary handle in the system goes hunting for a
/// misspelling that is not there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotAKind {
    /// **Nothing seeded this process.** No token can be a kind, including the
    /// ten the software ships. The repair is a boot that loads the set, never
    /// a change to the handle.
    SetNeverLoaded,
    /// The set is loaded and this token is not in it. The repair is a kind
    /// somebody declares, or the handle spelled the way its kind is.
    NotDeclared {
        /// What the caller asked for.
        token: String,
        /// The kinds this process does know, so the refusal carries a way
        /// forward rather than only a no.
        known: Vec<String>,
    },
}

impl std::fmt::Display for NotAKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotAKind::SetNeverLoaded => f.write_str(
                "the kind set was never loaded, so no handle can be read: this process has \
                 seeded nothing. The repair is a boot that reaches the store, not a different \
                 handle — nothing about the one you sent is wrong",
            ),
            NotAKind::NotDeclared { token, known } => {
                write!(
                    f,
                    "no kind is named '{token}'. The kinds here are: {}",
                    known.join(", ")
                )
            }
        }
    }
}

/// **The kind a token names**, or which kind of nothing it found.
pub fn resolve(token: &str) -> Result<EntityKind, NotAKind> {
    let set = loaded().read().expect("the kind set is not poisoned");
    if set.is_empty() {
        return Err(NotAKind::SetNeverLoaded);
    }
    match set.get(token) {
        Some(held) => Ok(EntityKind::of(intern(held))),
        None => Err(NotAKind::NotDeclared {
            token: token.to_string(),
            known: set.iter().cloned().collect(),
        }),
    }
}

/// **A loaded token, with the lifetime a kind carries.** A kind is `Copy` and
/// hands out `&'static str`, so a token that arrived at runtime is leaked
/// deliberately: the set is small, loaded at startup, and a token that has
/// been a kind once stays readable for as long as the process runs.
pub fn intern(token: &str) -> &'static str {
    static INTERNED: OnceLock<RwLock<BTreeSet<&'static str>>> = OnceLock::new();
    let interned = INTERNED.get_or_init(|| RwLock::new(BTreeSet::new()));
    if let Some(held) = interned
        .read()
        .expect("the tokens are not poisoned")
        .get(token)
    {
        return held;
    }
    let mut writing = interned.write().expect("the tokens are not poisoned");
    if let Some(held) = writing.get(token) {
        return held;
    }
    let held: &'static str = Box::leak(token.to_string().into_boxed_str());
    writing.insert(held);
    held
}

/// **Reconcile the kinds this build ships, then load what the store holds.**
///
/// **The order is the point, and it is write-then-read rather than read.** A
/// seed that read first could not tell an instance whose kinds are missing
/// from one that never had them, and would then serve a set the software
/// believes in and the store does not hold. Writing first makes those the same
/// instance.
///
/// **[`SHIPPED`] is authoritative, so this is a reconcile and not an upsert**
/// (rule 234). Writing alone reaches an instance with what a later build ADDS
/// and nothing else: a kind an older build shipped and this one dropped would
/// sit on every upgraded instance for ever, indistinguishable from one the
/// operator declared. So what the store holds as the software's and this build
/// does not ship is taken back — see [`owned`](super::owned) for the marker
/// that tells those two rows apart.
///
/// **What is loaded is the store's answer**, not the list above it: a kind a
/// caller declared is parsed by this process, and a shipped kind the store
/// somehow lost stops being parsed rather than being kept alive by the code
/// that just wrote it. The second read is taken only when reclaiming changed
/// something, and it is a read of the store for the same reason the first is.
///
/// It lives here rather than beside the boot because the contract cases need
/// the same two steps, and a second copy of them is where the two would drift.
pub async fn seed<M: super::Memory + ?Sized>(store: &M) -> Result<usize, super::MemoryError> {
    for token in SHIPPED {
        store
            .declare_kind(token, super::types::Origin::Shipped, keys_of(token))
            .await?;
    }
    let mut held = store.declared_kinds().await?;
    let stale = super::owned::reclaimed(
        held.iter().map(|(token, origin)| (token.as_str(), *origin)),
        SHIPPED,
    );
    if !stale.is_empty() {
        for token in &stale {
            store.reclaim_kind(token).await?;
        }
        held = store.declared_kinds().await?;
    }
    load(held.iter().map(|(token, _)| token.clone()));
    Ok(held.len())
}

/// **Re-read the set from the store**, for a process that has just declared a
/// kind and must be able to read a handle carrying it.
///
/// A declaration writes a row; the set this process parses against is a copy
/// taken at the boot. Without this, a kind a caller declares is one their very
/// next call cannot use, and the refusal would say nobody declared it — while
/// the store says somebody did.
pub async fn reload<M: super::Memory + ?Sized>(store: &M) -> Result<usize, super::MemoryError> {
    let held = store.declared_kinds().await?;
    load(held.iter().map(|(token, _)| token.clone()));
    Ok(held.len())
}

/// **Load the shipped kinds, for a test that parses a handle without a store.**
///
/// A store is what holds the declarations, so a case that opens one — real or
/// double — gets the set with it. A case that builds records by hand does not,
/// and a handle it parses is refused for a reason that has nothing to do with
/// what the case is about.
///
/// **It is `pub` and not test-gated on purpose**: the crates above this one
/// have their own suites with the same need, and a second copy of this call in
/// each of them is the drift this module exists to end.
pub fn load_shipped() {
    load(SHIPPED);
}

/// Every kind this process knows, for a refusal to name and a caller to list.
pub fn all() -> Vec<String> {
    loaded()
        .read()
        .expect("the kind set is not poisoned")
        .iter()
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attention::Outcome;

    /// **The key choosing which date a late cycle counts from holds the two
    /// tokens the arithmetic accepts, and there is only one list of them.**
    ///
    /// Free text here is worse than free text elsewhere: a mistyped value is
    /// stored silently, the arithmetic cannot read it, and the loop then reads
    /// overdue every day — a nag nobody asked for, from a typo nothing
    /// reported.
    ///
    /// Both halves, for the reason the outcome case has both: every token the
    /// policy accepts is a value the key takes, and the key takes nothing else.
    #[test]
    fn the_schedule_policy_key_holds_the_two_choices_the_arithmetic_reads() {
        let advances = keys_of("rhythm")
            .into_iter()
            .find(|f| f.key == crate::attention::ADVANCES_FROM)
            .expect("the rhythm kind names the policy key");
        for choice in crate::attention::AdvancesFrom::ALL {
            assert!(
                advances.accepts(choice.as_token()),
                "the key takes {:?}, which the arithmetic reads",
                choice.as_token(),
            );
        }
        assert_eq!(
            advances.one_of.as_deref().map(<[String]>::len),
            Some(crate::attention::AdvancesFrom::ALL.len()),
            "…and it names those and no others: {advances:?}",
        );
        assert!(
            !advances.accepts("whenever"),
            "a value the arithmetic cannot read is not one the key holds either",
        );
    }

    /// **The loop's outcome key holds the check-in's own vocabulary, and there
    /// is only one list of it.**
    ///
    /// Both halves, because each covers how the other passes on a build that is
    /// wrong: every token the verb accepts is a value the key takes, and the key
    /// takes nothing else. A declaration that had copied the three words would
    /// pass today and fail the first time the verb learns a fourth — which is
    /// the drift this asserts against, not the agreement it has right now.
    #[test]
    fn the_outcome_key_holds_the_check_ins_own_vocabulary() {
        let outcome = keys_of("rhythm")
            .into_iter()
            .find(|f| f.key == crate::attention::OUTCOME)
            .expect("the rhythm kind names the outcome key");
        for token in Outcome::ALL {
            assert!(
                outcome.accepts(token.as_token()),
                "the key takes {:?}, which is a word the check-in verb accepts",
                token.as_token(),
            );
        }
        assert_eq!(
            outcome.one_of.as_deref().map(<[String]>::len),
            Some(Outcome::ALL.len()),
            "…and it names those and no others: {outcome:?}",
        );
        assert!(
            !outcome.accepts("swapped"),
            "a word the verb does not accept is not one the key holds either",
        );
    }
}
