//! Memory — facts, portraits, rules-and-receipts.
//!
//! Provenance is a *type*, not a convention: a fact the user stated is
//! testimony; anything derived is inference, and inference is the default.
//! Making this an enum means every place that consumes a fact must decide how
//! it treats the two — the compiler lists the sites.
//!
//! This module carries the [`Memory`] port and the records it moves — the
//! [`Entity`] (a noun, one of nine [`EntityKind`]s) and the [`Fact`] (an
//! assertion about one). Six verbs, bound by one invariant: a write succeeds
//! only if the read path returns it. There is no privileged owner entity — the
//! user is a person like any other. The port is pure (no rmcp, no reqwest);
//! adapters behind it (the in-memory fake, the real Outline store) live outside
//! this crate, and the write guard ([`guard`]) sits on their write paths.
//!
//! **This is user-agnostic software: no user PII, fixtures included.** Records
//! name roles and synthetic placeholders; every real specific is data, read from
//! the store at runtime.

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

pub mod guard;
pub mod search;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// The nine kinds of noun jojobot knows about — a **closed** set, each earned
/// by an inventory of real data. Closed is the point: an id whose kind isn't one
/// of these is not an entity id, so no unknown kind can enter the store, and
/// every consumer that matches on a kind is exhaustive by construction.
///
/// `project` is jojobot's own personal-goal sense (trips, big rocks, builds),
/// deliberately not schema.org's Organization-subtype meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityKind {
    /// People in the user's life and public figures alike (artists included).
    Person,
    /// An activity with a status funnel: big rocks, builds, processes, trips.
    Project,
    /// Venues, destinations, trails, informal spots.
    Place,
    /// A dated occurrence: shows, stays, outings, festivals.
    Event,
    /// A creative/media artifact with its own identity: sets, albums, posts.
    Work,
    /// A named possession or device with a history: bikes, plants, machines.
    Thing,
    /// Clubs, venues-as-institutions, labels, schools, vendors.
    Org,
    /// The glue noun: interest areas, and the anchor for world-facts that
    /// attach to no person, place, or project.
    Topic,
    /// An AI identity: a handle, the charter its doc's prose carries, the rules
    /// and memory its facts carry, and the mailbox it owns. A noun like any
    /// other — **nothing about a bot is compiled in**; a bot is data in the
    /// operator's own store, and this kind is only what lets it be one.
    Bot,
}

impl EntityKind {
    /// Every kind, in declaration order — the enumeration `list_entities`
    /// filters over and the guard scans.
    pub const ALL: [EntityKind; 9] = [
        EntityKind::Person,
        EntityKind::Project,
        EntityKind::Place,
        EntityKind::Event,
        EntityKind::Work,
        EntityKind::Thing,
        EntityKind::Org,
        EntityKind::Topic,
        EntityKind::Bot,
    ];

    /// The wire token — the `kind:` prefix of an id and the frontmatter value.
    pub fn as_token(self) -> &'static str {
        match self {
            EntityKind::Person => "person",
            EntityKind::Project => "project",
            EntityKind::Place => "place",
            EntityKind::Event => "event",
            EntityKind::Work => "work",
            EntityKind::Thing => "thing",
            EntityKind::Org => "org",
            EntityKind::Topic => "topic",
            EntityKind::Bot => "bot",
        }
    }

    /// Parse a kind token. Strict — unlike the tolerant `status`/`provenance`
    /// cells, an unknown kind has no safe fallback: guessing one would file a
    /// record under a noun the user never chose.
    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_token() == token)
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
/// nine kinds** — no per-kind fields: what varies between a person and a place
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
    /// The kanban token this entity is mirrored by, if any (`card:554`).
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

/// An entity about to be created. `create_new` is the caller's explicit
/// "I checked, it's a different one" answer to the write guard.
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
    /// Optional kanban token.
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
    /// Set only after the guard reported candidates and the caller judged them
    /// different. Never clears an exact handle collision.
    pub create_new: bool,
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
            create_new: false,
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
    /// New kanban token.
    pub crm: Option<String>,
    /// Set only after the guard reported candidates for the new name and the
    /// caller judged them different. Same signal as [`NewEntity::create_new`].
    pub create_new: bool,
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
    /// A typed edge to attach. `None` leaves any existing edge alone; this
    /// milestone writes one edge per fact, so setting it replaces.
    pub edge: Option<Edge>,
    /// The user's explicit confirmation, required to promote a claim to
    /// testimony. jojobot infers freely; it never blesses on its own.
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
    /// The wire token written to the table's `provenance` column.
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

