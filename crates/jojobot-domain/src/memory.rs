//! Memory — facts, portraits, rules-and-receipts.
//!
//! Provenance is a *type*, not a convention: a fact the user stated is
//! testimony; anything derived is inference, and inference is the default.
//! Making this an enum means every place that consumes a fact must decide how
//! it treats the two — the compiler lists the sites.
//!
//! This module carries the [`Memory`] port and the records it moves — the
//! [`Entity`] (a noun, one of ten [`EntityKind`]s) and the [`Fact`] (an
//! assertion about one). Six verbs, bound by one invariant: a write succeeds
//! only if the read path returns it. There is no privileged owner entity — the
//! user is a person like any other. The port is pure (no rmcp, no reqwest);
//! adapters behind it (the in-memory fake, the real store) live outside this
//! crate, and the write guard ([`guard`]) sits on their write paths.
//!
//! **This is user-agnostic software: no user PII, fixtures included.** Records
//! name roles and synthetic placeholders; every real specific is data, read from
//! the store at runtime.

use std::collections::BTreeMap;

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

pub mod entitlement;
pub mod graph;
pub mod guard;
pub mod kinds;
pub mod search;
pub mod types;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// **A kind: the namespace in a handle, and the schema of what it names**
/// (rule 213).
///
/// It carries its token rather than being a variant, because **the set of
/// kinds is data** — a declaration in the store, seeded at startup and held in
/// [`kinds`]. A closed enum made the nouns one person's life contains a fact
/// about the software, which is the thing nothing else in this repository does.
///
/// The token is `&'static str` so a kind stays `Copy` and free to pass around:
/// [`kinds::intern`] gives a loaded token that lifetime. The set is small and
/// loaded at startup, so what it costs is bounded by the number of kinds that
/// have ever been declared to one process.
///
/// `project` is jojobot's own personal-goal sense (trips, big rocks, builds),
/// deliberately not schema.org's Organization-subtype meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityKind(&'static str);

impl EntityKind {
    /// People in the user's life and public figures alike (artists included).
    pub const PERSON: EntityKind = EntityKind("person");
    /// An activity with a status funnel: big rocks, builds, processes, trips.
    pub const PROJECT: EntityKind = EntityKind("project");
    /// Venues, destinations, trails, informal spots.
    pub const PLACE: EntityKind = EntityKind("place");
    /// A dated occurrence: shows, stays, outings, festivals.
    pub const EVENT: EntityKind = EntityKind("event");
    /// A creative/media artifact with its own identity: sets, albums, posts.
    pub const WORK: EntityKind = EntityKind("work");
    /// A named possession or device with a history: bikes, plants, machines.
    pub const THING: EntityKind = EntityKind("thing");
    /// Clubs, venues-as-institutions, labels, schools, vendors.
    pub const ORG: EntityKind = EntityKind("org");
    /// The glue noun: interest areas, and the anchor for world-facts that
    /// attach to no person, place, or project.
    pub const TOPIC: EntityKind = EntityKind("topic");
    /// An AI identity: a handle, the charter its doc's prose carries, the rules
    /// and memory its facts carry, and the mailbox it owns. A noun like any
    /// other — **nothing about a bot is compiled in**; a bot is data in the
    /// operator's own store, and this kind is only what lets it be one.
    pub const BOT: EntityKind = EntityKind("bot");
    /// A companion animal: a dog, a cat, a horse.
    ///
    /// **Not a `thing`.** `thing` is a named possession, and a pet is not one.
    /// The kind set follows the life it models rather than its own tidiness, so
    /// a part of that life this size gets a noun of its own instead of the
    /// nearest one already here.
    pub const PET: EntityKind = EntityKind("pet");
    /// A cyclical thing: a loop that comes round on a cadence, and the
    /// question it answers is which ones have gone quiet.
    ///
    /// **A kind rather than a key on the thing the loop is about**, because two
    /// loops on one object have to be told apart and as entities they are two
    /// handles. A text label naming them would be identity one layer below
    /// where the near-miss guard can see it.
    ///
    /// **It is the one kind that requires a parent** ([`validate_entity`]): the
    /// parent says whose job the loop is, and a rhythm nobody owns is a
    /// modelling failure rather than a valid shape.
    pub const RHYTHM: EntityKind = EntityKind("rhythm");

    /// **The kinds the software ships**, in the order they are seeded and
    /// listed. Not "every kind there is": that is [`kinds::all`], which answers
    /// from what this process loaded.
    pub const ALL: [EntityKind; 11] = [
        EntityKind::PERSON,
        EntityKind::PROJECT,
        EntityKind::PLACE,
        EntityKind::EVENT,
        EntityKind::WORK,
        EntityKind::THING,
        EntityKind::ORG,
        EntityKind::TOPIC,
        EntityKind::BOT,
        EntityKind::PET,
        EntityKind::RHYTHM,
    ];

    /// A kind from a token this crate already holds for the life of the
    /// process — the constructor [`kinds::resolve`] uses once it has decided a
    /// token really is a kind.
    pub(crate) fn of(token: &'static str) -> Self {
        EntityKind(token)
    }

    /// The wire token — the `kind:` prefix of an id and the frontmatter value.
    pub fn as_token(self) -> &'static str {
        self.0
    }

    /// **A kind from a token this process has loaded.** Strict — unlike the
    /// tolerant `status`/`provenance` cells, an unknown kind has no safe
    /// fallback: guessing one would file a record under a noun the user never
    /// chose.
    ///
    /// It answers from the loaded set rather than from any list here, which is
    /// what makes the set data. A token nobody declared is not a kind, and
    /// [`kinds::resolve`] says which kind of nothing it found.
    pub fn from_token(token: &str) -> Option<Self> {
        kinds::resolve(token).ok()
    }
}

/// **A kind crosses the wire as its token**, exactly as it did when the set
/// was an enum: one lowercase word, in the `kind:` prefix and the frontmatter
/// cell. Written by hand rather than derived because the type carries a
/// borrowed token, and read back through the loaded set, so a stored kind
/// nobody declares any more is refused on the way in rather than resurrected.
impl Serialize for EntityKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}

impl<'de> Deserialize<'de> for EntityKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let token = String::deserialize(deserializer)?;
        kinds::resolve(&token).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for EntityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_token())
    }
}

/// A noun jojobot knows about. Stable; never true/false. Everything points at an
/// entity by its id, so the id is a stable typed string with the grammar
/// `kind:slug` (`person:alpha`) — the **handle**: identity, not position. There is
/// no privileged `self`/owner entity: the user is a person like any other.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityId(pub String);

impl EntityId {
    /// An id from its two parts: `new(Project, "atlas")` → `project:atlas`.
    pub fn new(kind: EntityKind, slug: impl AsRef<str>) -> Self {
        EntityId(format!("{}:{}", kind.as_token(), slug.as_ref().trim()))
    }

    /// A person entity id from a bare handle: `person("alpha")` → `person:alpha`.
    /// If the handle already carries a `kind:` prefix it is used verbatim. The
    /// bare-handle default stays `person` because that is what the wire has
    /// meant since slice 1; every other kind must be spelled out.
    pub fn person(handle: impl AsRef<str>) -> Self {
        let h = handle.as_ref().trim();
        if h.contains(':') {
            EntityId(h.to_string())
        } else {
            EntityId(format!("person:{h}"))
        }
    }

    /// Borrow the underlying id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The id's kind, or `None` if it doesn't parse — a read never panics on a
    /// hand-edited id.
    pub fn kind(&self) -> Option<EntityKind> {
        self.0
            .split_once(':')
            .and_then(|(k, _)| EntityKind::from_token(k))
    }

    /// The token before the colon, whatever it is — **the prefix as written**,
    /// which is what a refusal has to name and what the loaded set is asked
    /// about. Empty when the id carries no colon at all.
    pub fn kind_token(&self) -> &str {
        self.0.split_once(':').map(|(k, _)| k).unwrap_or("")
    }

    /// The id's slug — the part after the kind. Empty for a malformed id.
    pub fn slug(&self) -> &str {
        self.0.split_once(':').map(|(_, s)| s).unwrap_or("")
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A fact's light, local id — unique within its home doc. Facts stay
/// light/local until something must point at one directly (supersede/merge);
/// only then do they earn a global typed id. The store mints it on capture.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FactId(pub String);

impl FactId {
    /// Borrow the underlying id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// **Where this row sits in the order its home was written in.**
    ///
    /// A local id is `f` and a number counted up from the rows already on the
    /// page, so the number is monotonic within one home and says which of two
    /// rows was written later. It is read from the id rather than kept beside
    /// it: an ordinal stored twice is an ordinal that can disagree.
    ///
    /// **It orders the records, and it does not order the writes.** Which value
    /// a thing holds under a key is decided by [`KeyWrite::ordinal`] — an edit
    /// to an old record is a new write on an old row, so ranking records here
    /// would answer with the value that edit replaced.
    ///
    /// `None` for an id in any other shape, which nothing here mints.
    pub fn ordinal(&self) -> Option<u64> {
        self.0.trim().strip_prefix('f')?.parse().ok()
    }
}

impl std::fmt::Display for FactId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether an entity's doc is read on every boot or fetched when the
/// conversation reaches for it. Tolerant on read — an unknown value means
/// on-demand, which is the cheap, safe side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Boot {
    /// Read every boot.
    Always,
    /// Fetched when something reaches for it.
    #[default]
    OnDemand,
}

impl Boot {
    /// The wire token written to the frontmatter's `boot` field.
    pub fn as_token(self) -> &'static str {
        match self {
            Boot::Always => "always",
            Boot::OnDemand => "on-demand",
        }
    }

    /// Parse a `boot` field; anything but the exact `always` token is on-demand.
    pub fn from_token(value: &str) -> Self {
        match value.trim() {
            "always" => Boot::Always,
            _ => Boot::OnDemand,
        }
    }
}

/// An entity as its doc's frontmatter carries it. **Lean and uniform across all
/// ten kinds** — no per-kind fields: what varies between a person and a place
/// is the *facts* about them, not the record's shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    /// The handle — identity, never position. Immutable in this milestone.
    pub id: EntityId,
    /// The kind, always the one its handle carries.
    pub kind: EntityKind,
    /// Display name. Free text for humans; renamed freely, nothing moves.
    pub name: String,
    /// The other names this entity answers to — the nickname, the short form,
    /// the initials. SKOS's split: `name` is the preferred label, these are the
    /// alternate ones, and **nothing distinguishes them but preference** — the
    /// guard and search treat every label alike (see [`Entity::labels`]).
    /// Absent in docs written before the field existed, which read as none.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Where this entity came from — **never invented**. An entity exists
    /// because the user named it or a real source produced it.
    pub source: String,
    /// The cross-link to this entity in the task layer, if there is one, in
    /// whatever form that layer addresses things.
    pub crm: Option<String>,
    /// The entity this one sits **under**, if it sits under anything. A root
    /// has none, and most entities are roots.
    ///
    /// **An entity can have children, and a fact lives on the most specific
    /// entity it is about.** A flat entity silts the way a 10,000-line source
    /// file silts: everything about it lands on one page. The tree is the fix —
    /// detail moves down onto a child, and reading becomes zooming instead of
    /// loading.
    ///
    /// Single-parent, and **the pointer lives on the child**. Upward is the
    /// direction that is one value; downward is a set, and a set stored on the
    /// parent would be a second place the same truth lives. Children are
    /// therefore *derived* ([`Memory::children`]), never stored.
    ///
    /// Absent in docs written before the field existed, which read as a root.
    #[serde(default)]
    pub parent: Option<EntityId>,
    /// Boot tier.
    pub boot: Boot,
}

impl Entity {
    /// Every name this entity answers to: its display name first, then its
    /// aliases, blanks dropped.
    ///
    /// **The one definition of "what is this thing called."** The write guard
    /// screens against it and the search index ingests it, so a nickname cannot
    /// be findable but unrecognized, or recognized but unfindable.
    pub fn labels(&self) -> Vec<&str> {
        std::iter::once(self.name.as_str())
            .chain(self.aliases.iter().map(String::as_str))
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect()
    }
}

/// An entity about to be created. `override_token` is the token the write
/// guard's own refusal minted, handed back — the caller's evidence that it read
/// what it is overriding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEntity {
    /// The handle to create.
    pub id: EntityId,
    /// Display name.
    pub name: String,
    /// The other names it answers to; empty is the ordinary case.
    pub aliases: Vec<String>,
    /// Where it came from — required, and never invented by jojobot.
    pub source: String,
    /// Optional cross-link to this entity in the task layer.
    pub crm: Option<String>,
    /// The entity to create this one **under**, if it is not a root. Screened
    /// by [`guard::decide_parent`]: the parent must already exist, and nothing
    /// may be its own parent.
    ///
    /// **Set here and nowhere else.** There is deliberately no `parent` on
    /// [`EntityPatch`]: reparenting would have to move the child's page in the
    /// store as well as rewrite its frontmatter, and that is a decision this
    /// milestone does not make. Because parentage is fixed at creation, a cycle
    /// deeper than self-parenting is unreachable — a fresh entity has no
    /// children to be caught below.
    pub parent: Option<EntityId>,
    /// Boot tier.
    pub boot: Boot,
    /// The token a previous call's refusal minted, handed back after the caller
    /// read the candidates and judged them a different entity. It lifts only
    /// the refusal that minted it, and never an exact handle collision.
    pub override_token: Option<String>,
}

impl NewEntity {
    /// An entity with the two fields that are always required, defaults
    /// elsewhere — the common shape.
    pub fn new(id: EntityId, name: impl Into<String>, source: impl Into<String>) -> Self {
        NewEntity {
            id,
            name: name.into(),
            aliases: Vec::new(),
            source: source.into(),
            crm: None,
            parent: None,
            boot: Boot::default(),
            override_token: None,
        }
    }

    /// Every name this write claims — the same set [`Entity::labels`] reads off
    /// a stored one, so the guard screens the incoming record exactly as it
    /// screens the ones already there.
    pub fn labels(&self) -> Vec<&str> {
        std::iter::once(self.name.as_str())
            .chain(self.aliases.iter().map(String::as_str))
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect()
    }
}

/// A metadata edit to an existing entity. **The handle is not in here**: a
/// rename is a pointer-rewrite, a separate gated operation, not a field edit.
/// A `None` field is left alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntityPatch {
    /// New display name. Screened by the write guard exactly as a creation is —
    /// otherwise the guard is trivially side-steppable: create under a
    /// throwaway name, then rename onto the collision.
    pub name: Option<String>,
    /// The whole alias set, replaced. `None` leaves it alone; `Some(vec![])`
    /// clears it — "it has none" is a thing a caller must be able to say, and a
    /// field that only ever grows is one nobody can correct.
    pub aliases: Option<Vec<String>>,
    /// New source.
    pub source: Option<String>,
    /// New cross-link to this entity in the task layer.
    pub crm: Option<String>,
    /// The token the relabel refusal minted, handed back after the caller read
    /// the candidates and judged them different. Same mechanism, and the same
    /// type, as [`NewEntity::override_token`].
    pub override_token: Option<String>,
}

