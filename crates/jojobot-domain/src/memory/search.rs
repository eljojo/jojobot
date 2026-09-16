//! Retrieval — the vocabulary of the `search` verb, and the port behind it.
//!
//! Ask-across ("which friends are in Shelbyville?", "what's connected to Duff Fest?") is
//! the retrieval jojobot exists to serve, and it is served by **one ranked list**
//! over four things at once: entities, facts, the **prose** a human wrote, and
//! the **messages** left in mailboxes. Mixing them is the point — a detail
//! demoted into a paragraph, or filed in a report to another session, must be
//! findable without anyone having remembered to file it as a fact.
//!
//! **This is the one place the two bounded contexts meet, and it meets them as a
//! reader.** Memory and Mailboxes share no type anywhere else and must not; here
//! a [`Hit`] carries a [`Message`] because the requirement is one ranked list and
//! one front door, not two search verbs a caller has to know to call both of. It
//! is a read-side union: nothing here writes to either context, and neither
//! context learns anything about the other from it.
//!
//! Truth stays in the store; the index is a **projection**, filled by a full
//! re-scan at start, updated in-process on every write, and **refreshed again on
//! every read**. Read-back extends to it: a fact captured a moment ago is
//! findable without a restart. The read-path refresh is what makes an answer
//! backed by a reading taken for it, so a record the store has since lost stops
//! being served — nothing inside the process can observe that loss any other
//! way.
//!
//! This module is pure vocabulary — no tantivy, no I/O. The index that satisfies
//! [`Search`] lives in the adapters.

use std::collections::HashSet;

use super::{
    Edge, EdgeShape, Entity, EntityId, EntityKind, Fact, FactStatus, MemoryError, Provenance,
    Standing, types, validate_edge, validate_subject,
};
use crate::mailbox::Message;
use crate::session::SessionId;

/// One document as the index needs it: its prose, the entity it is (if it is
/// one), and the facts in its table. This is the shape a **full re-scan** yields,
/// and the unit an incremental update re-reads — so the index is always built
/// from the store's own text, never from a diff the writer guessed at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocScan {
    /// The store's id for the document. Opaque to the domain; it is what a hit
    /// hands back so a human can open the page.
    pub doc_id: String,
    /// The document's title — human, renamable, never used to resolve anything.
    pub title: String,
    /// The human-written prose: everything that is neither jojobot's machine
    /// block nor its fact table.
    pub prose: String,
    /// The entity this doc *is*, or `None` for a doc carrying no id marker — a
    /// page the user wrote themselves. Those are still searchable prose.
    pub entity: Option<Entity>,
    /// Every fact the doc's table holds.
    pub facts: Vec<Fact>,
    /// **What the thing this doc IS holds now** — one value per key, folded
    /// from its writes ([`super::folded_fields`]).
    ///
    /// **It travels with the scan because it cannot be worked out from
    /// [`DocScan::facts`].** A record projects its own fields from its own
    /// newest writes, so the records have already thrown away which of them was
    /// written last and which key was taken off; a reader folding them for
    /// itself gets a stale answer that no other read of the store agrees with.
    /// The store is what holds the order, so the store is what says this.
    ///
    /// Empty on a doc that is no entity, and on a thing nobody has written a
    /// key on.
    pub fields: std::collections::BTreeMap<String, String>,
    /// **Who may read this document** — `None` for everything the whole
    /// instance can see, which is every stored row.
    ///
    /// ⚠️ **A PROPERTY, never a field.** `owner` is an ordinary key a caller
    /// may write on anything, so reading visibility out of the fields map
    /// would let a caller change what somebody else can see by writing a key.
    /// This is set by whatever projects the document and a caller never
    /// constructs one.
    pub owner: Option<crate::memory::EntityId>,
}

/// The subjects in `doc`'s table that name **no known entity** — the split-brain
/// tell, deduped and in first-seen order.
///
/// A row is legitimately homed in its doc and legitimately about another entity;
/// what is never legitimate is a subject cell naming something that does not
/// exist. That is a hand edit gone wrong, and it stays invisible otherwise:
/// the row stays reachable through its home (so nothing breaks) while
/// quietly projecting onto a handle no other read would ever agree on.
///
/// Counting is all this does. **A scan must never hard-fail on one, and must
/// never drop it** — the row is a fact somebody wrote. Surfacing the quarantine
/// to the caller is later work; being able to see it in a log is the floor.
pub fn orphan_subjects(doc: &DocScan, known: &HashSet<EntityId>) -> Vec<EntityId> {
    let mut seen = HashSet::new();
    doc.facts
        .iter()
        .map(|f| &f.subject)
        .filter(|s| !known.contains(*s))
        .filter(|s| seen.insert((*s).clone()))
        .cloned()
        .collect()
}