/// The shape of a fact's structured edge — a **closed** set of four, and the only
/// shapes this milestone writes. The general edges vocabulary (`receiptOf`,
/// `supersedes`, `derivedFrom`, …) arrives with the graph milestone.
///
/// These four exist because ask-across — "which friends are in Shelbyville?", "what's
/// connected to Duff Fest?" — must never rest on an AI scanning prose. A fact that
/// puts an entity somewhere, in something, at something, or about something
/// produces a typed edge **at capture**, so a cross-entity question is an edge
/// walk instead of fifty sequential reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeShape {
    /// The subject is somewhere. Object is a [`EntityKind::Place`].
    Location,
    /// The subject belongs to something. Object is an [`EntityKind::Org`].
    Membership,
    /// The subject was at something. Object is an [`EntityKind::Event`].
    Attendance,
    /// The subject is about something — the open shape: any kind of object.
    About,
}

impl EdgeShape {
    /// Every shape, in declaration order.
    pub const ALL: [EdgeShape; 4] = [
        EdgeShape::Location,
        EdgeShape::Membership,
        EdgeShape::Attendance,
        EdgeShape::About,
    ];

    /// The **input and storage** token — what a caller passes and what the
    /// table's `edges` cell holds.
    pub fn as_token(self) -> &'static str {
        match self {
            EdgeShape::Location => "location",
            EdgeShape::Membership => "membership",
            EdgeShape::Attendance => "attendance",
            EdgeShape::About => "about",
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
            EdgeShape::Location => Some(EntityKind::Place),
            EdgeShape::Membership => Some(EntityKind::Org),
            EdgeShape::Attendance => Some(EntityKind::Event),
            EdgeShape::About => None,
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
}

impl FactStatus {
    /// The wire token written to the table's `status` column.
    pub fn as_token(self) -> &'static str {
        match self {
            FactStatus::Active => "active",
            FactStatus::Superseded => "superseded",
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

/// Validate an entity id before it is written anywhere. Ids are **structured**
/// (`kind:slug`, kind one of the nine, slug `[a-z0-9-]+`), never free text — so
/// an adversarial subject can neither forge markdown nor invent a kind. This is
/// the primary defence; escaping-on-write is the belt-and-suspenders.
pub fn validate_subject(subject: &EntityId) -> Result<(), MemoryError> {
    let s = subject.as_str();
    let ok = s.len() <= 128
        && subject.kind().is_some()
        && !subject.slug().is_empty()
        && subject.slug().bytes().all(is_slug_byte);
    if ok {
        Ok(())
    } else {
        Err(MemoryError::InvalidSubject(s.to_string()))
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

/// Validate the optional `crm` link. It points at a kanban token and has exactly
/// one shape — `card:N` — so a typo can't quietly become a dangling pointer.
pub fn validate_crm(crm: &str) -> Result<(), MemoryError> {
    let ok = crm
        .strip_prefix("card:")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    if ok {
        Ok(())
    } else {
        Err(MemoryError::InvalidEntity(format!(
            "crm must be card:N, got '{crm}'"
        )))
    }
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
            "prose is empty; a page with nothing on it is not a charter".into(),
        ));
    }
    if let Some(reserved) = prose
        .lines()
        .map(str::trim)
        .find(|line| RESERVED_PROSE_LINES.contains(line))
    {
        return Err(MemoryError::InvalidEntity(format!(
            "prose carries the line '{reserved}', which a document reserves for its own \
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
pub fn validate_content(content: &str) -> Result<(), MemoryError> {
    if content.trim().is_empty() {
        return Err(MemoryError::InvalidFact("content is empty".into()));
    }
    if breaks_the_row(content) {
        return Err(MemoryError::InvalidFact(
            "content spans multiple lines; a table cell is one line".into(),
        ));
    }
    Ok(())
}

/// Details ride in the same table row, so they are one line too — but may be
/// absent.
pub fn validate_details(details: Option<&str>) -> Result<(), MemoryError> {
    if details.is_some_and(breaks_the_row) {
        return Err(MemoryError::InvalidFact(
            "details span multiple lines; a table cell is one line".into(),
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
    if let Some(content) = &patch.content {
        validate_content(content)?;
    }
    validate_details(patch.details.as_deref())?;
    if let Some(edge) = &patch.edge {
        validate_edge(edge)?;
    }

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
    if let Some(edge) = &patch.edge {
        fact.edge = Some(edge.clone());
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
        patch.create_new,
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
    /// Testimony vs inference; defaults to inference.
    pub provenance: Provenance,
    /// Lifecycle state; a fresh capture is [`FactStatus::Active`].
    pub status: FactStatus,
    /// The fact's own freshness stamp, authoritative in the source.
    pub date: Date,
    /// The typed edge this fact draws, if it draws one. Written atomically with
    /// the fact: an edge is never a second, separately-failing write.
    ///
    /// There is deliberately **no `create_new` on this record.** Every entity a
    /// capture names — its subject, its edge's object — must already exist (see
    /// [`guard::decide_existing`]), so there is no suspicion for a caller to
    /// wave away: a new entity is `add_entity` and then this, two steps.
    pub edge: Option<Edge>,
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
            status: FactStatus::default(),
            date,
            edge: None,
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
    /// Testimony vs inference.
    pub provenance: Provenance,
    /// Lifecycle state.
    pub status: FactStatus,
    /// The fact's own freshness stamp.
    pub date: Date,
    /// The typed edge this fact draws, if any. Read tolerantly: a cell the reader
    /// can't parse costs the edge, never the fact.
    pub edge: Option<Edge>,
}

impl Fact {
    /// This fact's global address — returned with every read precisely so the
    /// caller can turn around and edit it.
    pub fn address(&self) -> FactAddress {
        FactAddress::new(self.home.clone(), self.id.clone())
    }
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
    /// rename takes one of the candidates or an explicit create-new signal
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
    #[error(
        "invalid entity id '{0}': ids are kind:slug — kind one of \
         person|project|place|event|work|thing|org|topic|bot, slug [a-z0-9-]+"
    )]
    InvalidSubject(String),
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
    /// A claim can only become testimony on the user's explicit confirmation.
    #[error(
        "promoting inference → testimony requires the user's explicit confirmation \
         (confirmed_by_user); jojobot infers freely but never blesses on its own"
    )]
    UnconfirmedPromotion,
    /// **A write failed, and putting the record back failed too.** It is left
    /// mid-verb: not written, not restored, and not something the caller can
    /// retry its way out of.
    ///
    /// Its own variant for the reason the mailbox and session contexts' are:
    /// whether the rollback worked is the one thing a caller cannot infer from
    /// anything else in the answer. This context had no such variant at all,
    /// so a failed rollback here arrived as an ordinary [`MemoryError::Store`]
    /// with the outcome written into the sentence — which is precisely the
    /// shape the other two exist to prevent.
    #[error(
        "{verb} failed ({cause}) AND putting it back failed ({rollback}) — {} is left mid-{verb}, \
         and a person has to look",
        .stranded.join(", ")
    )]
    Stranded {
        /// The verb that failed.
        verb: String,
        /// The ids left mid-write.
        stranded: Vec<String>,
        /// What failed first.
        cause: String,
        /// Why the rollback could not undo it.
        rollback: String,
    },
    /// The underlying store (Outline, or its network/parse layer) failed.
    #[error("store error: {0}")]
    Store(String),
    /// The store isn't configured (no credentials). Production fronts real
    /// Outline; until it's wired, the memory verbs refuse rather than lie.
    #[error("memory store not configured: {0}")]
    NotConfigured(String),
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
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
    ) -> Result<Guarded<Fact>, MemoryError>;

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
}

#[cfg(test)]
mod tests {
    use super::testing::{InMemoryMemory, contract};
    use super::*;

    /// The invariant, red→green, in milliseconds against the fake: a capture
    /// succeeds only if a subsequent recall returns the fact.
    #[tokio::test]
    async fn capture_reads_back_against_the_fake() {
        contract::capture_reads_back(&InMemoryMemory::new()).await;
    }

    /// The full behavioural contract holds for the fake — the same suite the
    /// gated integration test runs against real Outline.
    #[tokio::test]
    async fn fake_satisfies_the_contract() {
        contract::run_all(&InMemoryMemory::new()).await;
    }

    #[test]
    fn person_id_prefixes_a_bare_handle_but_respects_a_typed_one() {
        assert_eq!(EntityId::person("alpha").as_str(), "person:alpha");
        assert_eq!(EntityId::person("person:alpha").as_str(), "person:alpha");
    }

    #[test]
    fn validate_subject_accepts_ids_and_rejects_adversarial_ones() {
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

    /// All nine kinds round-trip through their wire token, and nothing else
    /// parses — the enum is closed, so an unknown kind can never enter the store.
    #[test]
    fn the_nine_kinds_round_trip_and_the_set_is_closed() {
        let all = [
            (EntityKind::Person, "person"),
            (EntityKind::Project, "project"),
            (EntityKind::Place, "place"),
            (EntityKind::Event, "event"),
            (EntityKind::Work, "work"),
            (EntityKind::Thing, "thing"),
            (EntityKind::Org, "org"),
            (EntityKind::Topic, "topic"),
            (EntityKind::Bot, "bot"),
        ];
        for (kind, token) in all {
            assert_eq!(kind.as_token(), token);
            assert_eq!(EntityKind::from_token(token), Some(kind));
        }
        assert_eq!(EntityKind::ALL.len(), 9, "nine kinds, no more");
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
        let id = EntityId::new(EntityKind::Bot, "otto");
        assert_eq!(id.as_str(), "bot:otto");
        assert_eq!(id.kind(), Some(EntityKind::Bot));
        assert!(validate_subject(&id).is_ok());
        // And it is spelled out on a bare handle, exactly as every non-person is.
        assert_eq!(EntityId::person("bot:otto").as_str(), "bot:otto");
    }

    /// An id is `kind:slug` — the kind and the slug are readable off it, which is
    /// what lets the guard compare slugs and the codec stamp a kind.
    #[test]
    fn an_id_splits_into_its_kind_and_slug() {
        let id = EntityId::new(EntityKind::Project, "jojobot-server");
        assert_eq!(id.as_str(), "project:jojobot-server");
        assert_eq!(id.kind(), Some(EntityKind::Project));
        assert_eq!(id.slug(), "jojobot-server");
        // A malformed id yields no kind rather than panicking — reads never hard-fail.
        assert_eq!(EntityId("nonsense".into()).kind(), None);
    }

    /// The grammar is `kind:slug` with slug `[a-z0-9-]+`: an unknown kind, a
    /// missing kind, an underscore, or a second colon is not an entity id.
    #[test]
    fn validate_subject_enforces_the_kind_slug_grammar() {
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
    fn the_four_edge_shapes_round_trip_and_the_set_is_closed() {
        let all = [
            (EdgeShape::Location, "location", "location"),
            (EdgeShape::Membership, "membership", "memberOf"),
            (EdgeShape::Attendance, "attendance", "attendee"),
            (EdgeShape::About, "about", "about"),
        ];
        for (shape, token, name) in all {
            assert_eq!(shape.as_token(), token);
            assert_eq!(shape.as_name(), name);
            assert_eq!(EdgeShape::from_token(token), Some(shape));
        }
        assert_eq!(EdgeShape::ALL.len(), 4, "four shapes in M2, no more");
        // A response name is NOT an input token: the input grammar is unchanged.
        for unknown in ["memberOf", "attendee", "knows", "Location", "", "locaton"] {
            assert_eq!(
                EdgeShape::from_token(unknown),
                None,
                "{unknown:?} is not a shape token"
            );
        }
    }

    /// Each shape pins its object's kind — `about` is the one open shape. A
    /// `location` pointing at a person is a mis-drawn edge, not a nuance, and it
    /// is refused before anything is written.
    #[test]
    fn an_edge_object_must_be_the_kind_its_shape_requires() {
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
            kind: EntityKind::Person,
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
            kind: EntityKind::Person,
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