/// An in-place edit to one addressed fact. A `None` field is left alone; this is
/// fix-the-source, so the row is rewritten rather than appended beside.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FactPatch {
    /// New crisp claim.
    pub content: Option<String>,
    /// New nuance / why / merge notes.
    pub details: Option<String>,
    /// New lifecycle state. A refutation is **not** one of these: rewrite
    /// `content` to state the negative truth instead (see [`FactStatus`]).
    pub status: Option<FactStatus>,
    /// New provenance. Promoting inference → testimony additionally requires
    /// [`FactPatch::confirmed_by_user`].
    pub provenance: Option<Provenance>,
    /// New standing. Settling an open claim additionally requires
    /// [`FactPatch::confirmed_by_user`], and a claim nobody backs cannot be
    /// settled at all — see [`check_standing`]. Reopening is free.
    pub standing: Option<Standing>,
    /// A typed edge to attach. `None` leaves any existing edge alone; this
    /// milestone writes one edge per fact, so setting it replaces.
    pub edge: Option<Edge>,
    /// **Fields to set.** Each key named is written; a key the record already
    /// carries and this does not name is left alone, so an edit reaches one
    /// field without restating the rest.
    pub fields: BTreeMap<String, String>,
    /// **Fields to remove**, by key. Its own list rather than an empty value in
    /// [`FactPatch::fields`]: an empty value is a value somebody wrote, and
    /// setting a key to nothing and taking the key off the record are two
    /// different edits.
    ///
    /// A key that is not on the record is not an error. The patch says what
    /// the record must not carry afterwards, and it does not.
    pub clear_fields: Vec<String>,
    /// **A new day to look again on** — see [`Fact::stale_after`]. It is the
    /// caller's judgement about how long a reading stays trustworthy, so a
    /// caller may move it; the stamp that says when jojobot took the record in
    /// is the store's and is not here.
    pub stale_after: Option<Date>,
    /// **Take the day off**, leaving a claim that makes no promise about how
    /// long it stays good. Its own flag rather than an empty value in
    /// [`FactPatch::stale_after`], for the reason `clear_fields` is its own
    /// list: leaving it alone and taking it off are two different edits.
    pub clear_stale_after: bool,
    /// The user's explicit confirmation, required to promote a claim to
    /// testimony AND to settle one that is open. jojobot infers freely; it
    /// never blesses on its own, on either axis.
    pub confirmed_by_user: bool,
}

/// The promotion gate: a claim may only become testimony on the user's explicit
/// confirmation. Everything else — demotion, a no-op restatement, a status flip
/// — is free. This is one face of the cross-kind invariant (the other being
/// draft → confirmed for rules), so it lives in the domain where both adapters
/// call it and neither can forget it.
pub fn check_promotion(
    current: Provenance,
    requested: Provenance,
    confirmed_by_user: bool,
) -> Result<(), MemoryError> {
    let promoting = current == Provenance::Inference && requested == Provenance::Testimony;
    if promoting && !confirmed_by_user {
        return Err(MemoryError::UnconfirmedPromotion);
    }
    Ok(())
}

/// Where a claim came from. The default is [`Provenance::Inference`]: anything
/// not tied to the user's own words is a hypothesis until confirmed. Stored in
/// its **own** table column — never folded into the content — so a claim that
/// happens to end in a marker glyph can't be misread as inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provenance {
    /// The user said or confirmed it.
    Testimony,
    /// jojobot (or Claude) derived it. Carries no more authority than a guess.
    #[default]
    Inference,
}

impl Provenance {
    /// The wire token this provenance is written and read back as.
    pub fn as_token(self) -> &'static str {
        match self {
            Provenance::Testimony => "testimony",
            Provenance::Inference => "inference",
        }
    }

    /// Parse a `provenance` cell. Only the exact `testimony` token yields
    /// testimony; everything else — the default, a blank cell, an unknown
    /// value — is inference. This is deliberately lenient and one-directional:
    /// a garbled cell degrades to the *less*-trusted class, never up to
    /// testimony, and no fact is ever dropped.
    pub fn from_token(cell: &str) -> Self {
        match cell.trim() {
            "testimony" => Provenance::Testimony,
            _ => Provenance::Inference,
        }
    }
}

/// **How sure anyone is of a claim.** [`Provenance`] answers who backs it;
/// this answers how sure they are. The two are independent, which is why they
/// are two fields: the operator stating something first-hand and hedging it has
/// no honest single-field answer.
///
/// **Two values, and it stays two.** Open or settled — not a confidence level,
/// a score, or a degree. Whether a claim is still open to being wrong is a fact
/// about it rather than a measurement of it.
///
/// All four pairings are legal. Settling is gated on the operator's
/// confirmation ([`check_standing`]) and nothing else, so a derived claim
/// somebody has since confirmed says exactly that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Standing {
    /// Nobody is treating this as open to question.
    Settled,
    /// Still open to being wrong — a hedge, or an inference nobody confirmed.
    Open,
}

impl Standing {
    /// The wire token this standing is written and read back as.
    pub fn as_token(self) -> &'static str {
        match self {
            Standing::Settled => "settled",
            Standing::Open => "open",
        }
    }

    /// Read a standing back **in the light of the claim's provenance**: an
    /// absent cell means what that claim meant before the column existed, so
    /// a record from an earlier era keeps saying what it always said without
    /// being rewritten. A garbled value degrades to `open`, never toward a
    /// claim being more settled than the record can vouch for.
    pub fn parse(cell: &str, provenance: Provenance) -> Self {
        match cell.trim() {
            "" => Standing::default_for(provenance),
            "settled" => Standing::Settled,
            _ => Standing::Open,
        }
    }

    /// What a claim's standing is when nobody said. Derived from provenance
    /// rather than flat: the operator's word is settled unless it is hedged, a
    /// session's guess is open until the operator confirms it. A flat `Settled`
    /// would declare every old guess a fact, and a flat `Open` would reopen
    /// every claim the operator ever stated flatly.
    pub fn default_for(provenance: Provenance) -> Self {
        match provenance {
            Provenance::Testimony => Standing::Settled,
            Provenance::Inference => Standing::Open,
        }
    }
}

/// The standing gate, and the twin of [`check_promotion`].
///
/// **Settling a claim is the user's move**: closing an open claim needs the
/// same explicit confirmation promoting one does, because withdrawing a hedge
/// is a statement only the operator can make.
///
/// It says nothing about provenance. The two fields answer different questions
/// — who backs a claim, and how sure anyone is — so a derived claim somebody
/// has since confirmed is an ordinary row rather than a refused one.
///
/// Reopening is free, exactly as demotion to inference is free. Nothing is
/// risked by a claim admitting it might be wrong.
pub fn check_standing(
    current: Standing,
    requested: Standing,
    confirmed_by_user: bool,
) -> Result<(), MemoryError> {
    let settling = current == Standing::Open && requested == Standing::Settled;
    if settling && !confirmed_by_user {
        return Err(MemoryError::UnconfirmedSettling);
    }
    Ok(())
}

/// **The standing a fresh capture lands on.** One function so the fake and the
/// real store cannot come to disagree about what a silent `standing` means.
///
/// A capture declares its standing on honour, exactly as it declares its
/// provenance on honour: the gate is on promotion, never on assertion.
pub fn standing_of(new: &NewFact) -> Standing {
    new.standing
        .unwrap_or(Standing::default_for(new.provenance))
}

/// The shape of a fact's structured edge — a **closed** set of five, and the
/// only shapes this milestone writes. The general edges vocabulary
/// (`receiptOf`, `supersedes`, `derivedFrom`, …) still arrives with the graph
/// milestone: [`EdgeShape::Connection`] is not that vocabulary arriving early,
/// because it names no relation at all. It is the shape for a link whose
/// nature was never recorded.
///
/// These exist because ask-across — "which friends are in Shelbyville?", "what's
/// connected to Duff Fest?" — must never rest on an AI scanning prose. A fact that
/// puts an entity somewhere, in something, at something, or about something
/// produces a typed edge **at capture**, so a cross-entity question is an edge
/// walk instead of fifty sequential reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeShape {
    /// The subject is somewhere. Object is a [`EntityKind::PLACE`].
    Location,
    /// The subject belongs to something. Object is an [`EntityKind::ORG`].
    Membership,
    /// The subject was at something. Object is an [`EntityKind::EVENT`].
    Attendance,
    /// The subject is about something — the open shape: any kind of object.
    About,
    /// **The subject is connected to something, and nobody recorded how.**
    ///
    /// Not a weaker `about`, and the distinction is the reason it exists.
    /// `about` asserts that the record is ABOUT that entity — a claim somebody
    /// made, and one a reader is entitled to act on. This one admits only that
    /// a link is there; its nature is deferred, not implied. Filing an unknown
    /// link as `about` would launder it into an assertion nobody made, and
    /// everything downstream would read it as one.
    ///
    /// Deferred is not absent: the pointer is real and walks like any other
    /// edge, because an edge that cannot be followed is not an edge.
    Connection,
}

impl EdgeShape {
    /// Every shape, in declaration order.
    pub const ALL: [EdgeShape; 5] = [
        EdgeShape::Location,
        EdgeShape::Membership,
        EdgeShape::Attendance,
        EdgeShape::About,
        EdgeShape::Connection,
    ];

    /// The **input and storage** token — what a caller passes and what the
    /// table's `edges` cell holds.
    pub fn as_token(self) -> &'static str {
        match self {
            EdgeShape::Location => "location",
            EdgeShape::Membership => "membership",
            EdgeShape::Attendance => "attendance",
            EdgeShape::About => "about",
            EdgeShape::Connection => "connection",
        }
    }

    /// The **response** name — schema.org's word for this edge. Names only: the
    /// recognition benefit is the vocabulary, not the machinery.
    pub fn as_name(self) -> &'static str {
        match self {
            EdgeShape::Location => "location",
            EdgeShape::Membership => "memberOf",
            EdgeShape::Attendance => "attendee",
            EdgeShape::About => "about",
            // **Not `about`, and not a synonym for it.** schema.org's own word
            // for a link whose nature is unstated: it relates these two and
            // says nothing more, which is exactly the claim being made.
            EdgeShape::Connection => "relatedTo",
        }
    }

    /// Parse a shape token. Strict, like [`EntityKind::from_token`]: an unknown
    /// shape has no safe fallback — guessing one would file an edge the user
    /// never drew.
    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_token() == token.trim())
    }

    /// The kind this shape's object must be, or `None` where any kind will do.
    /// A `location` pointing at a person is a mis-drawn edge, not a nuance.
    pub fn object_kind(self) -> Option<EntityKind> {
        match self {
            EdgeShape::Location => Some(EntityKind::PLACE),
            EdgeShape::Membership => Some(EntityKind::ORG),
            EdgeShape::Attendance => Some(EntityKind::EVENT),
            EdgeShape::About => None,
            // Any kind, for the same reason `about` takes any kind — and a
            // stronger one: refusing a kind here would be jojobot deciding what
            // the link means, which is the one thing it does not know.
            EdgeShape::Connection => None,
        }
    }
}

impl std::fmt::Display for EdgeShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_token())
    }
}

/// A fact's typed edge: one shape, one object entity. One edge per fact in this
/// milestone — written atomically with the fact it belongs to, and covered by the
/// same read-back invariant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    /// Which of the four shapes.
    pub shape: EdgeShape,
    /// The entity the edge points at. Validated, and required to **already
    /// exist** exactly as a subject is: a typo'd object gets candidates back and
    /// an unrecognized one gets refused, so an edge never points at a node
    /// nobody else references.
    pub object: EntityId,
}

impl Edge {
    /// An edge from its two parts.
    pub fn new(shape: EdgeShape, object: EntityId) -> Self {
        Edge { shape, object }
    }
}

/// Validate an edge before it is written: the object's id grammar, then the
/// shape's kind rule. Both adapters call this, so neither can store an edge the
/// other would refuse.
pub fn validate_edge(edge: &Edge) -> Result<(), MemoryError> {
    validate_subject(&edge.object)?;
    match edge.shape.object_kind() {
        Some(required) if edge.object.kind() != Some(required) => {
            Err(MemoryError::InvalidEdge(format!(
                "a '{}' edge points at a {required}, got '{}'",
                edge.shape, edge.object
            )))
        }
        _ => Ok(()),
    }
}

/// A fact's lifecycle state. Lifecycle is a **status flip**, never a deletion:
/// an id is never destroyed while anything might reference or re-derive it.
///
/// **There is no `negated`.** A refutation is an ordinary
/// [`update_fact`](Memory::update_fact) that rewrites the content to state the
/// negative truth — "does NOT play the theremin" is a fact like any other, and
/// it reads back as the current truth rather than as a flag beside a claim the
/// reader then has to adjudicate. That is fix-the-source; a card is what is so
/// today, and the journal keeps the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FactStatus {
    /// The current truth.
    #[default]
    Active,
    /// Replaced by a later fact; kept so references survive.
    Superseded,
    /// **Taken back — one way, and only ever an event.**
    ///
    /// Not a third flavour of superseded. Superseded says a later claim
    /// replaced this one and the row is kept so references survive; this says
    /// the thing recorded here should not have been, and there is no
    /// replacement. It arrives only through [`Memory::retract`], never through
    /// an ordinary edit, and nothing takes a row back out of it — see
    /// [`check_retractable`] for why one-way has to be enforced rather than
    /// merely intended.
    ///
    /// A retracted row is still a row: the record stays, marked, exactly as
    /// the no-delete rule requires. What it loses is its standing as
    /// something a reader should act on, which is what keeping it out of a
    /// default search buys.
    Retracted,
}

impl FactStatus {
    /// The wire token this status is written and read back as.
    pub fn as_token(self) -> &'static str {
        match self {
            FactStatus::Active => "active",
            FactStatus::Superseded => "superseded",
            FactStatus::Retracted => "retracted",
        }
    }

    /// Parse a `status` cell. Lenient in one direction only: a blank or garbled
    /// cell reads as active rather than dropping the fact — but a fact is never
    /// *promoted* out of superseded by a bad cell, because that token is matched
    /// exactly.
    ///
    /// The retired **`negated`** token maps to superseded. Rows carrying it are
    /// on disk, and removing a variant must not hard-fail a read any more than
    /// adding one may: the behaviour that mattered — excluded from a default
    /// search — is the same, and the row is rewritten in the current spelling on
    /// its next touch (lazy migration, no sweep).
    pub fn from_token(cell: &str) -> Self {
        match cell.trim() {
            "superseded" | "negated" => FactStatus::Superseded,
            "retracted" => FactStatus::Retracted,
            _ => FactStatus::Active,
        }
    }
}

/// The global address of a fact: its home doc's entity handle plus the row's
/// local id, written `person:alpha#f3`. Local ids are only unique within a doc, so
/// every cross-doc reference is doc-qualified and can never collide. `recall`
/// returns one with every fact; `update_fact` targets one — that pairing is what
/// makes day-to-day editing possible at all.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FactAddress {
    /// The entity whose doc holds the row.
    pub home: EntityId,
    /// The row's light, doc-local id.
    pub local: FactId,
}

impl FactAddress {
    /// An address from its two parts.
    pub fn new(home: EntityId, local: FactId) -> Self {
        FactAddress { home, local }
    }

    /// Parse the wire form `kind:slug#local-id`. Both halves are validated —
    /// an address arrives from a client and is used to select a row to rewrite.
    pub fn parse(raw: &str) -> Result<Self, MemoryError> {
        let bad = || MemoryError::InvalidAddress(raw.to_string());
        let (home, local) = raw.trim().split_once('#').ok_or_else(bad)?;
        let home = EntityId(home.to_string());
        validate_subject(&home).map_err(|_| bad())?;
        let local = local.trim();
        if local.is_empty() || !local.bytes().all(is_slug_byte) {
            return Err(bad());
        }
        Ok(FactAddress::new(home, FactId(local.to_string())))
    }
}

impl std::fmt::Display for FactAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}#{}", self.home, self.local)
    }
}

/// The slug charset: `[a-z0-9-]`. Deliberately narrow — no newline (forge a row
/// or a `###` header), no `|` (forge a cell), no backtick (forge a fence), no
/// space, no uppercase (so a handle has exactly one spelling).
fn is_slug_byte(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'
}

impl Fact {
    /// **Has this claim gone past the day somebody said to look again?**
    ///
    /// `false` when nobody set one. **Absence means no promise was made** —
    /// not that the claim is fresh, and not that it is stale. Reading it as
    /// stale would make every record in the store suspect.
    pub fn is_stale(&self, as_of: Date) -> bool {
        self.stale_after.is_some_and(|day| day < as_of)
    }
}