/// The subjects in `doc`'s table that name **a different entity that exists** —
/// the doc's declared id and its own rows disagreeing, deduped and in first-seen
/// order.
///
/// This is the split-brain tell in its commoner disguise. [`orphan_subjects`]
/// only fires when a subject names *nothing*; a hand edit that retypes the cell
/// into another live handle leaves every read working — the row answers to one
/// id and lives under another, and the entity ends up readable under one and
/// writable under the other. Unlike an orphan, this **can be perfectly
/// legitimate**: a fact about one entity is often written on another's page. So
/// it is a signal, not a fault — counted, said out loud, and nothing more.
///
/// A doc that declares no entity has no id for its rows to disagree with.
pub fn foreign_subjects(doc: &DocScan, known: &HashSet<EntityId>) -> Vec<EntityId> {
    let Some(home) = doc.entity.as_ref().map(|e| &e.id) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    doc.facts
        .iter()
        .map(|f| &f.subject)
        .filter(|s| *s != home && known.contains(*s))
        .filter(|s| seen.insert((*s).clone()))
        .cloned()
        .collect()
}

/// Every entity a scan declares — the set [`orphan_subjects`] checks against.
pub fn known_entities(scan: &[DocScan]) -> HashSet<EntityId> {
    scan.iter()
        .filter_map(|d| d.entity.as_ref().map(|e| e.id.clone()))
        .collect()
}

/// Match facts by the edge they draw. `shape: None` means **any** shape pointing
/// at this object — "what's connected to X".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeFilter {
    /// Narrow to one shape, or `None` for any.
    pub shape: Option<EdgeShape>,
    /// The entity the edge must point at.
    pub object: EntityId,
}

/// The default number of results — raisable by the caller. There is **no
/// pagination and no cursor**: a second page is a better query.
pub const DEFAULT_LIMIT: usize = 20;

/// What to search for. `text` is optional as long as a structural filter narrows
/// the field, because the structural questions ("every superseded fact", "who is
/// in Shelbyville") have no keyword.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    /// Free text, matched over entity handles/names, fact content/details, and
    /// prose. All terms must match.
    pub text: Option<String>,
    /// Narrow to one entity kind: an entity's own kind, a fact's subject's kind,
    /// or the kind of the entity whose doc the prose sits in.
    pub kind: Option<EntityKind>,
    /// Narrow to one lifecycle state. **`None` means active only** — a
    /// superseded fact is excluded unless asked for by name, because a claim the
    /// store has already moved past coming back as current truth is worse than
    /// no memory at all.
    pub status: Option<FactStatus>,
    /// Narrow to testimony or inference.
    pub provenance: Option<Provenance>,
    /// **Narrow to settled or open** — the other axis, and the question
    /// *which of these am I not sure about*.
    ///
    /// `provenance` says who backs a claim and this says how sure anybody is.
    /// A store full of hedged claims that could not be asked for them made the
    /// axis a thing a reader could see one claim at a time and never gather.
    pub standing: Option<Standing>,
    /// Facts about one entity.
    pub subject: Option<EntityId>,
    /// Facts drawing a matching edge.
    pub edge: Option<EdgeFilter>,
    /// **THINGS that answer a type, matched STRUCTURALLY.**
    ///
    /// It carries the declaration itself rather than a name, because nothing
    /// below this point needs a store: a thing answers a type by the keys its
    /// records carry, so the keys are the whole of what a search needs.
    /// Resolving a name to a declaration happens once, at the surface, where a
    /// name that names no type can be answered as a name that names no type.
    ///
    /// **The question is asked of the thing, not of one row.** What a thing is
    /// gets written down a piece at a time, so its fields are the newest write
    /// of each key on it ([`super::folded_fields`]) and a thing described over
    /// two sittings answers a type that neither sitting answers alone.
    ///
    /// **Nothing is ever asked what it was declared to be.** A thing carrying
    /// these keys comes back whether or not anybody declared anything, and one
    /// carrying some of them comes back too, saying which it lacks — a filter
    /// that kept only complete matches would hide exactly the things worth
    /// finding.
    pub answers_type: Option<types::DeclaredType>,
    /// **Only the things that FIT this type** — the ones carrying every key it
    /// names, counted across everything recorded about each.
    ///
    /// The strict half of the same question, and which of the two a reader is
    /// asking is the reader's choice: `answers_type` finds the things described
    /// like one of these and says what each is missing, and this keeps only the
    /// ones with nothing missing. *Which of these are described like a service*
    /// against *which of these ARE services*.
    ///
    /// **The tolerant one is the default wherever neither is named**, because a
    /// thing arriving with its gaps named can neither hide nor overclaim, while
    /// a thing missing from an answer reads exactly like a thing that is not
    /// there (rule 62).
    ///
    /// Structural, like everything else here: a thing carrying the keys fits
    /// whether or not anybody declared it anything.
    pub fits_type: Option<types::DeclaredType>,
    /// Whether messages are in the answer. **False by default, and worth
    /// setting true.**
    ///
    /// Mail in the one ranked list is how a session finds a finding it did not
    /// know to go looking for — a report another session filed is exactly the
    /// context nobody would think to ask for — so this is a flag to reach for
    /// rather than an obscure corner.
    ///
    /// It is still off unless asked for, because a mail hit carries somebody's
    /// box, sender and a snippet of their message, and this is the verb the
    /// surface tells a session to reach for FIRST. A session told to leave a
    /// box alone must be able to use the front door without knowing to switch
    /// something off: the safe branch is the default, never the documented
    /// preference (rule 62).
    pub include_mail: bool,
    /// **Whether matching also reaches a claim's earlier wordings.** False by
    /// default: an ordinary search never surfaces content that lived only in
    /// a superseded write, which is the corpus note's own claim and stays
    /// true unless a caller asks for otherwise.
    ///
    /// A claim that matches through this reaches the surface as the same
    /// [`Hit::Fact`] any other match does — the current record, address and
    /// all, never a second thing standing in its own right. Reading what it
    /// used to say is `recall` with `history_record`, unchanged.
    pub include_history: bool,
    /// **Who is asking, when anybody is** — the bot the caller booted as.
    ///
    /// It is what scopes session hits: a bot finds its own runs and nobody
    /// else's. **Absent means no session hit comes back at all**, rather than
    /// all of them: a caller with no identity is not everybody, and the safe
    /// branch is the default here for the same reason it is on `include_mail`.
    ///
    /// **Owner-scoping is what replaced keeping sessions out of the index.**
    /// What makes a run safe to index is who may read it, not whether it is
    /// there — and the exclusion cost a bot access to its own work while
    /// buying nothing this does not.
    ///
    /// It scopes nothing else. Entities, facts and prose are the operator's
    /// and every bot may see them; mail has its own rule and its own flag.
    pub asked_by: Option<EntityId>,
    /// How many results to return.
    pub limit: usize,
}

