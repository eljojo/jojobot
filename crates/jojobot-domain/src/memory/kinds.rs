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
pub const SHIPPED: [&str; 21] = [
    "person",
    "project",
    "place",
    "event",
    "work",
    "thing",
    "org",
    "topic",
    "bot",
    "pet",
    "rhythm",
    "promise",
    "machine",
    "view",
    "session",
    "thread",
    "decision",
    "role",
    "voucher",
    "challenge",
    "document",
];

/// **The statuses a `work` thing or a `project` holds**, in the order work
/// moves through them: put off, up next, under way, blocked on somebody, over.
///
/// A closed set, and the build's: a status a cold session can ask "what is
/// next" of has to be a word every writer spells the same way. A project that
/// needs a finer state says it under a key of its own, which no kind refuses,
/// or lists the word under `columns`; a word outside the lists still lands, and
/// the answer to the write says so.
pub const WORK_STATUSES: [&str; 5] = ["someday", "next", "now", "waiting", "done"];

/// **The status that finishes a piece of work**: the last of the five. A work
/// item or a project holding it is not owed.
pub const FINISHED: &str = WORK_STATUSES[4];

/// The key a work thing or a project holds its status under.
pub const STATUS: &str = "status";

/// **The shipped kinds that hold a status and finish at [`FINISHED`]**, in the
/// order the kinds are shipped.
pub fn finishing_kinds() -> Vec<&'static str> {
    SHIPPED
        .iter()
        .copied()
        .filter(|kind| holds_columns(kind))
        .collect()
}

/// **The prefix of the keys that say what an instance's own status word
/// means.** `status_word/considering` set to `someday` makes `considering` read
/// as `someday` wherever a status is read. The key lives on the instance's
/// record, beside its `columns`.
pub const STATUS_WORD: &str = "status_word/";

/// **What a status word means**, as one of the shipped five, or `None` when the
/// instance has not said.
///
/// A shipped word means itself, whatever a stored mapping says, so `done` is
/// never remapped. Any other word means what the instance maps it to, provided
/// that is a shipped word; a mapping to anything else maps nothing. An unmapped
/// word has no meaning here, and a caller reads that as not finished.
pub fn meaning_of<'a>(
    word: &'a str,
    instance: &'a std::collections::BTreeMap<String, String>,
) -> Option<&'a str> {
    let word = word.trim();
    if let Some(shipped) = WORK_STATUSES.iter().find(|shipped| **shipped == word) {
        return Some(shipped);
    }
    let mapped = instance.get(&format!("{STATUS_WORD}{word}"))?.trim();
    WORK_STATUSES
        .iter()
        .find(|shipped| **shipped == mapped)
        .copied()
}

/// **The words a work item's or project's status is accepted at**, in the order
/// a reader walks them: the shipped five, then the instance's own columns, then
/// the governing project's, each word once. A project cannot shut out a word
/// the instance lists, because both lists are added to the five and neither
/// replaces it. `project` is the governing project's folded fields, or `None`
/// for a thing under none.
pub fn accepted_words(
    instance: &std::collections::BTreeMap<String, String>,
    project: Option<&std::collections::BTreeMap<String, String>>,
) -> Vec<String> {
    let mut words: Vec<String> = WORK_STATUSES.iter().map(|word| word.to_string()).collect();
    for listed in [columns_of(instance), project.and_then(columns_of)]
        .into_iter()
        .flatten()
        .flatten()
    {
        if !words.contains(&listed) {
            words.push(listed);
        }
    }
    words
}

/// **Whether a thing of this kind is finished**: a work item or a project whose
/// status means `done`, by the shipped word or by a word the instance maps to
/// it. A finished thing is not owed, and its date stays on the record. A word
/// nobody mapped is not finished. Every other kind owes by its dates alone,
/// whatever a `status` key on it says.
pub fn is_finished(
    kind: &str,
    fields: &std::collections::BTreeMap<String, String>,
    instance: &std::collections::BTreeMap<String, String>,
) -> bool {
    holds_columns(kind)
        && fields
            .get(STATUS)
            .is_some_and(|held| meaning_of(held, instance) == Some(FINISHED))
}