/// Validate an entity id before it is written anywhere. Ids are **structured**
/// (`kind:slug`, slug `[a-z0-9-]+`), never free text — so an adversarial
/// subject can neither forge markdown nor invent a kind. This is the primary
/// defence; escaping-on-write is the belt-and-suspenders.
///
/// **The refusal says which thing is wrong**, and the kind half of that has two
/// answers rather than one. A process that loaded no kinds refuses `bot:gamma`
/// exactly as it refuses a typo, and a caller told "unknown kind" about the
/// most ordinary handle in the system has no way forward from it (rule 68).
pub fn validate_subject(subject: &EntityId) -> Result<(), MemoryError> {
    let s = subject.as_str();
    let shaped =
        s.len() <= 128 && !subject.slug().is_empty() && subject.slug().bytes().all(is_slug_byte);
    if !shaped {
        return Err(MemoryError::InvalidSubject(format!(
            "'{s}' is no handle: a handle is kind:slug, slug [a-z0-9-]+, at most 128 characters"
        )));
    }
    // The kind is asked of the loaded set, which is what makes the answer say
    // what is really wrong: nothing seeded this process, or nobody declared
    // this kind.
    match kinds::resolve(subject.kind_token()) {
        Ok(_) => Ok(()),
        // **The two failures have two repairs, so they are two errors.** A
        // caller can spell a kind differently; nobody can spell their way out
        // of a process that loaded nothing.
        Err(kinds::NotAKind::SetNeverLoaded) => Err(MemoryError::KindsNeverLoaded {
            attempted: Some(s.to_string()),
        }),
        Err(why) => Err(MemoryError::InvalidSubject(format!("'{s}': {why}"))),
    }
}

/// Validate a value destined for a **frontmatter line** (`name`, `source`).
/// A frontmatter field is one line inside a fenced block, so a newline would
/// forge a field and a backtick could close the fence. Rejected outright rather
/// than escaped: unlike a fact's content, these are short labels, and silently
/// mangling a name is worse than refusing it.
pub fn validate_field(label: &str, value: &str) -> Result<(), MemoryError> {
    let v = value.trim();
    if v.is_empty() {
        return Err(MemoryError::InvalidEntity(format!("{label} is empty")));
    }
    if v.chars().count() > 200 {
        return Err(MemoryError::InvalidEntity(format!("{label} is too long")));
    }
    if v.chars().any(|c| c == '`' || c.is_control()) {
        return Err(MemoryError::InvalidEntity(format!(
            "{label} must be one plain line (no newline, no backtick)"
        )));
    }
    Ok(())
}

/// Validate an alias set. Each alias is a frontmatter label like `name` — one
/// plain line, non-empty, no backtick — with one rule of its own: **no comma**.
/// The block carries the whole set on a single comma-separated line, so an alias
/// containing a comma would come back as two aliases, neither of them what
/// anyone wrote.
pub fn validate_aliases(aliases: &[String]) -> Result<(), MemoryError> {
    for alias in aliases {
        validate_field("alias", alias)?;
        if alias.contains(',') {
            return Err(MemoryError::InvalidEntity(format!(
                "alias '{alias}' contains a comma, which separates aliases"
            )));
        }
    }
    Ok(())
}

/// Validate the optional `crm` link — a cross-link to this entity in the task
/// layer, in whatever form that layer addresses things.
///
/// **The grammar belongs to the task layer, not to jojobot.** One system
/// addresses a task `card:874` and another `ENG-421`, and a validator that
/// accepted one shape refused every other layer the link outright, leaving the
/// entity no way to record it. So the screening is only what the frontmatter
/// line needs: one plain token, non-empty, no backtick — the same
/// [`validate_field`] rules every other label takes — with **no whitespace and
/// no comma**, so the reference cannot split into two on the way back out.
pub fn validate_crm(crm: &str) -> Result<(), MemoryError> {
    validate_field("crm", crm)?;
    if crm.trim().chars().any(|c| c.is_whitespace() || c == ',') {
        return Err(MemoryError::InvalidEntity(format!(
            "crm must be one reference with no space and no comma, got '{crm}'"
        )));
    }
    Ok(())
}

/// The line that opens a document's machine-readable fact table.
///
/// **jojobot's document schema is jojobot's, not a store's.** A store decides
/// how a document is fetched and saved; the shape *inside* it — a metadata
/// block, a prose half, a fact table under this header — is this domain's, and
/// it has been all along: a fact's content is one line here because a table
/// cell is, and a field carries no backtick because it sits inside a fenced
/// block. Naming the header here makes that ownership explicit, and it is what
/// lets [`validate_prose`] bind every adapter equally instead of one of them
/// keeping a private copy the others could not enforce.
pub const FACTS_HEADER: &str = "### ⚙ facts";

/// The frontmatter field that marks a document as **jojobot's own machinery**
/// rather than anybody's content: a bot's sessions page, and later its mailbox
/// page. The value names which kind of machinery it is.
///
/// Named in the domain for the same reason [`FACTS_HEADER`] is. This one string
/// decides **what search can see**. jojobot's bookkeeping
/// lives in the same collection as the entities — a sessions page is a child of
/// its bot's page, which is the whole point of the tree — and the boot scan
/// reads every document it finds, generously, because a page somebody wrote by
/// hand is exactly the page worth finding. A machinery page is the opposite: it
/// is jojobot talking to itself, and a search that surfaced it would answer a
/// question about the operator's life with a session's focus line.
///
/// A store keeping a private copy of this is a store that can start indexing
/// its own bookkeeping without anything noticing.
pub const MACHINERY_FIELD: &str = "machinery";

/// The lines a document reserves for its own structure. Prose may not carry
/// one, whatever store it is bound for — see [`validate_prose`].
///
/// A list rather than the single constant: this is the seam. When the schema
/// grows a second structural line, it is added here once and every adapter,
/// the fake included, refuses it from that moment.
pub const RESERVED_PROSE_LINES: &[&str] = &[FACTS_HEADER];

/// Validate an entity's prose — the human half of its doc, where a bot's
/// charter lives.
///
/// Permissive by design: paragraphs are the point, so only two things are
/// refused. **Emptiness**, because a page with nothing on it is not a charter.
/// And **a line the document schema reserves** ([`RESERVED_PROSE_LINES`]),
/// because the reader finds the fact table by the first such line: prose
/// carrying one moves the boundary, and every fact below it stops being read as
/// a fact. Refused rather than escaped — silently mangling somebody's charter
/// is worse than declining to write it.
///
/// The second rule lives here, not in the one adapter whose documents it would
/// corrupt, because a fake that waves it through is how a green suite ships a
/// store-corrupting write. Every adapter validates through this, so none can be
/// the lenient one.
pub fn validate_prose(prose: &str) -> Result<(), MemoryError> {
    if prose.trim().is_empty() {
        return Err(MemoryError::InvalidEntity(
            "prose is empty; there is nothing to write".into(),
        ));
    }
    if let Some(reserved) = prose
        .lines()
        .map(str::trim)
        .find(|line| RESERVED_PROSE_LINES.contains(line))
    {
        return Err(MemoryError::InvalidEntity(format!(
            "prose carries the line '{reserved}', which jojobot reserves for a record's own \
             structure; every fact below such a line would stop being read as a fact. Say it \
             some other way — the words on their own, not on a line of their own, are fine"
        )));
    }
    Ok(())
}

/// Normalize prose to the form that survives a round-trip: edge whitespace is
/// not significant and no store preserves it, and CRLF folds to `\n` because a
/// store that rebuilds text line by line strips the `\r`s. Both adapters call
/// this, which is what makes the returned prose identical to a later read's.
pub fn normalize_prose(prose: &str) -> String {
    let mut out = prose.to_string();
    while out.contains("\r\n") {
        out = out.replace("\r\n", "\n");
    }
    out.trim().to_string()
}

/// Validate everything an entity write carries: the handle's grammar, its
/// required labels, and the `crm` and `parent` pointers if present.
/// Both adapters call this, so neither can accept a record the other would
/// refuse.
///
/// A `parent` is an entity handle, so it is held to the handle grammar — the
/// same defence [`validate_subject`] gives every other id that reaches the
/// store. Whether that handle *resolves*, and whether it is this entity's own,
/// are the guard's questions ([`guard::decide_parent`]), not this one's: those
/// come back blocked-with-candidates, and this comes back malformed.
pub fn validate_entity(
    id: &EntityId,
    name: &str,
    aliases: &[String],
    source: &str,
    crm: Option<&str>,
    parent: Option<&EntityId>,
) -> Result<(), MemoryError> {
    validate_subject(id)?;
    validate_field("name", name)?;
    validate_aliases(aliases)?;
    validate_field("source", source)?;
    if let Some(crm) = crm {
        validate_crm(crm)?;
    }
    if let Some(parent) = parent {
        validate_subject(parent)?;
    }
    // **A rhythm names whose job it is, or it is not written.** The parent is
    // the answer: a maintenance loop sits under the thing maintained, a review
    // loop under the bot that has to carry it through. A rhythm under nothing
    // would surface at a boot with no way to say what it is a loop ON, so the
    // absent parent is a modelling failure rather than a shape to store and
    // repair later.
    if id.kind() == Some(EntityKind::RHYTHM) && parent.is_none() {
        return Err(MemoryError::InvalidEntity(format!(
            "'{id}' is a rhythm and names no parent. A rhythm is a loop ON something, and the \
             parent says whose job it is — the thing maintained, or the bot that carries the \
             review. Send this again with parent set to the handle it is a loop on"
        )));
    }
    Ok(())
}

/// A line break, either byte. A bare `\r` is refused as hard as `\n`: while the
/// store preserves the byte nothing breaks, but a store that normalizes line
/// endings (`\r` → `\n`, which markdown pipelines routinely do) splits the row —
/// and the split ends the table's contiguous run of `|` lines, so **every fact
/// below it is unread too**, not just the one carrying the CR.
fn breaks_the_row(value: &str) -> bool {
    value.contains('\n') || value.contains('\r')
}

/// A fact's content must be one non-empty line — a table cell is one line, and
/// an empty claim is not a claim.
/// The key that marks a record as a retraction, holding the ADDRESS it takes
/// back — not a handle, so it is deliberately not a walkable link: the target
/// is a row, and rows are reached by address.
pub const RETRACTS: &str = "retracts";

/// **How long a field key may be**, in characters.
///
/// **The domain says the number and the store holds what the domain admits.**
/// The column carrying a key is 191 characters wide and sits inside a primary
/// key, where widening is bounded by the index rather than by preference — so
/// the answer here is a limit under the column, not a wider column.
///
/// **128 is the number the other half of that key already gets.** A write is
/// addressed by the thing and the key together, and an entity id is bounded at
/// 128 by [`validate_subject`]. One rule for both halves beats two numbers
/// nobody can derive from each other, and it leaves 63 characters of headroom
/// under the column, so a key that passes here cannot reach the wall even if a
/// store lengthens it on the way in.
///
/// It refuses no real key: a key is a name a caller invents for one property,
/// and the longest this software ships is under twenty characters.
pub const MAX_KEY_CHARS: usize = 128;

/// **Whether a field key is one jojobot writes itself.**
///
/// The bag is flat and free with exactly one exception, and it is the key that
/// says a record takes another one back. jojobot writes that key and a caller
/// may not: a record naming what it retracts is the only record on this rail
/// that changes the standing of a different row, so a caller able to write the
/// key could mark somebody else's record taken back.
///
/// Refused rather than renamed. Renaming it would hand back a record the caller
/// did not write, and what a type is gets derived from what accumulates here,
/// so a silently moved key is a corrupted sample.
pub fn reserved_key(key: &str) -> bool {
    key.trim() == RETRACTS
}

/// **A record's fields may not use the key jojobot writes itself, and a key
/// has a length.**
///
/// Checked on the write path in the domain, so both adapters answer for it — a
/// rule enforced in one store and not the other is a rule that holds until
/// somebody switches stores.
pub fn validate_fields(fields: &BTreeMap<String, String>) -> Result<(), MemoryError> {
    if let Some(key) = fields.keys().find(|k| reserved_key(k)) {
        return Err(MemoryError::InvalidFact(format!(
            "a record's fields cannot use '{key}' as a key: jojobot writes that key itself, on \
             the record it writes when something is taken back, and it names the record that was \
             taken back. Rename the key — anything else is yours to choose."
        )));
    }
    if let Some(key) = fields.keys().find(|k| k.chars().count() > MAX_KEY_CHARS) {
        return Err(MemoryError::InvalidFact(format!(
            "a field key may be {MAX_KEY_CHARS} characters and '{}…' is {}. A key names one \
             property of a thing, so shorten the name — what it HOLDS has no such limit, and a \
             key that is carrying a sentence probably wants to be a value.",
            key.chars().take(24).collect::<String>(),
            key.chars().count(),
        )));
    }
    Ok(())
}

/// **Nor may an edit take that key back off.**
///
/// The set path is screened so the marker cannot be forged; the clear path is
/// the same key on the same rail, and unscreened it defeats the same rule from
/// the other side. Removing it makes [`Fact::is_retraction`] answer false, so
/// the account becomes retractable — the reversal one-way exists to forbid —
/// and the only machine-readable link from the marked row to the record saying
/// why is gone.
///
/// **It refuses the one move, never the record.** A retraction account is an
/// ordinary record in every other respect, and every key on it but this one is
/// still the caller's to set and clear: a screen that locked the record would
/// break the floor rule's promise that a record can always be repaired.
pub fn validate_cleared_fields(cleared: &[String]) -> Result<(), MemoryError> {
    if let Some(key) = cleared.iter().find(|k| reserved_key(k)) {
        return Err(MemoryError::InvalidFact(format!(
            "a record's fields cannot clear '{key}': jojobot writes that key itself, on the \
             record it writes when something is taken back, and taking it off would leave a \
             retraction that no longer reads as one. If the retraction was a mistake, capture \
             what is so now as a new record — every other key on it is yours to clear."
        )));
    }
    Ok(())
}

pub fn validate_content(content: &str) -> Result<(), MemoryError> {
    if content.trim().is_empty() {
        return Err(MemoryError::InvalidFact("content is empty".into()));
    }
    if breaks_the_row(content) {
        return Err(MemoryError::InvalidFact(
            "content spans multiple lines; a claim is one line".into(),
        ));
    }
    Ok(())
}

/// Details ride in the same table row, so they are one line too — but may be
/// absent.
pub fn validate_details(details: Option<&str>) -> Result<(), MemoryError> {
    if details.is_some_and(breaks_the_row) {
        return Err(MemoryError::InvalidFact(
            "details span multiple lines; details are one line".into(),
        ));
    }
    Ok(())
}

/// Apply an in-place edit to a fact — **the** definition of what an update
/// means, called by every adapter so none can drift. Enforces the promotion gate
/// before touching anything, so a rejected promotion leaves the fact untouched.
/// Passing `details: Some("")` clears the details; omitting it leaves them.
pub fn apply_fact_patch(fact: &mut Fact, patch: &FactPatch) -> Result<(), MemoryError> {
    if let Some(requested) = patch.provenance {
        check_promotion(fact.provenance, requested, patch.confirmed_by_user)?;
    }
    if let Some(requested) = patch.standing {
        check_standing(fact.standing, requested, patch.confirmed_by_user)?;
    }
    if let Some(content) = &patch.content {
        validate_content(content)?;
    }
    validate_details(patch.details.as_deref())?;
    if let Some(edge) = &patch.edge {
        validate_edge(edge)?;
    }
    // The same gate the write path has, on the other verb that can reach a
    // record's fields — see [`reserved_key`]. **Both of the patch's key lists**:
    // the reserved key is as unwritable off a record as onto one.
    validate_fields(&patch.fields)?;
    validate_cleared_fields(&patch.clear_fields)?;

    if let Some(content) = &patch.content {
        fact.content = normalize_content(content);
    }
    if patch.details.is_some() {
        fact.details = normalize_details(patch.details.as_deref());
    }
    if let Some(status) = patch.status {
        fact.status = status;
    }
    if let Some(provenance) = patch.provenance {
        fact.provenance = provenance;
    }
    // **A patch moves the axis it names and no other.** The two fields answer
    // different questions, so a promotion carrying a standing with it would be
    // the coupling `standing` exists to remove — and it would move it without
    // passing the gate that guards it. A caller meaning both says both.
    if let Some(standing) = patch.standing {
        fact.standing = standing;
    }
    if let Some(edge) = &patch.edge {
        fact.edge = Some(edge.clone());
    }
    // **Cleared first, then set**, so a patch naming one key in both lists
    // leaves the value it asked for rather than depending on which list the
    // reader walked first.
    for key in &patch.clear_fields {
        fact.fields.remove(key.trim());
    }
    for (key, value) in &patch.fields {
        fact.fields.insert(key.trim().to_string(), value.clone());
    }
    // **Cleared before set, for the reason the keys are.** A caller that names
    // both meant the day it named.
    if patch.clear_stale_after {
        fact.stale_after = None;
    }
    if let Some(day) = patch.stale_after {
        fact.stale_after = Some(day);
    }
    Ok(())
}