impl Default for SearchQuery {
    fn default() -> Self {
        SearchQuery {
            text: None,
            kind: None,
            status: None,
            provenance: None,
            standing: None,
            subject: None,
            edge: None,
            answers_type: None,
            fits_type: None,
            asked_by: None,
            include_mail: false,
            include_history: false,
            limit: DEFAULT_LIMIT,
        }
    }
}

impl SearchQuery {
    /// A query for free text, everything else defaulted.
    pub fn text(text: impl Into<String>) -> Self {
        SearchQuery {
            text: Some(text.into()),
            ..Default::default()
        }
    }

    /// The trimmed text, or `None` if there is none worth matching.
    pub fn terms(&self) -> Option<&str> {
        self.text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
    }

    /// Is this query scoped to **facts alone**? `status`, `provenance`,
    /// `subject` and `edge` are properties only a fact has, so naming one is a
    /// statement that entities and prose are not what the caller is looking
    /// for.
    ///
    /// The *default* status (active only) does not count — a default must not
    /// silently narrow a search to one hit type.
    ///
    /// **`answers_type` is not one of them any more.** A type is answered by a
    /// THING, over the keys its records carry between them, so naming one is a
    /// question about things — see [`SearchQuery::is_thing_scoped`].
    pub fn is_fact_scoped(&self) -> bool {
        self.status.is_some()
            || self.provenance.is_some()
            || self.standing.is_some()
            || self.subject.is_some()
            || self.edge.is_some()
    }

    /// Is this query scoped to **things**? Naming a type asks which things
    /// carry its keys, so rows, prose and messages are not what the caller is
    /// after: none of them is a thing, and none of them has fields to answer
    /// with.
    pub fn is_thing_scoped(&self) -> bool {
        self.answers_type.is_some() || self.fits_type.is_some()
    }

    /// The type this query narrows by, whichever question it asked — and
    /// whether only whole matches survive it.
    ///
    /// **One place decides which is which.** The two filters select the same
    /// way and differ only in what they keep, so a reader that asked each of
    /// them separately would be two readers to keep in step.
    pub fn typed(&self) -> Option<(&types::DeclaredType, bool)> {
        match (&self.answers_type, &self.fits_type) {
            (_, Some(strict)) => Some((strict, true)),
            (Some(tolerant), None) => Some((tolerant, false)),
            (None, None) => None,
        }
    }