/// The key a project holds its own ordered list of statuses under.
pub const COLUMNS: &str = "columns";

/// **The keys a shipped kind carries**, and most of them carry none.
///
/// What a `person` or a `place` holds is not the software's to decide, and a
/// kind that names no key is coherent: its identity is its row. A kind names
/// keys only where the software knows the shape — where the thing exists
/// BECAUSE the software has a use for it. `work` and `project` name the
/// project-management keys below; `promise` and `view` name theirs.
///
/// **`rhythm` declares the WHOLE loop.** Every key the
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
            // **The day a snooze lasts until.** The loop falls due on the later
            // of this day and its own, and the next consuming check-in spends
            // it. Optional: most loops are never snoozed.
            Field::new(crate::attention::SNOOZED_UNTIL, ValueType::Date),
            // **What the last check-in found**, and the set is the check-in's
            // own vocabulary rather than a copy of it. A second list of the
            // same three tokens is two vocabularies that drift, and a session
            // learns whichever it reads first.
            Field::one_of(
                "outcome",
                crate::attention::Outcome::ALL.map(crate::attention::Outcome::as_token),
            ),
        ],
        // **A piece of work, and the keys a bot's own work needs so that none
        // invents its own.** All optional: a work thing is whole with none of
        // them, and a required key is a refusal waiting to happen. Each is a
        // question a cold session asks — what is next, what waits on whom, what
        // stands behind what — answered from the keys instead of from prose.
        //
        // **`owner` and `waiting_on` are references, and `depends_on` a list of
        // them**, because a handle written there is a link whatever the key
        // is called, and the declaration is what makes a record findable from
        // each target. **No `due_on`**: how a key passes from a record up to
        // its thing is not settled, and a deadline-shaped key here would
        // multiply that defect.
        "work" => vec![
            status_key(),
            Field::new("owner", ValueType::Reference),
            Field::new("waiting_on", ValueType::Reference),
            Field::listing("depends_on", ValueType::Reference),
            Field::new("commit", ValueType::Text),
            Field::new("verified_by", ValueType::Text),
        ],
        // **A project holds the same status, and the columns its work moves
        // through.** One word has one meaning across the two kinds, so one
        // function builds the key.
        //
        // **`columns` is the project's own ordered list of statuses**, a
        // superset of the shipped five: the project places those where it likes
        // and adds its own beside them. A work item filed under the project is
        // held to that list on every write that sets its status, and the order
        // is the one a reader walks. A project that lists none holds its work to
        // the shipped five in their shipped order.
        "project" => vec![status_key(), Field::listing(COLUMNS, ValueType::Text)],
        // **A thing somebody has to do by a day.** The keys are what the
        // owed-and-late read asks of it and what a question about it needs, and
        // nothing that describes. **One key is required: the day.** A promise
        // with no day owes nothing, and one whose day is taken off silently
        // stops being owed, so once it holds a day no write may take it away.
        // `regarding` is what it is about, optional because a promise with
        // nothing to point at is still a promise. `ended` is how it ended,
        // and the three words are told apart afterwards because *which
        // promises were kept* and *which stopped mattering* are different
        // questions; a promise carrying none is open. Being surfaced ends
        // nothing.
        "promise" => vec![
            Field::required(crate::attention::PROMISED_BY, ValueType::Date),
            Field::new(crate::attention::REGARDING, ValueType::Reference),
            Field::one_of(
                crate::attention::ENDED,
                crate::attention::PromiseEnd::ALL.map(crate::attention::PromiseEnd::as_token),
            ),
        ],
        // **Who a bot reports to**, the handle of its manager. A handle held in
        // a field is a link whatever the key is called; declaring the key is
        // what lets the chart be walked by its name, out to a manager and in to
        // everyone who reports there. Optional, because a bot is whole without
        // one. Display only: nothing here acts on it.
        "bot" => vec![Field::pointing_at("reports_to", super::EntityKind::BOT)],
        // **Who the operator is**, held on the instance's own `topic` record
        // and read by the boot. A reference to a person, so a handle written
        // there is a link and a piece of work that waits on the operator holds
        // the same handle under `waiting_on`. Optional: an instance is whole
        // before anybody has made the operator an entity. It decides whose box
        // operator mail lands in, so the build guards who may write it: see
        // `GUARDED_KEYS`.
        //
        // **The same record lists the instance's own `columns`**, the words its
        // work may hold beside the shipped five, and maps those words onto
        // shipped ones with `status_word/<word>` keys. The `columns` key is
        // built as a project's is. **The instance's list ADDS words and need not
        // repeat the shipped five**, which a project's list must hold.
        "topic" => vec![
            Field::pointing_at("operator", super::EntityKind::PERSON),
            Field::listing(COLUMNS, ValueType::Text),
        ],
        // **A question asked by name.** No key is required. What a view needs is
        // something that narrows it, and what narrows is `graph::Narrowing`, the
        // one list a recall's own selection, the refusal of a view asked as it
        // stands and the texts that name the keys all read. A view that asks what
        // is overdue looks at every kind that owes something and names no kind, so
        // the shipped `view:due` holds no `selects`. A view holding none of those
        // is refused when it is asked, which is where that is guarded.
        "view" => vec![
            // **What the question looks at** — the kind it selects. Optional: an
            // `overdue` ask names no kind (see below).
            Field::new("selects", ValueType::Text),
            // **What of each one comes back**, as the names the read already
            // uses: `facts`, `prose`, `charter`. Optional, and a view that
            // names none answers with the things themselves.
            Field::new("shows", ValueType::Text),
            // **A question the graph answers that a selection cannot** —
            // `overdue`, which is arithmetic over two dates and a cadence, and
            // `ready` and `plan`, which ask about the things a reference list
            // names. They are named asks rather than filters because none is a
            // value held on the thing itself.
            Field::one_of(
                "asks",
                ["overdue", super::readiness::ASK, super::readiness::ASK_PLAN],
            ),
            // **Which things, as one expression over keys** — the line a recall's
            // `where` takes. A handle written in it is bound to the permanent id
            // when the view is written, so a rename does not break the view.
            Field::new("where", ValueType::Expression),
            // **What `where` is asked of**: what each thing holds now (`thing`,
            // the default), or what one record says.
            Field::one_of("scope", ["thing", "record"]),
            // **The order the things come back in**: a key, then `ascending` or
            // `descending`. Text, read when the view runs.
            Field::new("order", ValueType::Text),
            // **What the view takes**, as `name: type` separated by commas, where a
            // type is `kind <kind>`, `date`, `number` or `text` and a trailing `?`
            // lets a call leave the input out. The question uses an input as
            // `$name` in `where`, as `parent` and as `answers_type`; see
            // [`super::view_inputs`].
            Field::new(super::view_inputs::KEY, ValueType::Text),
            // **The day an overdue ask is asked about**, an input written `$name`.
            // A view that asks what is overdue needs no `selects`: the question
            // already answers only the kinds that owe something.
            Field::new(super::view_inputs::AS_OF, ValueType::Text),
            // **Only the direct children of this thing**, a handle or an input
            // written `$name`. Text, read when the view runs.
            Field::new("parent", ValueType::Text),
        ],
        // **A seat, and the three keys the software knows about it.** All optional:
        // a released role has no holder, and a role nobody has claimed has none of
        // them. `claimed_at` is text because it holds a MOMENT, to the second, and
        // a lease is 45 minutes; the date type holds a day. A watcher's marks about
        // the seat are plain keys beside these and no kind refuses them.
        "role" => vec![
            Field::new(crate::session::ROLE_HOLDER, ValueType::Text),
            Field::new(crate::session::ROLE_CLAIMED_AT, ValueType::Text),
            Field::new("agent", ValueType::Text),
        ],
        // **A grant to do one gated act**, and every key is optional: only the
        // engine writes one, whole, through `crate::voucher::Voucher`, so a
        // required key would guard nothing a caller can reach. `holder` and
        // `issuer` are sids and stay text, because the handle of a run is not
        // its sid. `target` names a bot or a role, so it is text too. `parent`
        // is the link back towards the root and names a voucher.
        "voucher" => vec![
            Field::new(crate::voucher::RIGHT, ValueType::Text),
            Field::new(crate::voucher::TARGET, ValueType::Text),
            Field::new(crate::voucher::HOLDER, ValueType::Text),
            Field::new(crate::voucher::ISSUER, ValueType::Text),
            Field::pointing_at(crate::voucher::PARENT, super::EntityKind::VOUCHER),
            Field::one_of(
                crate::voucher::STATE,
                crate::voucher::VoucherState::ALL.map(crate::voucher::VoucherState::as_token),
            ),
        ],
        // **A code waiting to be presented**, every key optional for the reason a
        // voucher's are: only the engine writes one, whole. `bot` names the bot
        // the code is for and is a link. `issued_at` holds a MOMENT, to the
        // second, so it is text; a window is ten minutes and the date type holds
        // a day.
        "challenge" => vec![
            Field::pointing_at(crate::challenge::BOT, super::EntityKind::BOT),
            Field::new(crate::challenge::CODE_HASH, ValueType::Text),
            Field::one_of(
                crate::challenge::STATE,
                crate::challenge::ChallengeState::ALL
                    .map(crate::challenge::ChallengeState::as_token),
            ),
            Field::new(crate::challenge::ISSUED_AT, ValueType::Text),
            Field::new(crate::challenge::WRONG_TRIES, ValueType::Number),
        ],
        // **What a reader needs before opening a document**, all optional. The
        // title is the thing's own name, not a key. `one_liner` is the one line
        // saying what it is, the same key a bot carries for the same purpose.
        // `stale_after` is the day past which the document promises nothing, and
        // `resource` the page it mirrors when one still lives elsewhere.
        "document" => vec![
            Field::new("one_liner", ValueType::Text),
            Field::new("stale_after", ValueType::Date),
            Field::new("resource", ValueType::Text),
        ],
        _ => Vec::new(),
    }
}