/// **The writes an edit makes on a record's keys**, in the order
/// [`apply_fact_patch`] applies them: a cleared key carries no value, and a set
/// key carries what it puts there.
///
/// It exists so that what the substrate records and what the record reads back
/// are derived from one place. The keys are trimmed here because the patch
/// trims them there, and a substrate holding `" cost"` under a record reading
/// back `cost` is a history nobody can ask for.
///
/// A patch that names no key makes no write. An edit to a claim's content is not
/// a write of a key, and a history that gained an entry every time a sentence
/// was fixed would count sentences.
///
/// **A clear is scoped to the record it addresses**, which is why `carried` —
/// the addressed record's own fields as they read now — is here. Naming a key
/// that record does not carry is a no-op, exactly as [`FactPatch::clear_fields`]
/// says: the patch describes the record, and the record already does not carry
/// it. Writing the clear anyway would take the key off the THING, because a
/// clear is the newest write of its key — a loss no surface the caller sees
/// would report, since the receipt is the addressed record's projection and the
/// record that really holds the key still reads it back.
pub fn writes_of(
    patch: &FactPatch,
    carried: &BTreeMap<String, String>,
) -> Vec<(String, Option<String>)> {
    patch
        .clear_fields
        .iter()
        .map(|key| key.trim().to_string())
        .filter(|key| carried.contains_key(key))
        .map(|key| (key, None))
        .chain(
            patch
                .fields
                .iter()
                .map(|(key, value)| (key.trim().to_string(), Some(value.clone()))),
        )
        .collect()
}

/// **The thing's fields as they will stand once this edit lands** — what
/// [`guard_fit`] weighs the write against.
///
/// Built over the substrate rather than over the records, because that is the
/// only place the answer is: the patch's own writes take the newest ordinals of
/// their keys, so a set replaces the thing's value whichever record held it —
/// and a clear of a key the addressed record carries takes it off the thing,
/// because that clear is then the newest write of the key. A clear the record
/// does not need writes nothing, so it costs the thing nothing ([`writes_of`]).
///
/// **A status is a write's standing, so an edit that moves the record past
/// re-weighs every key it wrote.** Only writes carried by an active record
/// fold, and the key falls back to the newest write that still counts — which
/// is why this restates the standing of this record's writes rather than only
/// laying the new ones on top.
///
/// `edited` is the record as the patch leaves it: its id says which writes are
/// its, and its status is the standing its writes — old and new — will have.
/// `carried` is that same record as it reads BEFORE the patch, which is what
/// decides whether each clear is a write at all.
pub fn stood_after(
    writes: &[KeyWrite],
    edited: &Fact,
    patch: &FactPatch,
    carried: &BTreeMap<String, String>,
    declared: &[types::DeclaredType],
) -> BTreeMap<String, String> {
    let mut next: Vec<KeyWrite> = writes
        .iter()
        .map(|write| KeyWrite {
            status: if write.fact == edited.id {
                edited.status
            } else {
                write.status
            },
            ..write.clone()
        })
        .collect();
    // In [`writes_of`]'s order and taking the ordinals an append would take, so
    // the guard cannot judge a result the substrate would not produce.
    for (key, value) in writes_of(patch, carried) {
        let ordinal = next
            .iter()
            .filter(|w| w.key == key)
            .map(|w| w.ordinal)
            .max()
            .unwrap_or(0)
            + 1;
        next.push(KeyWrite {
            key,
            ordinal,
            value,
            fact: edited.id.clone(),
            status: edited.status,
        });
    }
    folded_fields(&next, declared)
}

/// **The thing's fields as they will stand once this capture lands** — the
/// capture-side twin of [`stood_after`], and what [`guard_fit`] weighs a new
/// record against.
///
/// A thing's fields are every write on it folded, so a NEW record carrying a
/// key takes that key on the thing exactly as an edit to an old record does.
/// A guard that ran on the edit alone would hold the rule true of one verb
/// while the other walked past it.
///
/// Every key the record carries is a write, taking the next ordinal of its key,
/// which is the ordinal the substrate will give it — a guard judging a result
/// the store would not produce is worse than no guard.
///
/// It reads the declarations for the same reason [`stood_after`] does: how a
/// key folds is declared, so a guard that folded the capture path one way and
/// the edit path another would judge two different things about one store.
pub fn stood_after_capture(
    writes: &[KeyWrite],
    captured: &Fact,
    declared: &[types::DeclaredType],
) -> BTreeMap<String, String> {
    let mut next = writes.to_vec();
    for (key, value) in &captured.fields {
        let ordinal = next
            .iter()
            .filter(|w| &w.key == key)
            .map(|w| w.ordinal)
            .max()
            .unwrap_or(0)
            + 1;
        next.push(KeyWrite {
            key: key.clone(),
            ordinal,
            value: Some(value.clone()),
            fact: captured.id.clone(),
            status: captured.status,
        });
    }
    folded_fields(&next, declared)
}

/// **The entities a write names through a declared reference key**, which must
/// already exist exactly as an edge's object must.
///
/// A reference is a walkable link, so a value naming nothing leaves the same
/// hole an edge into a missing entity leaves. This is rule 3 — everything a
/// write NAMES must already exist — reached through a key rather than through
/// an edge, which is why it is checked against what the write puts there rather
/// than against the thing's whole state. Checking every reference the thing
/// carries would make a link that went missing a wall in front of every later
/// repair.
///
/// **Only a value that is a well-formed handle is named.** A reference key
/// holding a phrase points at nothing and claims to point at nothing; asking
/// whether it exists would refuse loose prose over a link nobody drew. That the
/// phrase does not hold the key is the floor's question ([`guard_fit`]).
///
/// **A key any declaration calls a reference counts**, which is the question
/// the relation walk asks: two types may name one key, and one of them calling
/// it a reference is what makes it walkable.
pub fn referenced_by(
    fields: &BTreeMap<String, String>,
    declared: &[types::DeclaredType],
) -> Vec<EntityId> {
    fields
        .iter()
        .filter_map(|(key, value)| {
            let field = declared
                .iter()
                .filter_map(|d| d.field(key))
                .find(|f| f.holds == types::ValueType::Reference)?;
            // **Every item, because a list of references is a list of links.** A
            // key holding several handles that was read as one string would name
            // a handle nobody wrote and let each real one past unchecked.
            Some(field.items(value))
        })
        .flatten()
        .map(|item| EntityId(item.trim().to_string()))
        .filter(|id| id.kind().is_some())
        .collect()
}

/// **A write may not drop a thing below its kind's required keys, and may not
/// put in any declared key a value that key does not hold.**
///
/// **Two independent rules, and collapsing them is the mistake this shape
/// exists to avoid.** What a thing must HOLD is the required set — small on
/// purpose, because a required key is a refusal waiting to happen. What a key
/// may CONTAIN is checked every time the key is written, required or optional,
/// because an optional key is welcome rather than unchecked.
///
/// Strict is a **floor, not a ceiling**: what a kind requires has to survive,
/// and anything else a caller wants to say is welcome. Adding a key is never
/// refused, including a key nothing mentions, and a record that answers no
/// declaration at all is a first-class record.
///
/// **It reads the RESULT, never the change** — the thing's fields as they will
/// stand once the write lands, against the thing's fields as they stand now. A
/// check on the change itself builds a wedge: a thing already missing a key
/// would have every repair to it measured against a rule it already breaks, and
/// the record could never be fixed. Reading the result means **a thing that
/// fits nothing has nothing to protect**, so nothing is refused and it stays
/// repairable.
///
/// **The thing's own KIND governs, and nothing else does.** The guard is asked
/// about one declaration — the one named by the token in front of the thing's
/// handle — so what may be taken off a `pet:` is what the kind `pet` names.
/// **A declared type governs no write.** A type is the vocabulary a caller asks
/// WITH: it says which things answer a shape, and answering a shape is not the
/// same act as being held to one. Governing by resemblance instead made any
/// thing that structurally completed any declaration in the store subject to
/// it, with nothing offered and nothing confirmed — a thing on which somebody
/// wrote a kind's five keys was governed by a kind it is not.
///
/// **A kind that names no keys governs nothing**, which is every shipped kind
/// today, so today this refuses nothing. That is the floor being empty rather
/// than absent: it fills when a kind carries keys.
///
/// **A key is lost two ways, and both are refused.** Taking the key off is one.
/// Putting a value in it that the key does not hold is the other, because
/// holding a key badly is not holding it — so a thing whose venue slot has a
/// pet in it has stopped being a stay just as surely as one with no venue slot.
/// Both fall out of the one definition of fitting
/// ([`types::Match::complete`]) rather than being two rules that could come to
/// disagree.
///
/// The refusal names the type and the key, because those are what a caller
/// needs to decide what to do; the value refusal also names what the key wanted
/// and what was sent, since a caller looking at a well-formed handle cannot
/// otherwise see what is wrong with it.
pub fn guard_fit(
    kind: &str,
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
    declared: &[types::DeclaredType],
) -> Result<(), MemoryError> {
    // **The prefix as written, never the parsed kind.** A handle whose kind the
    // set never loaded still names one, and a guard that went quiet on an
    // unseeded process would stop refusing rather than say it could not tell.
    for declaration in declared.iter().filter(|d| d.name == kind) {
        // **① What a key HOLDS is checked whenever the key is SET** — required
        // or optional alike, and whether or not the thing meets its floor. The
        // two properties are independent: "the colour has to be a colour" is
        // not "every bike must have a colour".
        //
        // **Only what this write sets.** Asked of the whole result it would
        // re-refuse a bad value already stored, so every repair to a messy
        // thing would be measured against a rule the thing already breaks and
        // the record could never be fixed. Asked behind the floor it would
        // never fire on the write that first puts a value in a key, which is
        // the write worth catching.
        for field in &declaration.fields {
            let Some(value) = after.get(&field.key) else {
                continue;
            };
            if before.get(&field.key) == Some(value) {
                continue;
            }
            if !field.accepts(value) {
                let bad = types::Mistyped {
                    key: field.key.clone(),
                    declared: field.holds,
                    points_at: field.points_at,
                    one_of: field.one_of.clone(),
                    value: value.clone(),
                    required: field.required,
                };
                return Err(MemoryError::BreaksType {
                    name: declaration.name.clone(),
                    key: bad.key.clone(),
                    wanted: bad.wanted(),
                    value: bad.value,
                });
            }
        }
        // **② The floor, and it is the REQUIRED keys alone.** A thing that held
        // every one of them may not lose one. An optional key goes freely: a
        // thing without it is not a thing with something missing.
        //
        // A thing that did not meet its floor before has nothing this protects.
        if !declaration.matched_by(before).is_some_and(|m| m.complete()) {
            continue;
        }
        let lost: Vec<String> = declaration
            .fields
            .iter()
            .filter(|f| f.required)
            .map(|f| f.key.clone())
            .filter(|key| !after.contains_key(key))
            .collect();
        if !lost.is_empty() {
            return Err(MemoryError::BreaksFit {
                name: declaration.name.clone(),
                keys: lost,
            });
        }
    }
    Ok(())
}

/// The write guard's verdict on a metadata edit — the gate every adapter runs
/// before [`apply_entity_patch`], so neither can drift into its own idea of when
/// a patch is suspicious.
///
/// Screened against **the labels the entity will wear once the patch lands**,
/// which is the only set that can collide with anything. A patch supplying
/// neither `name` nor `aliases` inherits the entity's current ones, every
/// incoming label is then one it already wears, and [`guard::decide_relabel`]
/// proceeds — so a source/crm edit needs no special case to stay unscreened.
pub fn screen_entity_patch(
    entity: &Entity,
    patch: &EntityPatch,
    index: &[Entity],
) -> guard::Decision {
    let name = patch.name.as_deref().unwrap_or(&entity.name);
    let aliases: &[String] = patch.aliases.as_deref().unwrap_or(&entity.aliases);
    let incoming: Vec<&str> = std::iter::once(name)
        .chain(aliases.iter().map(String::as_str))
        .collect();
    guard::decide_relabel(
        &entity.id,
        &incoming,
        &entity.labels(),
        index,
        patch.override_token.as_deref(),
    )
}

/// Apply a metadata edit to an entity. Same contract as [`apply_fact_patch`]:
/// validate everything first, mutate only once it all passes.
pub fn apply_entity_patch(entity: &mut Entity, patch: &EntityPatch) -> Result<(), MemoryError> {
    if let Some(name) = &patch.name {
        validate_field("name", name)?;
    }
    if let Some(aliases) = &patch.aliases {
        validate_aliases(aliases)?;
    }
    if let Some(source) = &patch.source {
        validate_field("source", source)?;
    }
    if let Some(crm) = &patch.crm {
        validate_crm(crm)?;
    }

    if let Some(name) = &patch.name {
        entity.name = name.trim().to_string();
    }
    if let Some(aliases) = &patch.aliases {
        entity.aliases = aliases.iter().map(|a| a.trim().to_string()).collect();
    }
    if let Some(source) = &patch.source {
        entity.source = source.trim().to_string();
    }
    if let Some(crm) = &patch.crm {
        entity.crm = Some(crm.trim().to_string());
    }
    Ok(())
}

/// Normalize a fact's optional details the way [`normalize_content`] does its
/// content: edge whitespace can't survive a table cell, and a cell that trims to
/// nothing is no details at all.
pub fn normalize_details(details: Option<&str>) -> Option<String> {
    details
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(str::to_string)
}

/// Normalize a fact's content to the form that survives a table round-trip.
///
/// A markdown table cell cannot preserve leading/trailing whitespace, so edge
/// whitespace is not significant and is trimmed here. Both adapters call this on
/// capture, which is what makes the returned fact **byte-identical** to what a
/// later recall reads back — the fake can't preserve whitespace the real store
/// would drop.
pub fn normalize_content(content: &str) -> String {
    content.trim().to_string()
}

/// A fact about to be captured — everything but the id, which the store mints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFact {
    /// The entity this fact is about.
    pub subject: EntityId,
    /// The crisp claim — what surfaces, like a card title.
    pub content: String,
    /// Nuance / why / merge notes — the description under the title.
    pub details: Option<String>,
    /// Who backs it; defaults to inference.
    pub provenance: Provenance,
    /// How sure anyone is. A caller that says nothing gets
    /// [`Standing::default_for`] its provenance, which is what every claim
    /// written before this field meant — so only a hedge has to be asked for.
    pub standing: Option<Standing>,
    /// Lifecycle state; a fresh capture is [`FactStatus::Active`].
    pub status: FactStatus,
    /// The fact's own freshness stamp, authoritative in the source.
    pub date: Date,
    /// The typed edge this fact draws, if it draws one. Written atomically with
    /// the fact: an edge is never a second, separately-failing write.
    ///
    /// There is deliberately **no `override_token` on this record.** Every
    /// entity a capture names — its subject, its edge's object, a record's refs
    /// — must already exist (see [`guard::decide_existing`]), so there is no
    /// suspicion for a caller to wave away and no refusal that mints a token: a
    /// new entity is `add_entity` and then this, two steps.
    pub edge: Option<Edge>,
    /// The flat bag of fields this record carries. **Empty is the ordinary
    /// case, not a special one** — see [`Fact::fields`].
    pub fields: BTreeMap<String, String>,
    /// The entities this record points at — see [`Fact::refs`].
    pub refs: Vec<EntityId>,
    /// The claim this one was derived from, if any — see [`Fact::derived_from`].
    /// Written atomically with the fact, exactly as an edge is.
    pub derived_from: Option<FactAddress>,
    /// **When somebody should look at this again** — see [`Fact::stale_after`].
    /// Optional, and most claims never carry one.
    pub stale_after: Option<Date>,
}