    /// Reject a query that cannot be served, before any index work: no text and
    /// no filter is a request for "everything", which is not a search; and the
    /// entity references it carries must be well-formed ids.
    pub fn validate(&self) -> Result<(), MemoryError> {
        if self.terms().is_none()
            && self.kind.is_none()
            && !self.is_fact_scoped()
            && !self.is_thing_scoped()
        {
            return Err(MemoryError::InvalidQuery(
                "give a query, or at least one filter (kind, status, provenance, subject, edge, \
                 answers_type, fits_type)"
                    .into(),
            ));
        }
        if self.limit == 0 {
            return Err(MemoryError::InvalidQuery("limit must be at least 1".into()));
        }
        if let Some(subject) = &self.subject {
            validate_subject(subject)?;
        }
        // The same validator the declaring path uses. A query carrying a
        // declaration no store would have kept is the caller's mistake, and it
        // must read as one rather than as an honest empty answer.
        if let Some(declared) = &self.answers_type {
            types::validate_type(declared)?;
        }
        if let Some(declared) = &self.fits_type {
            types::validate_type(declared)?;
        }
        // **Two questions about one type, and a call names one of them.** They
        // keep different things, so a query carrying both is refused rather
        // than one of them being picked for the caller.
        if self.answers_type.is_some() && self.fits_type.is_some() {
            return Err(MemoryError::InvalidQuery(
                "ask answers_type or fits_type, not both: one keeps the things carrying some of a \
                 type's keys and says what each lacks, and the other keeps only the things \
                 carrying every key"
                    .into(),
            ));
        }
        // The same rule the write path applies, reused rather than restated: a
        // filter combination no write could ever produce must read as the
        // caller's mistake, not as an honest empty answer.
        match &self.edge {
            Some(EdgeFilter {
                shape: Some(shape),
                object,
            }) => validate_edge(&Edge::new(*shape, object.clone()))?,
            Some(EdgeFilter {
                shape: None,
                object,
            }) => validate_subject(object)?,
            None => {}
        }
        Ok(())
    }
}

/// A handle, resolved as far as the index can resolve it — **the cure for a bare
/// hit**. A result that says only `person:homer-simpson` makes the reader spend a second
/// call to learn whether that is Homer Simpson, and a third to learn he is also Cosme Fulanito.
///
/// `name` is `None` when the handle resolves to no entity the index holds. That
/// is the orphan case ([`orphan_subjects`]) and it is left visibly empty rather
/// than filled with the handle: a missing name is a fact about the store, and
/// papering over it is how the split brain stayed invisible in the first place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRef {
    /// The handle itself — always present, always what an edit or a follow-up
    /// query takes.
    pub id: EntityId,
    /// The kind the handle declares. `None` only for an id whose grammar is
    /// broken, which a tolerant reader can still hand back.
    pub kind: Option<EntityKind>,
    /// The display name, when the handle names an entity the index knows.
    pub name: Option<String>,
    /// The other names it answers to. Empty is the ordinary case — and also
    /// what an unresolved handle carries, because there are none to report,
    /// not because they are unknown. That is why this is a list where `name` is
    /// an option: absent and none-at-all are the same answer here.
    pub aliases: Vec<String>,
}

impl EntityRef {
    /// A handle nobody has resolved: the kind its grammar declares, no name.
    pub fn unresolved(id: EntityId) -> Self {
        EntityRef {
            kind: id.kind(),
            id,
            name: None,
            aliases: Vec::new(),
        }
    }

    /// A handle resolved against the entity it names.
    ///
    /// The aliases, **not** [`Entity::labels`]: labels lead with the display
    /// name, which is already `name` here, and repeating it would make one
    /// label read as two.
    pub fn resolved(entity: &Entity) -> Self {
        EntityRef {
            id: entity.id.clone(),
            kind: Some(entity.kind),
            name: Some(entity.name.clone()),
            aliases: entity.aliases.clone(),
        }
    }
}

/// **What became of the claim a derivation was worked out from.**
///
/// A derivation is a reading of another claim, and a reading stops being good
/// when what it read is archived. Nothing on a hit said so, so a gloss went on
/// being served as an answer long after the claim under it stopped being
/// current.
///
/// **Three states, because "the source stands" and "jojobot cannot see the
/// source" are different claims** and a reader acts on each of them
/// differently. It is the distinction [`Coverage`] draws over a whole half of
/// the corpus, drawn here over one link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStanding {
    /// The source is in the index, and still current.
    Stands,
    /// **The source stopped being current after this was worked out from
    /// it** — replaced by a later claim, or taken back outright.
    ///
    /// ⛔️ Not [`Stands`](Self::Stands): the source did not stand, whichever
    /// of the two it was, which is the case this marker exists for. One
    /// status covers both on the source's own row ([`FactStatus::Archived`]),
    /// so a reader here gets the same one answer: *your source is no longer
    /// current, and a replacement, if there is one, is an ordinary claim
    /// naming this one as [`Fact::derived_from`]*.
    Archived,
    /// **The index does not hold the source, so nothing can be said about it.**
    ///
    /// ⛔️ Never collapse this into [`Stands`](Self::Stands). A reader told a
    /// citation is good on the strength of a record nobody read is being
    /// vouched for by silence, which is the one thing this marker exists to
    /// stop.
    Unreadable,
}