/// **Whether a thing of this kind moves through the columns of the project it
/// is filed under** — work does, and so does a project filed under another.
pub fn holds_columns(kind: &str) -> bool {
    kind == "work" || kind == "project"
}

/// **The columns a project lists**, in the order it wrote them — `None` when it
/// lists none, which means its work is held to [`WORK_STATUSES`].
pub fn columns_of(fields: &std::collections::BTreeMap<String, String>) -> Option<Vec<String>> {
    let items: Vec<String> = fields
        .get(COLUMNS)?
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect();
    (!items.is_empty()).then_some(items)
}

/// **The `status` key both work-shaped kinds declare**, built once so the two
/// cannot name different words.
fn status_key() -> super::types::Field {
    super::types::Field::one_of(STATUS, WORK_STATUSES)
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
///
/// **The set belongs to the process, not to any one store.** A caller that
/// opens a second store in the same process must not rely on this call to
/// keep the two apart — it does not, and it is not the layer that could.
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

    /// **A document declares what a reader needs before opening it, and requires
    /// none of it.** Its title is the thing's own name; the one line saying what
    /// it is, the day past which it promises nothing, and the page it mirrors are
    /// optional keys. A document is whole with none of them.
    #[test]
    fn the_document_kind_declares_its_own_optional_fields() {
        use super::super::types::ValueType;
        assert!(SHIPPED.contains(&"document"), "document is a shipped kind");
        let declared = keys_of("document");
        let keys: Vec<(&str, ValueType)> = declared
            .iter()
            .map(|field| (field.key.as_str(), field.holds))
            .collect();
        assert_eq!(
            keys,
            [
                ("one_liner", ValueType::Text),
                ("stale_after", ValueType::Date),
                ("resource", ValueType::Text),
            ]
        );
        assert!(
            declared.iter().all(|field| !field.required),
            "a document is whole with none of them: {declared:?}"
        );
    }

    /// **A view requires no key of its own.** A view that asks what is overdue
    /// looks at every kind that owes something, so it names no kind to select,
    /// and a kind that demanded `selects` would call the shipped `view:due`
    /// incomplete. What a view needs is something that narrows it, and the
    /// ask-time refusal guards that.
    #[test]
    fn the_view_kind_requires_no_key_so_an_overdue_view_is_whole_without_selects() {
        let declared = keys_of("view");
        assert!(
            declared.iter().any(|field| field.key == "selects"),
            "selects is still a key a view may hold: {declared:?}"
        );
        assert!(
            declared.iter().all(|field| !field.required),
            "a view that asks what is overdue selects no kind: {declared:?}"
        );
    }

    /// **The loop kind declares the day a snooze lasts until, as a date.** The
    /// kind is what licenses an ordering on it, so *which loops are put off
    /// past the 20th* can be asked, and the key is one the loop carrier reads.
    /// Optional: most loops are never snoozed.
    #[test]
    fn the_rhythm_kind_declares_the_snooze_day_as_an_optional_date() {
        let declared = keys_of("rhythm")
            .into_iter()
            .find(|f| f.key == crate::attention::SNOOZED_UNTIL)
            .expect("the rhythm kind names the snooze day");
        assert_eq!(declared.holds, super::super::types::ValueType::Date);
        assert!(
            !declared.required,
            "a loop is whole without one: {declared:?}"
        );
    }

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

    /// **`work` ships the project-management keys, and every one is optional.**
    ///
    /// The spellings are pinned as literals: they are STORED in a fields bag
    /// and nothing outside this process declares them, so a rename that moved
    /// the constant and its assertion together would orphan every record
    /// already written under the old spelling.
    #[test]
    fn the_work_kind_names_the_project_management_keys_and_requires_none() {
        use super::super::types::ValueType;
        let declared = keys_of("work");
        let by_key = |key: &str| {
            declared
                .iter()
                .find(|f| f.key == key)
                .unwrap_or_else(|| panic!("the work kind names '{key}': {declared:?}"))
        };
        for reference in ["owner", "waiting_on"] {
            let field = by_key(reference);
            assert_eq!(field.holds, ValueType::Reference, "{field:?}");
            assert!(!field.list, "{reference} names one thing: {field:?}");
        }
        let depends_on = by_key("depends_on");
        assert_eq!(depends_on.holds, ValueType::Reference, "{depends_on:?}");
        assert!(depends_on.list, "depends_on names several: {depends_on:?}");
        for text in ["commit", "verified_by"] {
            assert_eq!(by_key(text).holds, ValueType::Text, "{text}");
        }
        assert!(
            declared.iter().all(|f| !f.required),
            "a work thing is whole without any of them: {declared:?}",
        );
        assert!(
            declared.iter().all(|f| f.key != "due_on"),
            "a deadline key on work is not shipped: {declared:?}",
        );
    }

    /// **A bot names who it reports to, as a link to another bot, and requires
    /// nothing.** The key is declared so that the chart is walkable by its own
    /// name in both directions: out to a manager, in to everyone who reports
    /// there. The spelling is pinned as a literal because it is stored in a
    /// fields bag and nothing outside this process declares it.
    #[test]
    fn the_bot_kind_names_reports_to_as_a_link_to_a_bot_and_requires_nothing() {
        use super::super::types::ValueType;
        let declared = keys_of("bot");
        let reports_to = declared
            .iter()
            .find(|f| f.key == "reports_to")
            .unwrap_or_else(|| panic!("the bot kind names 'reports_to': {declared:?}"));
        assert_eq!(reports_to.holds, ValueType::Reference, "{reports_to:?}");
        assert_eq!(
            reports_to.points_at,
            Some(crate::memory::EntityKind::BOT),
            "a manager is another bot: {reports_to:?}"
        );
        assert!(!reports_to.list, "a bot reports to one: {reports_to:?}");
        assert!(
            declared.iter().all(|f| !f.required),
            "a bot is whole without a manager: {declared:?}"
        );
    }

    /// **A topic names the operator, as a link to a person, and requires
    /// nothing.** The instance's own record holds it, so the key is declared for
    /// the same reason `reports_to` is: a handle held in a field is a link, and
    /// the declaration is what makes it walkable by its name. The spelling is
    /// pinned as a literal because it is stored in a fields bag and nothing
    /// outside this process declares it.
    #[test]
    fn the_topic_kind_names_the_operator_as_a_link_to_a_person_and_requires_nothing() {
        use super::super::types::ValueType;
        let declared = keys_of("topic");
        let operator = declared
            .iter()
            .find(|f| f.key == "operator")
            .unwrap_or_else(|| panic!("the topic kind names 'operator': {declared:?}"));
        assert_eq!(operator.holds, ValueType::Reference, "{operator:?}");
        assert_eq!(
            operator.points_at,
            Some(crate::memory::EntityKind::PERSON),
            "the operator is a person: {operator:?}"
        );
        assert!(!operator.list, "an instance has one operator: {operator:?}");
        assert!(
            declared.iter().all(|f| !f.required),
            "a topic is whole without an operator: {declared:?}"
        );
    }

    /// **The words a work item's status may hold are the shipped five, the
    /// instance's columns and the governing project's, once each and in that
    /// order.** A project cannot take a word the instance lists out of the set,
    /// and the instance cannot take one the project lists.
    #[test]
    fn the_accepted_words_are_the_five_then_the_instances_then_the_projects() {
        let fields = |columns: &str| -> std::collections::BTreeMap<String, String> {
            [(COLUMNS.to_string(), columns.to_string())]
                .into_iter()
                .collect()
        };
        let instance = fields("considering, someday, next, now, waiting, done");
        let project = fields("inbox, someday, next, now, waiting, done, considering");
        assert_eq!(
            accepted_words(&instance, Some(&project)),
            [
                "someday",
                "next",
                "now",
                "waiting",
                "done",
                "considering",
                "inbox"
            ],
        );
        assert_eq!(
            accepted_words(&std::collections::BTreeMap::new(), None),
            WORK_STATUSES,
            "with no list anywhere the five hold",
        );
        assert_eq!(
            accepted_words(&std::collections::BTreeMap::new(), Some(&project)),
            [
                "someday",
                "next",
                "now",
                "waiting",
                "done",
                "inbox",
                "considering"
            ],
            "a project's own order is kept for the words it adds",
        );
    }

    /// **`meaning_of` reads a shipped word as itself and a mapped word as what it
    /// is mapped to, and says nothing for any other.**
    #[test]
    fn a_word_means_itself_when_shipped_what_it_is_mapped_to_otherwise_and_nothing_else() {
        let instance: std::collections::BTreeMap<String, String> = [
            ("status_word/considering", "someday"),
            ("status_word/vague", "finished"),
            ("status_word/done", "someday"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        assert_eq!(meaning_of("next", &instance), Some("next"));
        assert_eq!(meaning_of("considering", &instance), Some("someday"));
        assert_eq!(meaning_of(" considering ", &instance), Some("someday"));
        assert_eq!(meaning_of("done", &instance), Some("done"));
        assert_eq!(meaning_of("vague", &instance), None);
        assert_eq!(meaning_of("unheard of", &instance), None);
    }

    /// **`work` and `project` hold one status vocabulary, and it is a closed
    /// set of five.** Free text here is what the keys exist to end: a status
    /// nobody can compare is one a cold session cannot ask "what is next" of.
    #[test]
    fn work_and_project_hold_the_same_five_statuses() {
        for kind in ["work", "project"] {
            let status = keys_of(kind)
                .into_iter()
                .find(|f| f.key == "status")
                .unwrap_or_else(|| panic!("the {kind} kind names status"));
            assert_eq!(
                status.one_of.as_deref(),
                Some(&["someday", "next", "now", "waiting", "done"].map(String::from)[..]),
                "{kind}: {status:?}",
            );
            assert!(!status.required, "{kind}: {status:?}");
            assert!(status.accepts("waiting"), "{kind}");
            assert!(
                !status.accepts("drafted, not asked"),
                "{kind}: free text is not a status",
            );
        }
        let project = keys_of("project");
        assert_eq!(
            project.iter().map(|f| f.key.as_str()).collect::<Vec<_>>(),
            ["status", "columns"],
            "a project gets the status and its columns: {project:?}",
        );
        assert!(
            project[1].list,
            "the columns are an ordered list: {project:?}"
        );
    }
}