impl NewFact {
    /// A fact about `subject` with default provenance (inference) and active
    /// status — the common shape this slice captures.
    pub fn about(subject: EntityId, content: impl Into<String>, date: Date) -> Self {
        NewFact {
            subject,
            content: content.into(),
            details: None,
            provenance: Provenance::default(),
            standing: None,
            status: FactStatus::default(),
            date,
            edge: None,
            fields: BTreeMap::new(),
            refs: Vec::new(),
            derived_from: None,
            stale_after: None,
        }
    }
}

/// A captured fact — a [`NewFact`] with the id its home assigned and its content
/// normalized to storage form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    /// Light/local id, unique in its home.
    pub id: FactId,
    /// The entity whose doc physically holds this row. With [`Fact::id`] it
    /// forms the fact's global [`FactAddress`] — the handle `update_fact` takes.
    pub home: EntityId,
    /// The entity this fact is about.
    pub subject: EntityId,
    /// The crisp claim.
    pub content: String,
    /// Nuance / why / merge notes.
    pub details: Option<String>,
    /// Who backs this claim — testimony vs inference.
    pub provenance: Provenance,
    /// How sure anyone is — settled vs open. The other axis, and independent
    /// of [`Fact::provenance`]; see [`Standing`].
    pub standing: Standing,
    /// Lifecycle state.
    pub status: FactStatus,
    /// The fact's own freshness stamp — **when the claim is true OF**, which
    /// is not when anybody learned it. See [`Fact::inserted_at`] for the other
    /// clock.
    pub date: Date,
    /// **When jojobot took this record in.** Written by the store, never by a
    /// caller, and never edited afterwards.
    ///
    /// **The two clocks are allowed to disagree, and that is the point rather
    /// than an edge case.** A booking is true of a day that has not happened
    /// yet; a backfill carries years of records that are true of days long
    /// past and arrive this morning. Neither is checked against the other,
    /// because both are ordinary.
    ///
    /// **`None` is a record from before this was recorded**, and it stays
    /// `None`: filling it in would claim jojobot learned something at a moment
    /// nobody observed, which is the one property this stamp exists to make
    /// unforgeable.
    pub inserted_at: Option<jiff::Timestamp>,
    /// **The day after which this reading stops being good**, if anybody said.
    ///
    /// **A fact about our KNOWLEDGE, not about the world.** A pass that runs
    /// out on a date is a claim about the world and belongs in the claim; this
    /// says how long a reading stays good. Nothing outside jojobot knows this
    /// date, which is the test that decides where it lives.
    ///
    /// **Past it, the claim does not stop being true — it stops being
    /// trusted**, and a read says so. `None` is not freshness and not
    /// staleness: it is a claim that **made no promise** about how long it
    /// stays good.
    ///
    /// **Nothing fires on it.** No sweep, no job, no reminder: a claim past its
    /// day says so when somebody reads it, and that is the whole behaviour.
    pub stale_after: Option<Date>,
    /// The typed edge this fact draws, if any. Read tolerantly: a cell the reader
    /// can't parse costs the edge, never the fact.
    pub edge: Option<Edge>,
    /// **The fields this record carries — always, and empty is ordinary.**
    ///
    /// A flat bag, sorted, that jojobot stores and never interprets. There are
    /// no native types and no schema: a key this build has never seen is simply
    /// a key, kept as it was written, and what the real types eventually are
    /// gets derived from what accumulates here. That is why the reader must not
    /// be allowed opinions — it is going to be wrong about the shape, and being
    /// wrong must cost nothing.
    ///
    /// **Nothing has to be declared for a record to carry fields.** While a
    /// field could only be written beside a class name its writer chose, "the
    /// fields of a thing" meant "the fields somebody opted in", and every read
    /// that groups a thing's records ran over that biased sample.
    pub fields: BTreeMap<String, String>,
    /// **The entities this record points at under no key at all.**
    ///
    /// Links whose nature is deferred — see [`EdgeShape::Connection`] for why
    /// that is not the same as `about` and must not be collapsed into it. A
    /// field holding a handle is the same link with the key doing the
    /// annotating; [`Fact::linked`] is what reads both.
    pub refs: Vec<EntityId>,
    /// **The claim this one was derived from, if it was derived from a claim
    /// rather than from an entity.** An edge's object is an [`EntityId`]; a
    /// claim derived from another claim has no entity to point at, only the
    /// other claim's own [`FactAddress`] — a different shape of reference,
    /// not a wider one. One link, with one fixed meaning: this is not a
    /// vocabulary of relations, and it draws no edge of its own.
    pub derived_from: Option<FactAddress>,
}

impl Fact {
    /// **Every entity this record points at — whatever key it sits under.**
    ///
    /// What makes a field's value a walkable reference is that it IS an entity
    /// handle, not the key it happens to have. [`Fact::refs`] is the unnamed
    /// case, used when there is nothing to call the relationship; `mechanic =
    /// person:alpha` is the same link with the key doing the annotating. **The
    /// key is the annotation** — so a projection keyed on the unnamed list
    /// alone would silently miss every named reference.
    ///
    /// That is also the boundary: a key annotates a link cheaply, and it does
    /// not make the link a place to keep things. An edge growing its own fields
    /// is a node that has not admitted it yet.
    ///
    /// Deduplicated and ordered, so two spellings of the same answer cannot
    /// come back as two answers.
    pub fn linked(&self) -> Vec<EntityId> {
        let mut found: Vec<EntityId> = self.refs.clone();
        found.extend(
            self.fields
                .values()
                .map(|v| EntityId(v.trim().to_string()))
                .filter(|id| validate_subject(id).is_ok()),
        );
        found.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        found.dedup();
        found
    }

    /// Whether this record IS a retraction — the question [`check_retractable`]
    /// asks to refuse retracting one.
    ///
    /// **Read off the key jojobot writes**, which is the key no caller may
    /// write: the marker cannot be forged, where a class name a writer chose
    /// could be typed by anybody.
    pub fn is_retraction(&self) -> bool {
        self.fields.contains_key(RETRACTS)
    }

    /// The address this retraction takes back, if it is one.
    pub fn retracts(&self) -> Option<&str> {
        self.fields.get(RETRACTS).map(String::as_str)
    }
}

/// **One write of one key on one thing, as the fold reads the substrate.**
///
/// [`FieldWrite`] is the same row served to a caller who asked for ONE key's
/// history, so it carries the key implicitly and carries the record's address
/// for a reader who wants to go and look. This one is thing-wide: it names its
/// key, and it carries the two things the fold decides on — where the write
/// sits in that key's order, and whether the record it arrived in still counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyWrite {
    /// The key this write was made under.
    pub key: String,
    /// Its place in the order of writes of THIS key on THIS thing, from one —
    /// what makes one of them the newest.
    pub ordinal: u64,
    /// What the write put there; `None` says it took the key off.
    pub value: Option<String>,
    /// The record that carried it, local to the thing — what lets a write about
    /// to change that record's standing be found ([`stood_after`]).
    pub fact: FactId,
    /// The status of the record that carried it.
    pub status: FactStatus,
}

/// **A thing's fields: the newest write of each key on it.**
///
/// What a thing IS gets written down a piece at a time — one sitting records
/// what it weighs, another records when it arrives — so the question "does this
/// carry the keys of a type" is asked of the thing and never of one record. Ask
/// it of a record and a thing described over two sittings answers nothing,
/// which is how most things get written down.
///
/// **It folds the writes and not the records, because those two orders
/// disagree.** A record projects its own fields from the newest write each of
/// its keys made, which throws away both when that write happened relative to
/// another record's and the fact that a key was ever taken off. So a key living
/// on two records, whose OLDER record is edited afterwards, folds to the value
/// the edit replaced — and a key cleared on the newest record comes back from
/// an older one. The substrate resolves the same key by its own per-(thing,
/// key) order; there is one right answer and this is where it is read.
///
/// **A clear is a write.** `None` takes the key off the thing when it is the
/// newest write of that key, and it stops doing so the moment somebody writes
/// the key again.
///
/// **Only writes carried by [`FactStatus::Active`] records fold.** A record
/// that was taken back or moved past is not what the thing is now, and a thing
/// that fitted a type on the strength of a retracted claim would be conforming
/// because of something nobody stands behind — so such a write is passed over
/// entirely, and the newest write that still counts wins the key.
///
/// **A key jojobot writes itself never folds** ([`reserved_key`]). What a thing
/// IS is what somebody said about it, and the marker is machinery: it says one
/// record takes another back, which is a fact about the rail and not a property
/// of the thing. Folded in, it would read out as the thing's one property — an
/// agent told a thing IS a fact address, a page rendering it under what the
/// thing is, an index posting for it. The record keeps it, which is where
/// [`Fact::is_retraction`] reads it. **Excluded here rather than at the callers
/// because this is the one fold**, so a second reserved key is out of the dense
/// row the day it is named.
pub fn folded_fields(
    writes: &[KeyWrite],
    declared: &[types::DeclaredType],
) -> BTreeMap<String, String> {
    let mut standing: Vec<&KeyWrite> = writes
        .iter()
        .filter(|w| w.status == FactStatus::Active && !reserved_key(&w.key))
        .collect();
    // Ordered here rather than trusted from the caller: the order is what the
    // answer IS, and a store that handed its rows over in another one would be
    // deciding the fold with its query plan.
    standing.sort_by(|a, b| a.key.cmp(&b.key).then(a.ordinal.cmp(&b.ordinal)));
    let mut folded = BTreeMap::new();
    // A counter's running total, kept beside the row because the row holds text
    // and a total has to keep adding. It is dropped with the key, so a clear
    // ends the total the same way it ends the value.
    let mut totals: BTreeMap<&str, Total> = BTreeMap::new();
    for write in standing {
        let Some(value) = &write.value else {
            folded.remove(&write.key);
            totals.remove(write.key.as_str());
            continue;
        };
        if types::fold_of(&write.key, declared) == types::Fold::Newest {
            folded.insert(write.key.clone(), value.clone());
            continue;
        }
        match totals
            .get(write.key.as_str())
            .copied()
            .unwrap_or_default()
            .plus(value)
        {
            Some(total) => {
                totals.insert(&write.key, total);
                folded.insert(write.key.clone(), total.render());
            }
            // **A write that is no number adds nothing, and the key still
            // arrives.** The messy record is the truth and the typed path
            // reports it rather than refusing over it — the same posture
            // [`types::Compare::holds_between`] takes when it is asked to
            // compare a value that will not parse. Hiding the key instead would
            // read as one nobody ever wrote.
            None => {
                folded
                    .entry(write.key.clone())
                    .or_insert_with(|| value.clone());
            }
        }
    }
    folded
}

/// **A counter's running total.**
///
/// Whole numbers add as whole numbers, because that is what a counter counts
/// and a float sum shows a caller rounding they never wrote. The moment a value
/// arrives that is not whole the total becomes fractional and stays there.
#[derive(Debug, Clone, Copy, Default)]
enum Total {
    #[default]
    Empty,
    Whole(i128),
    Fractional(f64),
}

impl Total {
    /// This total with one more value added, or nothing when the value is no
    /// number at all.
    fn plus(self, value: &str) -> Option<Total> {
        let value = value.trim();
        let held = match self {
            Total::Empty => Total::Whole(0),
            held => held,
        };
        match (held, value.parse::<i64>()) {
            (Total::Whole(running), Ok(added)) => Some(Total::Whole(running + i128::from(added))),
            _ => {
                let running = match held {
                    Total::Fractional(running) => running,
                    Total::Whole(running) => running as f64,
                    Total::Empty => 0.0,
                };
                value
                    .parse::<f64>()
                    .ok()
                    .map(|added| Total::Fractional(running + added))
            }
        }
    }

    /// What the folded row holds.
    fn render(self) -> String {
        match self {
            Total::Empty => "0".to_string(),
            Total::Whole(running) => running.to_string(),
            Total::Fractional(running) => running.to_string(),
        }
    }
}

/// **Whether this row can be taken back, and why not when it cannot.**
///
/// Two different mistakes with two different ways out, which is why they are
/// two sentences rather than one refusal. It lives in the domain because both
/// adapters call it: a rule enforced in one store and not the other is a rule
/// that holds until somebody switches stores.
///
/// **One-way is enforced here rather than intended elsewhere.** Nothing takes
/// a row out of [`FactStatus::Retracted`] — not this verb, which refuses a
/// second pass, and not an ordinary edit, which refuses a retracted row
/// outright. A rule that only lives in a tool description is a rule until the
/// first caller who did not read it.
pub fn check_retractable(fact: &Fact) -> Result<(), MemoryError> {
    let refuse = |why: String| {
        Err(MemoryError::NotRetractable {
            attempted: fact.address().to_string(),
            why,
        })
    };
    // **Its own variant, because it is not the same refusal.** The other two
    // say the act cannot be performed on this row; this one says it has been,
    // and the caller is asking for a state jojobot is already holding.
    if fact.status == FactStatus::Retracted {
        return Err(MemoryError::AlreadyRetracted {
            attempted: fact.address().to_string(),
        });
    }
    if fact.is_retraction() {
        return refuse(
            "it is itself a retraction. A retraction is the last word on what it takes back; \
             retracting one would be the reversal the one-way rule exists to forbid. If the \
             retraction was a mistake, capture what is so now as a new record"
                .to_string(),
        );
    }
    Ok(())
}

/// The record a retraction leaves behind: **a dated record of its own**, homed
/// with the record it takes back and naming it by address.
///
/// **Two rows rather than one, and this is what an append-only substrate looks
/// like in miniature.** The reason is not written into the row being retracted,
/// because that row is what somebody wrote and a later act does not get to
/// rewrite it. Taking something back is itself a thing that happened, on a day,
/// so it is recorded the way everything else that happened is. The two then
/// read as one story: the retracted row is marked, and the row beside it says
/// why.
///
/// Inference, like any other claim jojobot did not hear from the user
/// directly. The retraction is a real act either way — what is a hypothesis is
/// the REASON, and defaulting a reason to testimony would bless the retracting
/// agent's own account of itself.
///
/// **The reason is optional**, and when it is absent the record says that
/// rather than inventing one. A row still needs content, so there is a
/// sentence here either way; this one states only the act and the absence,
/// which is the whole of what jojobot knows when nobody said why.
pub fn retraction_of(
    target: &Fact,
    reason: Option<&str>,
    date: Date,
) -> Result<NewFact, MemoryError> {
    check_retractable(target)?;
    let reason = reason.map(str::trim).filter(|r| !r.is_empty());
    if let Some(reason) = reason {
        validate_content(reason)?;
    }
    let content = match reason {
        Some(reason) => normalize_content(reason),
        None => "retracted; no reason was given".to_string(),
    };
    Ok(NewFact {
        // **The link is written here, by jojobot, and never by a caller** — a
        // retraction that pointed wherever its author said would be a way to
        // mark somebody else's record taken back. That is why the key is
        // reserved; see [`reserved_key`].
        fields: [(RETRACTS.to_string(), target.address().to_string())]
            .into_iter()
            .collect(),
        ..NewFact::about(target.subject.clone(), content, date)
    })
}

impl Fact {
    /// This fact's global address — returned with every read precisely so the
    /// caller can turn around and edit it.
    pub fn address(&self) -> FactAddress {
        FactAddress::new(self.home.clone(), self.id.clone())
    }
}