impl SourceStanding {
    /// The wire token this standing is written as.
    pub fn as_token(self) -> &'static str {
        match self {
            SourceStanding::Stands => "stands",
            SourceStanding::Archived => "archived",
            SourceStanding::Unreadable => "unreadable",
        }
    }
}

/// One result. **Typed, and in one list with the others** — the caller is told
/// what each hit is rather than having to guess from its shape.
///
/// Every variant arrives **with its surroundings**: an answer that is only a row
/// leaves the reader to go and find out what it is attached to, and the reader
/// is an assistant that will simply not bother. So a fact names the entities it
/// is about and sits on, and an entity carries the edges its facts draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    /// An entity matched by handle or name.
    Entity {
        /// The entity itself.
        entity: Entity,
        /// The doc that carries it.
        doc_id: String,
        /// The edges its facts draw — where this entity sits in the graph,
        /// deduped and in first-seen order.
        edges: Vec<Edge>,
        /// **How this THING answers the type the query named**, when it named
        /// one: the keys its records carry between them, the keys they lack by
        /// name, and any whose value is not what the type said it holds.
        ///
        /// `None` when the query named no type. It is never `None` for a hit a
        /// type query returned — a thing that answers none of a type's keys is
        /// not a match at all, so it is not in the answer to be reported on.
        ///
        /// **Boxed** so that the one variant carrying it does not set the size
        /// of every hit in a result list.
        answers: Option<Box<types::Match>>,
    },
    /// A fact matched by content, details, or a filter. Carried **whole** — the
    /// row, not a snippet — because the answer usually IS the row, and its
    /// address is what an edit needs.
    Fact {
        /// The fact, address and all.
        ///
        /// **Boxed** for the reason [`Hit::Entity::answers`] is: a row is by
        /// far the biggest thing any variant carries, and unboxed it sets the
        /// size of every hit in a result list — most of which are not rows.
        fact: Box<Fact>,
        /// The entity the fact is about, resolved.
        subject: EntityRef,
        /// The entity whose doc holds the row, resolved. Usually the same as
        /// `subject`; when it is not, that difference is the thing worth seeing.
        home: EntityRef,
        /// **What became of the claim this one was worked out from.**
        ///
        /// `None` when this claim was worked out from nothing — the ordinary
        /// case, and an absence rather than an unknown.
        source: Option<SourceStanding>,
    },
    /// A message matched in a mailbox — **unmistakably mail**, and carrying the
    /// whole envelope: which box, what state it is in, who sent it, and the id
    /// `read_message` takes. Without those, a mail hit reads as an anonymous
    /// paragraph and a reader cannot tell live work from an archived report.
    ///
    /// The body arrives as a snippet, not whole: a message is often pages, and
    /// its id is right there for taking delivery of the rest.
    Message {
        /// The message, envelope and all. Its `body` is the whole stored text;
        /// what a caller is shown around the match is `snippet`.
        message: Message,
        /// The matching text with enough around it to read.
        snippet: String,
    },
    /// **A beat from a session's chronology, matched by its text** — a bot's
    /// own working history, reachable the way everything else is.
    ///
    /// **Owner-scoped, and that is what makes it safe to index at all.** A
    /// session is one bot's account of its own run; another bot's is not
    /// theirs to read. The rule lives on who may see a hit rather than on
    /// whether the record is in the index, because keeping it out cost a bot
    /// access to its own work and bought nothing the scoping does not.
    ///
    /// **It ranks below every other kind.** A session is context rather than
    /// an answer: reachable when it is what you are looking for, never
    /// crowding out what a search is usually for.
    Session {
        /// Which run it belongs to, so a caller can resume or read on.
        session: SessionId,
        /// The bot whose run it is — the owner the scoping is measured
        /// against, and never somebody else's.
        bot: EntityId,
        /// What the session says it is working on, when it still has one:
        /// the line that tells two runs apart.
        working_on: Option<String>,
        /// The matching text with enough around it to read.
        snippet: String,
    },
    /// Human prose matched inside a document body.
    Prose {
        /// The doc that carries it.
        doc_id: String,
        /// The doc's title.
        title: String,
        /// The entity whose doc this is, when it is an entity doc at all —
        /// whole, not a bare handle.
        entity: Option<Entity>,
        /// The edges that entity's facts draw; empty for a doc that is nobody's.
        edges: Vec<Edge>,
        /// The matching text with enough around it to read.
        snippet: String,
    },
}

/// How much of one half of the corpus the projection actually holds — **the
/// honesty half of degrade-don't-error**, and three states rather than two
/// because the middle one is reachable and was being reported as one of the
/// others.
///
/// A search is a read of an in-process index, so a store that was unreachable
/// when the index was built, or that could not be re-read after a write, does
/// not make searching fail; it makes material missing. "Nothing says that" and
/// "jojobot has not read it" are different claims, and a caller acts on both.
///
/// **One vocabulary for both halves.** Memory and mail are two stores behind
/// one index and each can be behind on its own, so each reports its own
/// coverage — in the same three words, because a caller reading an answer
/// should not have to learn two of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// Nothing of this half is searchable: it was never read and nothing has
    /// been written through this process since. An empty answer means nothing.
    Unread,
    /// Some of it is searchable and some is known to be missing. Hits are real
    /// and findable; what is absent may exist anyway, and a caller has to be
    /// told that rather than shown an empty list.
    ///
    /// **It carries WHY, because the two ways in are not the same claim.** One
    /// leaves almost nothing searchable and the other leaves almost everything
    /// searchable, and a caller decides whether to trust an empty answer on
    /// exactly that difference. A single word for both told the caller in the
    /// worse state that it was in the milder one. Never collapse it into
    /// [`Unread`](Self::Unread) either: that would answer with hits while
    /// saying nothing was searched.
    Partial(Behind),
    /// It was read and nothing is known to be behind: everything in it is
    /// searchable.
    Loaded,
}

/// **Why a half of the index is behind the store it mirrors.**
///
/// The two are ordered by how much they take away, and the wider one wins when
/// both hold: a store that was never read is missing nearly everything, which
/// is the state a caller has to hear about first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behind {
    /// No read has ever filled this half, so only what this process has written
    /// since is in it. Almost nothing is searchable.
    ///
    /// **Not startup-only and not permanent.** The boot read is the first
    /// chance, never the only one: a search refreshes each half, so the first
    /// read that reaches the store fills it and this state ends.
    Unscanned,
    /// This half was read whole and the index is holding an older version of it
    /// than the store. Almost everything is searchable.
    ///
    /// **Two routes in, and the halves do not share them.** A whole-half refresh
    /// that could not reach the store reaches this state on either half. A
    /// committed write whose re-read could not run reaches it on the memory half
    /// only — the mail path indexes the record the store hands back rather than
    /// re-reading it, so it has no write-path route and none is invented.
    Stale,
}

impl Behind {
    /// The wire spelling, so a caller branches on a token rather than on a
    /// sentence — the same deal `searched` already makes.
    pub fn as_token(self) -> &'static str {
        match self {
            Behind::Unscanned => "unscanned",
            Behind::Stale => "stale",
        }
    }
}

/// The retrieval port: one ranked, mixed list.
///
/// **Asynchronous, because an answer is only as good as its last look at the
/// store.** The projection behind this port is a copy, and a copy that is never
/// re-read serves whatever it last saw — including a record the store has since
/// lost, which nothing inside the process can observe. So a search takes a
/// scan; it is I/O, and the port says so.
#[async_trait::async_trait]
pub trait Search: Send + Sync {
    /// Search entities, facts, prose and messages at once. Ordering is the
    /// ranking: text relevance, boosted by recency, with an entity whose handle
    /// or name the query matches pinned to the top.
    ///
    /// **The answer is backed by a scan taken for it.** When that scan reaches
    /// the store the projection is replaced from it, so a record the store no
    /// longer has stops being served here. When it does not, the answer comes
    /// from the last scan that did and [`memory_coverage`](Self::memory_coverage)
    /// stops reporting [`Loaded`](Coverage::Loaded) — a search does not fail
    /// because a refresh could not run.
    async fn search(&self, query: &SearchQuery) -> Result<Vec<Hit>, MemoryError>;

    /// How much of the mail board this projection holds — see [`Coverage`].
    /// Memory results come back whatever it says; this is what lets an answer
    /// tell a caller which kind of silence they are looking at.
    fn mail_coverage(&self) -> Coverage;

    /// How much of the memory graph this projection holds — the same question
    /// as [`mail_coverage`](Self::mail_coverage), asked of the other store.
    ///
    /// It is not always [`Loaded`](Coverage::Loaded), and that is the point: a
    /// boot scan that failed, or a document whose refresh after a write could
    /// not run, leaves this half serving the version it last read. Without this
    /// the caller cannot tell that answer from a complete one.
    fn memory_coverage(&self) -> Coverage;
}

#[cfg(test)]
mod tests {
    use super::super::Standing;
    use super::*;

    #[test]
    fn a_query_with_neither_text_nor_a_filter_is_refused() {
        let empty = SearchQuery::default();
        assert!(matches!(
            empty.validate(),
            Err(MemoryError::InvalidQuery(_))
        ));
        // Whitespace is not a query either.
        assert!(SearchQuery::text("   ").validate().is_err());
    }