/// **One write of one key on one thing — the substrate a field read projects
/// from.**
///
/// A key is written a piece at a time: every capture carrying it and every edit
/// reaching it is a write of its own, kept. The current value of the key is the
/// newest of them, which is what an ordinary read answers with; the writes
/// themselves are what [`Memory::history`] answers with, and how many there are
/// is the answer to "how many times".
///
/// **The address is the thing and the key**, never the record's id. A hundred
/// sittings that each record one donut are a hundred records, so a history
/// hanging off a record's id would be a hundred histories of length one and
/// could count nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldWrite {
    /// **What this write put there, and `None` says it took the key off.**
    ///
    /// A clear is a write like any other: nothing is removed from the
    /// substrate, so the key stops being current while the writes that put it
    /// there stay readable.
    pub value: Option<String>,
    /// The record that carried the write — its address, so a reader can go and
    /// read what else that sitting said.
    pub fact: FactAddress,
    /// That record's own date: **when**, for a caller counting occurrences over
    /// time.
    pub date: Date,
    /// That record's status. A write inside a record somebody took back still
    /// happened, so it is reported rather than dropped — and reported as
    /// retracted, so a count can leave it out.
    pub status: FactStatus,
}

/// **What a retraction leaves behind: two rows, and both come back.**
///
/// The marked record and the account of why it was marked are one answer,
/// because they are one story — handing back only the mark would leave the
/// caller holding a record it could not explain, and handing back only the
/// account would not prove the mark landed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retraction {
    /// The row that was taken back: same id, same content, same place, now
    /// carrying [`FactStatus::Retracted`].
    pub retracted: Fact,
    /// The retraction itself — a dated event naming what it takes back.
    pub record: Fact,
}

/// The result of a write that names an entity: it either happened, or the write
/// guard stopped it and is asking. Modelled as a value rather than an error so
/// every caller has to face the question — a blocked write is a decision the AI
/// owes, not a failure to log and move past.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Guarded<T> {
    /// No suspicion, or the caller had already resolved it: this is the record.
    Written(T),
    /// **Nothing was written.** The way out depends on the gate: a creation or a
    /// rename takes one of the candidates or the token this refusal minted
    /// (which never clears an exact handle); a write that only *names* an entity
    /// takes an existing handle or an [`add_entity`](Memory::add_entity) first,
    /// because it cannot create one. `candidates` may be empty — an unrecognized
    /// handle is blocked whether or not anything resembles it.
    Blocked {
        /// The handle the caller tried to write.
        attempted: EntityId,
        /// What the guard found, strongest first.
        candidates: Vec<guard::EntityMatch>,
    },
}

impl<T> Guarded<T> {
    /// The written record, or `None` if the guard blocked the write.
    pub fn written(self) -> Option<T> {
        match self {
            Guarded::Written(v) => Some(v),
            Guarded::Blocked { .. } => None,
        }
    }
}

/// Why a memory operation failed. Adapters map their transport/parse errors into
/// these; the domain and the MCP layer speak only this vocabulary.
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    /// The claim is malformed for storage (empty, or spans multiple lines).
    #[error("invalid fact: {0}")]
    InvalidFact(String),
    /// The subject id is not a well-formed entity id (see [`validate_subject`]).
    /// Treated as adversarial: it never reaches the store.
    /// **The refusal carries its own reason**, because there are three and they
    /// have three repairs: the handle is not shaped like one, the kind set was
    /// never loaded, or nobody declared that kind. **It does not recite a list
    /// of kinds**: the set is data, so a sentence naming ten of them is one
    /// that goes stale the first time an instance declares an eleventh.
    #[error("invalid entity id {0}")]
    InvalidSubject(String),
    /// **Nothing loaded the kind set, so no handle can be read** — including
    /// the most ordinary handle in the store.
    ///
    /// Apart from [`InvalidSubject`](Self::InvalidSubject) because the repair
    /// is a different act by a different person (rule 68). That one says the
    /// call carries something wrong and the caller fixes it. This one says the
    /// call is fine: the set is loaded at startup, no verb re-reads it, and
    /// what repairs it is a person restarting the server. Telling a caller to
    /// send the same call again here is advice that cannot succeed.
    ///
    /// `attempted` carries the handle that could not be read, when one was
    /// named. A read that failed on stored rows has no single handle to blame,
    /// and blaming one would be a guess.
    #[error("{}{}", attempted_handle(attempted), kinds::NotAKind::SetNeverLoaded)]
    KindsNeverLoaded {
        /// The handle that could not be read, if the call named one.
        attempted: Option<String>,
    },
    /// A fact address didn't parse (see [`FactAddress::parse`]).
    #[error("invalid fact address '{0}': expected kind:slug#local-id, e.g. person:alpha#f3")]
    InvalidAddress(String),
    /// An entity field is malformed for storage (empty, multi-line, or carrying
    /// markdown that would break out of its frontmatter line).
    #[error("invalid entity: {0}")]
    InvalidEntity(String),
    /// The edge doesn't hold: a shape without an object (or the reverse), or an
    /// object of the wrong kind for its shape.
    #[error("invalid edge: {0}")]
    InvalidEdge(String),
    /// The search query can't be served as asked (see
    /// [`SearchQuery::validate`](search::SearchQuery::validate)).
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    /// The type declaration is malformed for storage: no keys, a key named
    /// twice, or a name that is not one plain label (see
    /// [`validate_type`](types::validate_type)). **It never says a record is
    /// wrong** — nothing on this path checks a record against a declaration.
    #[error("invalid type: {0}")]
    InvalidType(String),
    /// **The name belongs to a type the software ships, and a caller cannot
    /// write over one.**
    ///
    /// Not a malformed call: the declaration is well formed and the name is
    /// real. A shipped type is closed — a caller can neither extend it, shrink
    /// it nor replace it — so the way forward is a name of the caller's own,
    /// never the same call sent again.
    ///
    /// Apart from [`InvalidType`](Self::InvalidType) because the fix is
    /// different: that one says fix the declaration, this one says the
    /// declaration is fine and the name is not yours.
    #[error(
        "'{name}' is a type that ships with the software, and a shipped type is closed to callers"
    )]
    ShippedType {
        /// The type name that was declared.
        name: String,
    },
    /// **The write would drop the thing below a type it already fits.**
    ///
    /// Not a malformed call: the edit is well formed and the record is real.
    /// What it would cost is a key the thing needs to go on being what it is,
    /// so the way forward is to leave the key where it is or to put its value
    /// somewhere the type can still see — never the same call again.
    #[error(
        "this would leave '{name}' incomplete on this thing: it needs {}",
        keys.join(", ")
    )]
    BreaksFit {
        /// The type that would stop being answered.
        name: String,
        /// The keys it names that the thing would no longer carry.
        keys: Vec<String>,
    },
    /// **The write would put a value in a key that the key does not hold, on a
    /// thing that already fits the type.**
    ///
    /// The same loss as [`BreaksFit`](Self::BreaksFit) reached the other way:
    /// holding a key badly is not holding it, so the thing stops fitting. It
    /// is a variant of its own because the way forward is different — the key
    /// is not going anywhere and what has to change is the value.
    #[error("'{key}' holds {wanted} on anything that is a '{name}', and '{value}' is not one")]
    BreaksType {
        /// The type the thing would stop fitting.
        name: String,
        /// The key whose value the type refuses.
        key: String,
        /// What the key was declared to hold, as the declaration spells it —
        /// `date`, or `reference:place`.
        wanted: String,
        /// What the write would have put there, so the caller sees what it
        /// sent rather than being told it was wrong.
        value: String,
    },
    /// The addressed fact doesn't exist, in an entity that does. Never
    /// auto-created, never guessed at — the live addresses come back so the
    /// caller can retarget. An address that misses on its *handle* is
    /// [`MemoryError::UnknownEntity`] instead: different mistake, different fix.
    #[error("no fact at '{attempted}'{}", live_addresses(nearest))]
    UnknownFact {
        /// The address that missed.
        attempted: String,
        /// Addresses that do exist, nearest first.
        nearest: Vec<String>,
    },
    /// The named entity doesn't exist. Same rule: report, never create.
    #[error("no entity '{attempted}'{}", nearest_handles(nearest))]
    UnknownEntity {
        /// The handle that missed.
        attempted: String,
        /// What the write guard found nearby.
        nearest: Vec<guard::EntityMatch>,
    },
    /// **The addressed row is already retracted, and that is the state the
    /// caller was asking for.**
    ///
    /// Apart from [`NotRetractable`](Self::NotRetractable) because the two
    /// refuse for opposite reasons. That one says the act cannot be performed
    /// on this row; this one says it has been. Answering a caller who asks for
    /// a retraction jojobot is already holding with "this did not happen and
    /// cannot happen" is false in both directions at once, and a caller acts on
    /// it: the record they wanted taken back is taken back, and they are told
    /// to treat it as live.
    ///
    /// **Retract only.** A row refused an ordinary EDIT because it is retracted
    /// is a different answer and keeps the other variant: there the caller
    /// asked for something else and the retracted state is what stands in the
    /// way, not what they wanted.
    #[error("'{attempted}' is already retracted")]
    AlreadyRetracted {
        /// The address that was aimed at.
        attempted: String,
    },
    /// **The addressed row cannot be taken back**, and `why` says which of the
    /// two remaining reasons it is: itself a retraction, or an ordinary fact —
    /// which is fixed in place rather than retracted.
    ///
    /// A refusal rather than a no-op: a caller who asked to retract something
    /// and got a shrug would reasonably believe it happened.
    #[error("cannot retract '{attempted}': {why}")]
    NotRetractable {
        /// The address that was aimed at.
        attempted: String,
        /// Which reason, in a sentence that names the way forward.
        why: String,
    },
    /// A claim can only become testimony on the user's explicit confirmation.
    #[error(
        "promoting inference → testimony requires the user's explicit confirmation \
         (confirmed_by_user); jojobot infers freely but never blesses on its own"
    )]
    UnconfirmedPromotion,
    /// An open claim was asked to be settled without the user saying so.
    #[error(
        "settling an open claim requires the user's explicit confirmation \
         (confirmed_by_user); the operator hedged this claim, and only the operator can \
         withdraw the hedge"
    )]
    UnconfirmedSettling,
    /// The underlying store failed — it, or the layer that carries and parses
    /// its answers. **A clean failure**: a write either commits or does not,
    /// so the record is as it was and retrying is a reasonable next move.
    #[error("store error: {0}")]
    Store(String),
}

/// Render the addresses that do exist. An entity that simply holds nothing says
/// so — trailing off into an empty list ("addresses here: ") named nothing,
/// pointed at nothing, and read like a bug in the server rather than an answer.
fn live_addresses(nearest: &[String]) -> String {
    if nearest.is_empty() {
        return "; that entity has no facts yet".to_string();
    }
    format!("; addresses here: {}", nearest.join(", "))
}

/// Name the handle a never-loaded refusal was reached through, when there is
/// one to name.
fn attempted_handle(attempted: &Option<String>) -> String {
    match attempted {
        Some(handle) => format!("'{handle}': "),
        None => String::new(),
    }
}

/// Render the guard's nearby candidates for an error message.
fn nearest_handles(nearest: &[guard::EntityMatch]) -> String {
    if nearest.is_empty() {
        return String::new();
    }
    let list: Vec<String> = nearest
        .iter()
        .map(|m| format!("{} ({})", m.handle, m.name))
        .collect();
    format!("; did you mean: {}", list.join(", "))
}

/// The Memory port — six verbs over entities and the facts about them. One real
/// adapter stands behind it in production (Outline); a fake stands behind it in
/// tests. Three invariants bind every adapter:
///
/// * **read-back** — a write succeeds only if reading it back through the read
///   path returns it, byte-identical. Writing is not recording.
/// * **the guard is on the write path** — every entity-touching write screens
///   against the index first, so it cannot be skipped by a caller who forgot.
/// * **never create on a miss** — an unknown address or handle errors, or comes
///   back blocked, with the nearest candidates. Guessing is how two people
///   become one; auto-provisioning is how one typo becomes a second person.
///   Only [`add_entity`](Memory::add_entity) brings an entity into existence.
#[async_trait::async_trait]
pub trait Memory: Send + Sync {
    /// Create an entity. Kind-general: the handle carries the kind. Screened by
    /// the write guard, so this can come back [`Guarded::Blocked`].
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError>;

    /// Every entity jojobot knows, optionally filtered to one kind.
    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError>;
    /// **Which records point at this thing through a field**, whatever key
    /// they used.
    ///
    /// A reference key makes a value a link, and this is that link read from
    /// the far end: given a handle, the records elsewhere carrying it. The
    /// caller decides what the keys MEAN — this only says which records name
    /// it — so a reader after entitlements and a reader after anything else
    /// ask the same question and sort the answer themselves.
    ///
    /// **Records of every status**, superseded included, exactly as `recall`
    /// answers: a caller reading who points here is reading history as often
    /// as current truth.
    ///
    /// Defaulted off [`list_entities`](Memory::list_entities) and
    /// [`recall`](Memory::recall), like [`children`](Memory::children): an
    /// adapter that can find the holders without reading every entity
    /// overrides it, and one that cannot is still correct.
    async fn referring_to(&self, target: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        validate_subject(target)?;
        let mut pointing = Vec::new();
        for entity in self.list_entities(None).await? {
            for fact in self.recall(&entity.id).await? {
                if fact
                    .fields
                    .values()
                    .any(|value| value.trim() == target.as_str())
                {
                    pointing.push(fact);
                }
            }
        }
        Ok(pointing)
    }

    /// The entities sitting directly under `parent` — **their handles, and
    /// nothing else.**
    ///
    /// Handles are the whole point. Zooming is the reason the tree exists, and
    /// a parent read that dragged its subtree along would be the silting it was
    /// built to stop: the caller descends deliberately, one level at a time,
    /// paying only for the branch it actually wants. Direct children only, for
    /// the same reason — a level, not a subtree.
    ///
    /// **Derived, never stored.** The pointer lives on the child
    /// ([`Entity::parent`]); this reads the other way down the same one edge,
    /// so a parent and its children cannot come to disagree about who is whose.
    ///
    /// An unknown parent is [`MemoryError::UnknownEntity`], never an empty
    /// list: "nothing is under it" and "there is no such thing" are different
    /// answers, and a caller that cannot tell them apart will read a typo as a
    /// leaf.
    ///
    /// Ordering carries no meaning — nothing records where a child sits among
    /// its siblings. The handles come back sorted only so that two reads of an
    /// unchanged store agree.
    ///
    /// Defaulted off [`list_entities`](Memory::list_entities), like
    /// [`scan_entity`](Memory::scan_entity): an adapter that can do better
    /// overrides it, and one that can't is still correct.
    async fn children(&self, parent: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        validate_subject(parent)?;
        let all = self.list_entities(None).await?;
        if !all.iter().any(|e| &e.id == parent) {
            return Err(MemoryError::UnknownEntity {
                attempted: parent.to_string(),
                nearest: guard::screen(parent, &[], &all),
            });
        }
        let mut handles: Vec<EntityId> = all
            .into_iter()
            .filter(|e| e.parent.as_ref() == Some(parent))
            .map(|e| e.id)
            .collect();
        handles.sort();
        Ok(handles)
    }

    /// Edit an entity's metadata in place. Never the handle. A change to what it
    /// is **called** — its name or its aliases — is screened by the write guard
    /// just as a creation is (see [`screen_entity_patch`]), so this can come
    /// back [`Guarded::Blocked`]. An unknown handle is
    /// [`MemoryError::UnknownEntity`], never a create.
    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError>;

    /// Write a fact and return it with the id its home assigned, its content
    /// normalized. The returned fact must be visible — byte-identical — to a
    /// subsequent [`recall`](Memory::recall) of its subject. **Both entities it
    /// names — the subject and an edge's object — must already exist**
    /// ([`guard::decide_existing`]); this verb never creates one, so a handle it
    /// cannot resolve comes back [`Guarded::Blocked`].
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError>;