    /// A structural filter is enough on its own: "every archived fact" and
    /// "which people are in Shelbyville" carry no keyword.
    #[test]
    fn a_structural_filter_alone_is_a_valid_query() {
        // **This case runs in a booted process.** It parses a handle and asserts
        // about something else, so the set is setup — and setup comes from
        // standing a store up, filled from what that store holds.
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let archived = SearchQuery {
            status: Some(FactStatus::Archived),
            ..Default::default()
        };
        assert!(archived.validate().is_ok());
        let edged = SearchQuery {
            kind: Some(EntityKind::PERSON),
            edge: Some(EdgeFilter {
                shape: Some(EdgeShape::Location),
                object: EntityId("place:far-country".into()),
            }),
            ..Default::default()
        };
        assert!(edged.validate().is_ok());
    }

    /// The fact-only filters say "facts, please"; `kind` and free text do not.
    #[test]
    fn only_the_fact_properties_scope_a_query_to_facts() {
        assert!(!SearchQuery::text("shelbyville").is_fact_scoped());
        assert!(
            !SearchQuery {
                kind: Some(EntityKind::PERSON),
                ..Default::default()
            }
            .is_fact_scoped(),
            "a kind filter applies to entities and prose too"
        );
        for scoped in [
            SearchQuery {
                status: Some(FactStatus::Archived),
                ..Default::default()
            },
            SearchQuery {
                provenance: Some(Provenance::Testimony),
                ..Default::default()
            },
            SearchQuery {
                subject: Some(EntityId::person("person:alpha")),
                ..Default::default()
            },
            SearchQuery {
                edge: Some(EdgeFilter {
                    shape: None,
                    object: EntityId("place:x".into()),
                }),
                ..Default::default()
            },
        ] {
            assert!(
                scoped.is_fact_scoped(),
                "{scoped:?} names a fact-only property"
            );
        }
    }

    /// A malformed entity reference in a filter is refused, exactly as it is on a
    /// write: an id is structured, never free text.
    #[test]
    fn a_malformed_reference_in_a_filter_is_refused() {
        let bad_subject = SearchQuery {
            subject: Some(EntityId("not-an-id".into())),
            ..Default::default()
        };
        assert!(matches!(
            bad_subject.validate(),
            Err(MemoryError::InvalidSubject(_))
        ));
        let bad_object = SearchQuery {
            edge: Some(EdgeFilter {
                shape: None,
                object: EntityId("place:a|b".into()),
            }),
            ..Default::default()
        };
        assert!(matches!(
            bad_object.validate(),
            Err(MemoryError::InvalidSubject(_))
        ));
    }

    /// The shape→kind rule binds the **read** path as hard as the write path.
    /// `{shape: location, object: person:x}` is a combination no write can
    /// produce, so serving it returns zero hits — and zero hits reads as "nobody
    /// is there", not "you asked something impossible". The caller's mis-drawn
    /// filter has to come back as their mistake.
    #[test]
    fn an_edge_filter_whose_object_is_wrong_for_its_shape_is_refused() {
        // **The set is this case's subject, not its setup.** The object's kind
        // is what makes the filter wrong, and the set answers it.
        crate::memory::kinds::load_shipped();
        let impossible = SearchQuery {
            edge: Some(EdgeFilter {
                shape: Some(EdgeShape::Location),
                object: EntityId::person("person:alpha"),
            }),
            ..Default::default()
        };
        assert!(
            matches!(impossible.validate(), Err(MemoryError::InvalidEdge(_))),
            "a location edge points at a place, on the query path too"
        );
        // A shapeless filter is "what's connected to X" — every kind is fair game.
        let any_shape = SearchQuery {
            edge: Some(EdgeFilter {
                shape: None,
                object: EntityId::person("person:alpha"),
            }),
            ..Default::default()
        };
        assert!(any_shape.validate().is_ok());
        // …and `about` is the open shape, so it accepts a person too.
        let open = SearchQuery {
            edge: Some(EdgeFilter {
                shape: Some(EdgeShape::About),
                object: EntityId::person("person:alpha"),
            }),
            ..Default::default()
        };
        assert!(open.validate().is_ok());
    }

    /// A subject naming an entity nobody declares is counted; one naming a real
    /// entity — this doc's own, or another doc's — is not. The row is never
    /// dropped and never fails the scan: it is a fact somebody wrote, and the
    /// point is only that it stops being invisible.
    #[test]
    fn a_subject_that_names_no_known_entity_is_counted_once() {
        // **This case runs in a booted process.** It parses a handle and asserts
        // about something else, so the set is setup — and setup comes from
        // standing a store up, filled from what that store holds.
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        use crate::memory::{Boot, FactId};
        use jiff::civil::date;

        let entity = |id: &str| Entity {
            id: EntityId(id.into()),
            kind: EntityId(id.into())
                .kind()
                .expect("test ids are well-formed"),
            name: String::new(),
            aliases: Vec::new(),
            source: "test".into(),
            crm: None,
            parent: None,
            boot: Boot::OnDemand,
            merged_into: None,
            badge: None,
            archived: None,
        };
        let row = |id: &str, subject: &str| Fact {
            id: FactId(id.into()),
            home: EntityId::person("person:alpha"),
            subject: EntityId(subject.into()),
            content: "a claim".into(),
            details: None,
            provenance: Provenance::Inference,
            standing: Standing::Open,
            status: FactStatus::Active,
            recorded_at: date(2026, 7, 1),
            happened_at: None,
            happened_through: None,
            edge: None,
            fields: Default::default(),
            refs: Vec::new(),
            derived_from: None,
            stands_for: Vec::new(),
            inserted_at: None,
            stale_after: None,
        };

        let doc = DocScan {
            doc_id: "doc-1".into(),
            title: "Alpha".into(),
            prose: String::new(),
            entity: Some(entity("person:alpha")),
            facts: vec![
                row("f1", "person:alpha"),  // its own entity
                row("f2", "person:beta"),   // another doc's entity, legitimately
                row("f3", "person:alphaa"), // names nothing — the hand-edit tell
                row("f4", "person:alphaa"), // …twice, reported once
            ],
            fields: Default::default(),
            owner: None,
        };
        let known: HashSet<EntityId> = [
            EntityId::person("person:alpha"),
            EntityId::person("person:beta"),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            orphan_subjects(&doc, &known),
            vec![EntityId::person("person:alphaa")],
            "only the subject naming no entity, and only once"
        );
        assert_eq!(
            known_entities(std::slice::from_ref(&doc)),
            [EntityId::person("person:alpha")]
                .into_iter()
                .collect::<HashSet<_>>(),
            "a scan's known set is the entities its docs declare"
        );
    }

    /// The **other** half of the split-brain tell, and the one the Cosme incident
    /// actually wore: a row whose subject names a real entity that is not the doc
    /// it sits in. A hand edit retyped the subject cell into another live handle,
    /// so nothing was orphaned — the row simply answered to one id and lived under
    /// another, and the orphan counter (which only fires on a subject naming
    /// *nothing*) had nothing to say about it.
    #[test]
    fn a_subject_naming_another_existing_entity_is_counted_apart_from_an_orphan() {
        // **This case runs in a booted process.** It parses a handle and asserts
        // about something else, so the set is setup — and setup comes from
        // standing a store up, filled from what that store holds.
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        use crate::memory::{Boot, FactId};
        use jiff::civil::date;

        let entity = |id: &str| Entity {
            id: EntityId(id.into()),
            kind: EntityId(id.into())
                .kind()
                .expect("test ids are well-formed"),
            name: String::new(),
            aliases: Vec::new(),
            source: "test".into(),
            crm: None,
            parent: None,
            boot: Boot::OnDemand,
            merged_into: None,
            badge: None,
            archived: None,
        };
        let row = |id: &str, subject: &str| Fact {
            id: FactId(id.into()),
            home: EntityId::person("person:alpha"),
            subject: EntityId(subject.into()),
            content: "a claim".into(),
            details: None,
            provenance: Provenance::Inference,
            standing: Standing::Open,
            status: FactStatus::Active,
            recorded_at: date(2026, 7, 1),
            happened_at: None,
            happened_through: None,
            edge: None,
            fields: Default::default(),
            refs: Vec::new(),
            derived_from: None,
            stands_for: Vec::new(),
            inserted_at: None,
            stale_after: None,
        };
        let doc = DocScan {
            doc_id: "doc-1".into(),
            title: "Alpha".into(),
            prose: String::new(),
            entity: Some(entity("person:alpha")),
            facts: vec![
                row("f1", "person:alpha"),  // its own entity
                row("f2", "person:beta"),   // a different entity that exists
                row("f3", "person:beta"),   // …twice, reported once
                row("f4", "person:alphaa"), // names nothing: an orphan, not this
            ],
            fields: Default::default(),
            owner: None,
        };
        let known: HashSet<EntityId> = [
            EntityId::person("person:alpha"),
            EntityId::person("person:beta"),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            foreign_subjects(&doc, &known),
            vec![EntityId::person("person:beta")],
            "only the subject naming another live entity, and only once"
        );
        assert_eq!(
            orphan_subjects(&doc, &known),
            vec![EntityId::person("person:alphaa")],
            "the two counters must not swallow each other's case"
        );

        // A doc that declares no entity has no id to disagree with.
        let loose = DocScan {
            entity: None,
            ..doc
        };
        assert!(foreign_subjects(&loose, &known).is_empty());
    }

    #[test]
    fn a_zero_limit_is_refused() {
        let query = SearchQuery {
            limit: 0,
            ..SearchQuery::text("shelbyville")
        };
        assert!(matches!(
            query.validate(),
            Err(MemoryError::InvalidQuery(_))
        ));
    }
}