    /// Read back every fact belonging to `subject`, in an unspecified order:
    /// facts *about* it, and facts **homed in its doc** whatever their subject
    /// column says. Home-doc membership counts because a mistyped subject cell
    /// must not be able to hide a doc's own rows from the entity whose page they
    /// sit on. Each carries its [`FactAddress`] — that is what makes them
    /// editable, and it is how such a row gets repaired.
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError>;

    /// Edit one addressed fact in place (fix-the-source). An unknown address is
    /// [`MemoryError::UnknownFact`], never a create. A patch that attaches an
    /// **edge** names an entity, so it faces the write guard and can come back
    /// [`Guarded::Blocked`] — an edit is a write like any other.
    ///
    /// **A retracted row is not editable**, by anyone, for anything: that is
    /// where one-way is actually enforced, since a status flip back to active
    /// would otherwise be an ordinary patch away.
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
    ) -> Result<Guarded<Fact>, MemoryError>;

    /// **Every write of one key on one thing, oldest first** — what the
    /// ordinary read projects away.
    ///
    /// A read answers with current truth: one value per key, the newest write.
    /// This answers with the writes behind it, so the same data serves both
    /// questions — what the key holds now, and every time it was written. The
    /// count is the answer to "how many times", which is why the address is the
    /// thing and the key ([`FieldWrite`]) rather than a record's id.
    ///
    /// A key nobody has written is an empty list, not a miss: the thing exists
    /// and nothing was recorded under that key. An entity that does not exist
    /// is [`MemoryError::UnknownEntity`], exactly as [`recall`](Memory::recall)
    /// answers one.
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError>;

    /// **What the thing HOLDS: one value per key, the newest write winning.**
    ///
    /// The other read of the same substrate [`history`](Memory::history) reads:
    /// that one answers with every write of one key, this one with the standing
    /// value of every key. It is a read the store owes rather than a fold a
    /// caller does for itself, because the order that decides it is the
    /// substrate's own — see [`folded_fields`], which is what an adapter runs
    /// over its rows so the two stores cannot come to disagree about what a
    /// thing holds.
    ///
    /// A thing nobody has written a key on is an empty map, not a miss. An
    /// entity that does not exist is [`MemoryError::UnknownEntity`], exactly as
    /// [`recall`](Memory::recall) and [`history`](Memory::history) answer one:
    /// "nothing is recorded here" and "there is no such thing" are different
    /// answers with different repairs.
    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError>;

    /// **Take back an event** — one way, never reversed, and still a write.
    ///
    /// Nothing is removed: the addressed row keeps its id, its content and its
    /// place, and gains [`FactStatus::Retracted`]. Beside it lands the
    /// retraction itself, a dated record naming what it takes back and why
    /// ([`retraction_of`]), so the two read as one story instead of a marked
    /// row nobody can account for.
    ///
    /// **Both rows in one write.** They share a home document, so an adapter
    /// that marks the row and then fails to record the reason has produced the
    /// one state this verb must not leave behind — a record taken back with no
    /// account of why. That is also why it is a verb of its own rather than a
    /// flag on [`update_fact`](Memory::update_fact): the ceremony is the point,
    /// and a boolean is what gets flipped by accident.
    ///
    /// [`check_retractable`] decides what may be taken back; a row that may not
    /// is [`MemoryError::NotRetractable`], and an address naming nothing is
    /// [`MemoryError::UnknownFact`] exactly as an edit's would be.
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
    ) -> Result<Retraction, MemoryError>;

    /// Replace an entity's **prose** — the human half of its doc, everything
    /// that is neither jojobot's metadata nor its facts. A bot's charter is
    /// prose; so is a portrait, later. Returns the stored text, which a
    /// subsequent [`scan_entity`](Memory::scan_entity) must return unchanged.
    ///
    /// **Replaced whole, never appended**: prose is what is so now, and a page
    /// that accumulated every past version could not be read back. The facts
    /// sharing the doc are untouched. An unknown handle is
    /// [`MemoryError::UnknownEntity`] — this verb never creates a doc to hold
    /// the text, exactly as no other verb here creates on a miss.
    async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError>;

    /// Every document in the store, whole: its prose, the entity it is, and the
    /// facts in its table. This is the **index's boot scan** — the search
    /// projection is rebuilt from it by a plain full re-fetch at start, which is
    /// what keeps the index a projection and not a second source of truth.
    async fn scan(&self) -> Result<Vec<search::DocScan>, MemoryError>;

    /// One entity's document, scanned. The incremental half of the same
    /// mechanism: after a write, the index re-reads the touched doc **from the
    /// store** rather than patching itself from what the writer believed — so a
    /// partial-update bug has nowhere to live.
    ///
    /// Defaulted off [`scan`](Memory::scan), so an adapter that can do better
    /// overrides it and one that can't is still correct.
    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<search::DocScan>, MemoryError> {
        Ok(self
            .scan()
            .await?
            .into_iter()
            .find(|d| d.entity.as_ref().is_some_and(|e| &e.id == entity)))
    }

    /// **Declare a type: a name, and the keys a record of it carries.**
    ///
    /// Write-time help. It tells a writer which keys to fill and what belongs
    /// in them, and it admits nothing: a record carrying these keys answers
    /// this type whether or not this was ever called (see
    /// [`DeclaredType::matched_by`](types::DeclaredType::matched_by)). So this
    /// verb can never make a record findable or stop one being found.
    ///
    /// **The declaration is replaced whole**, for the reason prose is: a type
    /// is the set of keys it names NOW, and one that accumulated every key it
    /// ever named would describe no record. Returns what was stored, which a
    /// subsequent [`declared_types`](Memory::declared_types) must return
    /// unchanged.
    ///
    /// **A type the software ships is the one thing a caller cannot replace**,
    /// and the store is what holds that: it reads the origin of what it already
    /// has under the name and answers [`MemoryError::ShippedType`]. A shipped
    /// declaration may still be written, which is how the software moves its
    /// own type — see [`guard_replacement`](types::guard_replacement).
    ///
    /// A malformed declaration is [`MemoryError::InvalidType`]. Nothing else
    /// is refused — there is no near-miss screen here, because a type name
    /// that resembles another names a second set of keys rather than a second
    /// copy of one thing.
    async fn declare_type(
        &self,
        declared: types::DeclaredType,
    ) -> Result<types::DeclaredType, MemoryError>;

    /// Every type anybody has declared, each complete with its keys in the
    /// order it declared them.
    ///
    /// **The declarations are not the records**, and a caller reading this is
    /// reading what a writer was told to fill, never what the store holds.
    async fn declared_types(&self) -> Result<Vec<types::DeclaredType>, MemoryError>;

    /// **Declare a kind** — the namespace a handle carries and the schema of
    /// what it names (rule 213).
    ///
    /// It is a declaration of its own rather than a type with no keys,
    /// because a type with no keys is refused: a schema that names nothing is
    /// not a schema, while a kind that names nothing yet is an ordinary kind.
    /// What the two share is the [`types::Origin`] mechanism, and they share
    /// it rather than each having one.
    ///
    /// **A shipped kind is closed to a caller.** Declaring one again with a
    /// caller's origin is refused, exactly as a shipped type is, so the
    /// software's own nouns cannot be reshaped from outside. Re-declaring a
    /// shipped kind AS shipped is the seed running again and changes nothing.
    /// **The keys are keys like any other**, kept where a schema's keys are
    /// kept and owned by the kind's name. The kind row holds what makes it a
    /// kind; hanging its keys off that row would be one concept in two tables
    /// rather than the two halves of one sentence in the two places they
    /// belong.
    async fn declare_kind(
        &self,
        token: &str,
        origin: types::Origin,
        fields: Vec<types::Field>,
    ) -> Result<(), MemoryError>;

    /// **Every kind the store holds**, with where each came from.
    ///
    /// This is what a process loads into [`kinds`] at startup. It is a read of
    /// the store rather than of any list in this crate: a kind the store lost
    /// is a kind this process must stop parsing, and a list here could not
    /// say so.
    async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError>;
}

#[cfg(test)]
mod tests {
    use super::testing::{InMemoryMemory, contract};
    use super::*;

    /// One write of a key, at the place in that key's history it took.
    fn wrote(key: &str, ordinal: u64, value: Option<&str>) -> KeyWrite {
        KeyWrite {
            key: key.to_string(),
            ordinal,
            value: value.map(str::to_string),
            fact: FactId("f1".into()),
            status: FactStatus::Active,
        }
    }

    /// **A field key has a length, and the domain is what says so.**
    ///
    /// The column holding a key is 191 characters and a caller could write
    /// more, so the store was the thing deciding where a key stopped working:
    /// a short key round-tripped and a long one came back as a store failure,
    /// which is a caller mistake wearing a broken-server answer (rules 9 and
    /// 68).
    ///
    /// **Both ends in one case.** A limit that refused everything would pass a
    /// check that only sent the long key, and a limit that refused nothing
    /// would pass a check that only sent the short one. The refusal has to say
    /// the number, because a caller that cannot read the limit off the answer
    /// finds it by bisection.
    #[test]
    fn a_field_key_is_bounded_by_the_domain_and_the_refusal_says_the_bound() {
        let at_the_limit = BTreeMap::from([("k".repeat(MAX_KEY_CHARS), "value".to_string())]);
        validate_fields(&at_the_limit).expect("a key at the limit is one a caller may write");

        let over = BTreeMap::from([("k".repeat(MAX_KEY_CHARS + 1), "value".to_string())]);
        let refused = validate_fields(&over)
            .expect_err("a key past the limit is refused before it reaches a store");
        let said = refused.to_string();
        assert!(
            said.contains(&MAX_KEY_CHARS.to_string()),
            "the refusal must carry the limit a caller has to write under: {said}"
        );
    }

    /// **A key declared a counter sums its writes; every other key still takes
    /// the newest.**
    ///
    /// Both halves in one case. A fold that summed every key would pass a check
    /// that only looked at the counter, and a fold that summed nothing would
    /// pass a check that only looked at the other key.
    ///
    /// The last read is the negative the whole feature rests on: the same
    /// writes, with nothing declared, fold the way they always have. Without it
    /// the case passes on a build that sums every number it sees, and the
    /// declaration would be buying nothing.
    #[test]
    fn a_counter_sums_its_writes_and_every_other_key_takes_the_newest() {
        let writes = vec![
            wrote("donuts", 1, Some("1")),
            wrote("donuts", 2, Some("1")),
            wrote("donuts", 3, Some("1")),
            wrote("mood", 1, Some("hungry")),
            wrote("mood", 2, Some("content")),
        ];
        let snacking = types::DeclaredType::new(
            "snacking",
            vec![
                types::Field::summing("donuts"),
                types::Field::new("mood", types::ValueType::Text),
            ],
        );

        let folded = folded_fields(&writes, std::slice::from_ref(&snacking));
        assert_eq!(
            folded.get("donuts").map(String::as_str),
            Some("3"),
            "three writes of one is three: {folded:?}",
        );
        assert_eq!(
            folded.get("mood").map(String::as_str),
            Some("content"),
            "a key nobody declared a counter reads back its newest write: {folded:?}",
        );

        let undeclared = folded_fields(&writes, &[]);
        assert_eq!(
            undeclared.get("donuts").map(String::as_str),
            Some("1"),
            "with nothing declared the same writes take the newest, which is what the \
             declaration changes: {undeclared:?}",
        );
    }

    /// **A clear ends a total, and the writes after it start a new one.**
    ///
    /// A clear is a write and takes the key off the thing. On a counter that
    /// has to reset it, or a key somebody cleared would keep counting from
    /// whatever it held before nobody held it.
    #[test]
    fn clearing_a_counter_ends_its_total_and_the_next_write_starts_over() {
        let snacking = types::DeclaredType::new("snacking", vec![types::Field::summing("donuts")]);
        let cleared = folded_fields(
            &[
                wrote("donuts", 1, Some("2")),
                wrote("donuts", 2, Some("3")),
                wrote("donuts", 3, None),
            ],
            std::slice::from_ref(&snacking),
        );
        assert_eq!(
            cleared.get("donuts"),
            None,
            "a clear takes a counter off the thing exactly as it takes any key off: {cleared:?}",
        );

        // **Two writes after the clear, so the three answers are three different
        // numbers.** Counting since the clear is five; counting every write is
        // ten; taking the newest is four. One write after the clear would leave
        // the first and the third indistinguishable, and the case would pass on
        // a build that sums nothing at all.
        let again = folded_fields(
            &[
                wrote("donuts", 1, Some("2")),
                wrote("donuts", 2, Some("3")),
                wrote("donuts", 3, None),
                wrote("donuts", 4, Some("1")),
                wrote("donuts", 5, Some("4")),
            ],
            std::slice::from_ref(&snacking),
        );
        assert_eq!(
            again.get("donuts").map(String::as_str),
            Some("5"),
            "the total after a clear counts the writes since it, not the five before: {again:?}",
        );
    }

    /// **Only the writes that count are counted.**
    ///
    /// A write carried by a record somebody took back is not what the thing is
    /// now, and the rule already holds for newest-wins. A sum that added every
    /// row would resurrect the retracted one as arithmetic, where the old fold
    /// merely passed it over.
    #[test]
    fn a_counter_passes_over_a_write_whose_record_no_longer_counts() {
        let snacking = types::DeclaredType::new("snacking", vec![types::Field::summing("donuts")]);
        let folded = folded_fields(
            &[
                wrote("donuts", 1, Some("1")),
                KeyWrite {
                    status: FactStatus::Retracted,
                    ..wrote("donuts", 2, Some("40"))
                },
                wrote("donuts", 3, Some("1")),
            ],
            std::slice::from_ref(&snacking),
        );
        assert_eq!(
            folded.get("donuts").map(String::as_str),
            Some("2"),
            "the retracted forty is passed over and the two standing ones add: {folded:?}",
        );
    }

    /// The invariant, red→green, in milliseconds against the fake: a capture
    /// succeeds only if a subsequent recall returns the fact.
    #[tokio::test]
    async fn capture_reads_back_against_the_fake() {
        contract::capture_reads_back(&InMemoryMemory::booted()).await;
    }

    /// The full behavioural contract holds for the fake — the same suite the
    /// gated integration test runs against real Outline.
    #[tokio::test]
    async fn fake_satisfies_the_contract() {
        contract::run_all(&InMemoryMemory::booted()).await;
    }

    #[test]
    fn person_id_prefixes_a_bare_handle_but_respects_a_typed_one() {
        assert_eq!(EntityId::person("alpha").as_str(), "person:alpha");
        assert_eq!(EntityId::person("person:alpha").as_str(), "person:alpha");
    }

    #[test]
    fn validate_subject_accepts_ids_and_rejects_adversarial_ones() {
        // **The set is this case's subject, not its setup.** What makes an
        // adversarial id adversarial is that its kind half names no kind the
        // set holds, so the refusals below are statements about the loaded set.
        crate::memory::kinds::load_shipped();
        assert!(validate_subject(&EntityId::person("alpha")).is_ok());
        assert!(validate_subject(&EntityId("project:jojobot-server".into())).is_ok());
        // Injection vectors: newline, pipe, header, fence, space, uppercase, empty.
        for bad in [
            "person:a|b",
            "a\nb",
            "### forged",
            "a`b",
            "a b",
            "Person:Alpha",
            "",
        ] {
            assert!(
                validate_subject(&EntityId(bad.into())).is_err(),
                "must reject {bad:?}"
            );
        }
    }

    /// Every shipped kind round-trips through its wire token, and nothing else
    /// parses — a token nobody declared can never enter the store.
    #[test]
    fn the_shipped_kinds_round_trip_and_the_set_is_closed() {
        // **The set is this case's subject, not its setup.** The case walks the
        // shipped tokens and then asserts the set holds nothing else, so what
        // is loaded is the whole content of both assertions.
        crate::memory::kinds::load_shipped();
        let all = [
            (EntityKind::PERSON, "person"),
            (EntityKind::PROJECT, "project"),
            (EntityKind::PLACE, "place"),
            (EntityKind::EVENT, "event"),
            (EntityKind::WORK, "work"),
            (EntityKind::THING, "thing"),
            (EntityKind::ORG, "org"),
            (EntityKind::TOPIC, "topic"),
            (EntityKind::BOT, "bot"),
            (EntityKind::PET, "pet"),
            (EntityKind::RHYTHM, "rhythm"),
        ];
        for (kind, token) in all {
            assert_eq!(kind.as_token(), token);
            assert_eq!(EntityKind::from_token(token), Some(kind));
        }
        assert_eq!(
            EntityKind::ALL.len(),
            all.len(),
            "every shipped kind is named above, and no other",
        );
        for unknown in ["receipt", "self", "Person", "", "peson"] {
            assert_eq!(
                EntityKind::from_token(unknown),
                None,
                "{unknown:?} is not a kind"
            );
        }
    }

    /// **A bot is an entity like any other.** Its handle validates by the same
    /// grammar, so the codec, the guard, `search`, `recall` and `list_entities`
    /// need no per-kind branch to carry it.
    #[test]
    fn a_bot_handle_is_an_ordinary_entity_id() {
        // **The set is this case's subject, not its setup.** The claim is that
        // `bot` sits in the set beside the other kinds and is read the same
        // way, so what is loaded is the thing being asserted.
        crate::memory::kinds::load_shipped();
        let id = EntityId::new(EntityKind::BOT, "otto");
        assert_eq!(id.as_str(), "bot:otto");
        assert_eq!(id.kind(), Some(EntityKind::BOT));
        assert!(validate_subject(&id).is_ok());
        // And it is spelled out on a bare handle, exactly as every non-person is.
        assert_eq!(EntityId::person("bot:otto").as_str(), "bot:otto");
    }

    /// An id is `kind:slug` — the kind and the slug are readable off it, which is
    /// what lets the guard compare slugs and the codec stamp a kind.
    #[test]
    fn an_id_splits_into_its_kind_and_slug() {
        // **The set is this case's subject, not its setup.** The kind half of
        // the split is answered from the set, so what comes back is a read of
        // it rather than of the string.
        crate::memory::kinds::load_shipped();
        let id = EntityId::new(EntityKind::PROJECT, "jojobot-server");
        assert_eq!(id.as_str(), "project:jojobot-server");
        assert_eq!(id.kind(), Some(EntityKind::PROJECT));
        assert_eq!(id.slug(), "jojobot-server");
        // A malformed id yields no kind rather than panicking — reads never hard-fail.
        assert_eq!(EntityId("nonsense".into()).kind(), None);
    }

    /// The grammar is `kind:slug` with slug `[a-z0-9-]+`: an unknown kind, a
    /// missing kind, an underscore, or a second colon is not an entity id.
    #[test]
    fn validate_subject_enforces_the_kind_slug_grammar() {
        // **The set is this case's subject, not its setup.** The grammar's
        // first half is "a kind", and only the set can say whether a token is
        // one.
        crate::memory::kinds::load_shipped();
        for good in [
            "person:alpha",
            "topic:widgets",
            "org:north-trail-club",
            "thing:red-bike",
        ] {
            assert!(
                validate_subject(&EntityId(good.into())).is_ok(),
                "must accept {good:?}"
            );
        }
        for bad in [
            "alpha",           // no kind
            "receipt:il-2026", // not one of the nine
            "person:",         // empty slug
            ":alpha",          // empty kind
            "person:a_b",      // underscore is out of the slug charset
            "person:a:b",      // one colon only
        ] {
            assert!(
                validate_subject(&EntityId(bad.into())).is_err(),
                "must reject {bad:?}"
            );
        }
    }

    /// The compound address `doc#local-id` — what `recall` hands back and
    /// `update_fact` targets — round-trips, and a malformed one is rejected.
    #[test]
    fn a_fact_address_round_trips_through_its_wire_form() {
        // **This case runs in a booted process.** It parses a handle and asserts
        // about something else, so the set is setup — and setup comes from
        // standing a store up, filled from what that store holds.
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let addr = FactAddress::new(EntityId::person("alpha"), FactId("f3".into()));
        assert_eq!(addr.to_string(), "person:alpha#f3");
        assert_eq!(FactAddress::parse("person:alpha#f3").unwrap(), addr);
        for bad in [
            "person:alpha",
            "#f3",
            "person:alpha#",
            "person:alpha#f 3",
            "nope:x#f1",
            "",
        ] {
            assert!(FactAddress::parse(bad).is_err(), "must reject {bad:?}");
        }
    }

    /// Both lifecycle states have tokens; an unknown or blank cell degrades to
    /// active (the tolerant-read rule: never drop a fact over a bad cell).
    ///
    /// And the **legacy `negated` token reads as superseded**. Negation-as-status
    /// is gone — a refutation is an ordinary content edit now — but rows written
    /// under it are on disk, and a schema removal must never hard-fail a read
    /// any more than a schema addition may. Superseded is the honest landing
    /// spot: the behaviour that mattered, excluded-by-default, is identical.
    #[test]
    fn fact_status_tokens_round_trip_and_a_legacy_negated_reads_as_superseded() {
        for status in [FactStatus::Active, FactStatus::Superseded] {
            assert_eq!(FactStatus::from_token(status.as_token()), status);
        }
        assert_eq!(
            FactStatus::from_token("negated"),
            FactStatus::Superseded,
            "a row from before negation was removed still reads, and stays out of a default search"
        );
        assert_eq!(FactStatus::from_token(""), FactStatus::Active);
        assert_eq!(FactStatus::from_token("garbled"), FactStatus::Active);
    }

    /// Four shapes, no more — and each has two spellings on purpose: the token a
    /// caller passes (and the table stores) and the schema.org name a response
    /// renders. `membership`/`memberOf` and `attendance`/`attendee` are where
    /// they diverge; input stays lowercase, always.
    #[test]
    fn the_edge_shapes_round_trip_and_the_set_is_closed() {
        let all = [
            (EdgeShape::Location, "location", "location"),
            (EdgeShape::Membership, "membership", "memberOf"),
            (EdgeShape::Attendance, "attendance", "attendee"),
            (EdgeShape::About, "about", "about"),
            (EdgeShape::Connection, "connection", "relatedTo"),
        ];
        for (shape, token, name) in all {
            assert_eq!(shape.as_token(), token);
            assert_eq!(shape.as_name(), name);
            assert_eq!(EdgeShape::from_token(token), Some(shape));
        }
        assert_eq!(
            EdgeShape::ALL.len(),
            5,
            "four shapes from M2, plus the untyped one events point with"
        );
        // A response name is NOT an input token: the input grammar is unchanged.
        for unknown in ["memberOf", "attendee", "knows", "Location", "", "locaton"] {
            assert_eq!(
                EdgeShape::from_token(unknown),
                None,
                "{unknown:?} is not a shape token"
            );
        }
    }

    /// **The fifth shape, and the whole point of it is that it is not the
    /// fourth.**
    ///
    /// An event may point at entities whose relationship nobody recorded. The
    /// tempting move is to file those as `about`, since `about` already takes
    /// any kind — and that is exactly the move this shape exists to refuse.
    /// `about` ASSERTS that the record is about that entity, which is a claim
    /// somebody made; a `connection` ADMITS a link whose meaning is unknown.
    /// Collapsing the two launders an unknown into an assertion, and every
    /// reader downstream then reads a claim nobody ever made.
    ///
    /// The pointer is real either way — that is the other half. Only the NATURE
    /// of the link is deferred, so it must still be walkable, which is what
    /// `an_untyped_edge_is_walkable_like_any_other` holds it to.
    #[test]
    fn the_untyped_shape_is_its_own_shape_and_never_about() {
        // **This case runs in a booted process.** It parses a handle and asserts
        // about something else, so the set is setup — and setup comes from
        // standing a store up, filled from what that store holds.
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        assert_eq!(EdgeShape::Connection.as_token(), "connection");
        assert_eq!(
            EdgeShape::from_token("connection"),
            Some(EdgeShape::Connection)
        );
        assert_ne!(
            EdgeShape::Connection,
            EdgeShape::About,
            "an admitted link and an asserted one are not the same edge"
        );
        // …and they do not share a response name either, or a reader sorting by
        // name would put them back together.
        assert_ne!(EdgeShape::Connection.as_name(), EdgeShape::About.as_name());

        // **Any kind, like `about`** — an event points at whatever it points at,
        // and refusing a kind here would be jojobot deciding what the link
        // means, which is the thing it does not know.
        assert_eq!(EdgeShape::Connection.object_kind(), None);
        for object in ["person:alpha", "place:north-trail", "event:winter-fest"] {
            assert!(
                validate_edge(&Edge::new(EdgeShape::Connection, EntityId(object.into()))).is_ok(),
                "an untyped edge takes {object}"
            );
        }
        // The id grammar is still enforced: unknown MEANING is not unknown SHAPE.
        assert!(
            validate_edge(&Edge::new(EdgeShape::Connection, EntityId("a|b".into()))).is_err(),
            "a malformed handle is malformed whatever the link means"
        );
    }

    /// Each shape pins its object's kind — `about` is the one open shape. A
    /// `location` pointing at a person is a mis-drawn edge, not a nuance, and it
    /// is refused before anything is written.
    #[test]
    fn an_edge_object_must_be_the_kind_its_shape_requires() {
        // **The set is this case's subject, not its setup.** The kind of the
        // object is what this refuses on, read from the set.
        crate::memory::kinds::load_shipped();
        let ok = [
            (EdgeShape::Location, "place:north-trail"),
            (EdgeShape::Membership, "org:north-trail-club"),
            (EdgeShape::Attendance, "event:winter-fest"),
            (EdgeShape::About, "topic:widgets"),
            (EdgeShape::About, "person:alpha"),
        ];
        for (shape, object) in ok {
            assert!(
                validate_edge(&Edge::new(shape, EntityId(object.into()))).is_ok(),
                "{shape} must accept {object}"
            );
        }
        let bad = [
            (EdgeShape::Location, "person:alpha"),
            (EdgeShape::Membership, "place:north-trail"),
            (EdgeShape::Attendance, "project:atlas"),
        ];
        for (shape, object) in bad {
            let err = validate_edge(&Edge::new(shape, EntityId(object.into())))
                .expect_err("a wrong-kind object must be refused");
            assert!(
                matches!(err, MemoryError::InvalidEdge(_)),
                "expected InvalidEdge for {shape}/{object}, got {err:?}"
            );
        }
        // The object is an entity id first: the grammar is checked as a subject's is.
        let err = validate_edge(&Edge::new(EdgeShape::About, EntityId("a|b".into())))
            .expect_err("a malformed object must be refused");
        assert!(matches!(err, MemoryError::InvalidSubject(_)), "got {err:?}");
    }

    /// A **bare `\r`** is refused exactly as `\n` is, in content and in details.
    /// It looks harmless — until a store normalizes line endings, which markdown
    /// pipelines routinely do. Then the row splits, the split ends the table's
    /// contiguous run of `|` lines, and every fact BELOW it stops being read too
    /// (the blast radius the codec's `bare_cr` tests demonstrate).
    #[test]
    fn a_bare_carriage_return_is_refused_like_a_newline() {
        for bad in ["hello\rworld", "trailing\r", "\rleading", "a\r\nb"] {
            assert!(
                validate_content(bad).is_err(),
                "content must refuse a bare CR: {bad:?}"
            );
            assert!(
                validate_details(Some(bad)).is_err(),
                "details ride in the same row, so they refuse it too: {bad:?}"
            );
        }
        assert!(validate_content("hello world").is_ok());
        assert!(validate_details(Some("plain details")).is_ok());
    }

    /// **An entity answers to more than one name.** The display name is what it
    /// is *called*; an alias is what someone actually says — the nickname, the
    /// short form, the initials. Without them the guard cannot recognize a name
    /// the user uses every day, and search cannot find it.
    ///
    /// An alias is a plain one-line label, exactly as `name` is, with one extra
    /// rule: **no comma**, because the frontmatter carries the set on one
    /// comma-separated line and an alias with a comma in it would silently
    /// become two.
    #[test]
    fn an_alias_is_a_plain_label_and_never_carries_the_separator() {
        assert!(validate_aliases(&["Cosme Fulanito".into(), "H.".into()]).is_ok());
        assert!(
            validate_aliases(&[]).is_ok(),
            "no aliases is the ordinary case"
        );
        for bad in ["", "   ", "one, two", "two\nlines", "back`tick"] {
            assert!(
                validate_aliases(&[bad.into()]).is_err(),
                "must refuse the alias {bad:?}"
            );
        }
    }

    /// Aliases patch like every other metadata field: `None` leaves them alone,
    /// `Some` replaces the whole set — including `Some(vec![])`, which is how a
    /// caller says "it has none", a thing they must be able to say.
    #[test]
    fn an_alias_set_is_replaced_whole_or_left_alone() {
        let mut entity = Entity {
            id: EntityId::person("alpha"),
            kind: EntityKind::PERSON,
            name: "Alpha".into(),
            aliases: vec!["Al".into()],
            source: "user-named".into(),
            crm: None,
            parent: None,
            boot: Boot::OnDemand,
        };

        apply_entity_patch(
            &mut entity,
            &EntityPatch {
                source: Some("crm-card".into()),
                ..Default::default()
            },
        )
        .expect("patch ok");
        assert_eq!(
            entity.aliases,
            vec!["Al".to_string()],
            "an omitted field is left alone"
        );

        apply_entity_patch(
            &mut entity,
            &EntityPatch {
                aliases: Some(vec!["  Al  ".into(), "Alph".into()]),
                ..Default::default()
            },
        )
        .expect("patch ok");
        assert_eq!(
            entity.aliases,
            vec!["Al".to_string(), "Alph".to_string()],
            "the set is replaced whole, and trimmed the way a name is"
        );

        apply_entity_patch(
            &mut entity,
            &EntityPatch {
                aliases: Some(Vec::new()),
                ..Default::default()
            },
        )
        .expect("patch ok");
        assert!(
            entity.aliases.is_empty(),
            "an empty set is a set, not an omission"
        );

        assert!(
            apply_entity_patch(
                &mut entity,
                &EntityPatch {
                    aliases: Some(vec!["one, two".into()]),
                    ..Default::default()
                }
            )
            .is_err(),
            "a malformed alias is refused before anything is mutated"
        );
    }

    /// The **labels** of an entity: its name and every alias, which is the set
    /// the guard screens and search indexes. One definition, so "what is this
    /// thing called" cannot come to mean two different things in two places.
    #[test]
    fn an_entitys_labels_are_its_name_and_its_aliases() {
        let entity = |name: &str, aliases: Vec<String>| Entity {
            id: EntityId::person("alpha"),
            kind: EntityKind::PERSON,
            name: name.into(),
            aliases,
            source: "user-named".into(),
            crm: None,
            parent: None,
            boot: Boot::OnDemand,
        };
        assert_eq!(
            entity("Alpha", vec!["Al".into(), "Alph".into()]).labels(),
            vec!["Alpha", "Al", "Alph"],
            "the display name leads; it is the one the entity is filed under"
        );
        assert_eq!(entity("Alpha", Vec::new()).labels(), vec!["Alpha"]);
        assert!(
            entity("", vec!["  ".into()]).labels().is_empty(),
            "an entity with nothing written on it has no labels, not blank ones"
        );
    }
    #[test]
    fn provenance_tokens_round_trip_and_degrade_to_inference() {
        assert_eq!(Provenance::from_token("testimony"), Provenance::Testimony);
        assert_eq!(Provenance::from_token("inference"), Provenance::Inference);
        assert_eq!(Provenance::from_token(""), Provenance::Inference);
        assert_eq!(Provenance::from_token("garbled"), Provenance::Inference);
    }
}
