//! `search` — The front door — one ranked list over entities, facts, prose and mail.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// The `edge` filter of a `search` — a shape and the entity it points at.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EdgeFilterArgs {
    /// Narrow to one shape (`location` · `membership` · `attendance` · `about` ·
    /// `connection`).
    /// Omit for **any** edge pointing at `object` — "what's connected to X".
    #[serde(default)]
    pub(crate) shape: Option<String>,
    /// The entity the edge must point at, as `kind:slug`.
    pub(crate) object: String,
}

/// Arguments to `search`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    /// Free text over entity handles/names, fact claims and details, and an
    /// entity's prose. **All words must match.** Optional when at least one
    /// filter below is given.
    #[serde(default)]
    pub(crate) query: Option<String>,
    /// Narrow to one entity kind — an entity's own kind, a fact's subject's kind,
    /// or the kind of the entity whose prose matched.
    #[serde(default)]
    pub(crate) kind: Option<String>,
    /// `active` (the default) or `superseded`. A superseded fact is **excluded
    /// unless asked for by name** — a claim already moved past must not come
    /// back as current truth.
    #[serde(default)]
    pub(crate) status: Option<String>,
    /// `testimony`, `observation` or `inference` — keep only the claims backed
    /// that way. `observation` is a claim an AI read out of a system of record,
    /// and it names the system it was read from.
    #[serde(default)]
    pub(crate) provenance: Option<String>,
    /// `settled` or `open` — **the other certainty axis, and the one that
    /// answers *which of these am I not sure about***. `provenance` says who
    /// backs a claim; this says how sure anybody is of it. A hedge the operator
    /// gave you, and an inference nobody confirmed, are both `open`.
    #[serde(default)]
    pub(crate) standing: Option<String>,
    /// Facts about this entity, as `kind:slug`.
    #[serde(default)]
    pub(crate) subject: Option<String>,
    /// Facts drawing a matching edge. With `kind`, this is how a cross-entity
    /// question ("which people are in X") is answered in one call.
    #[serde(default)]
    pub(crate) edge: Option<EdgeFilterArgs>,
    /// **THINGS that answer this type, by name.** Matching is STRUCTURAL: a
    /// thing carrying the type's keys comes back whether or not anybody
    /// declared it to be one, so this finds things nobody filed under it. It is
    /// asked of the thing rather than of one of its records — what a thing is
    /// gets written down a piece at a time, and every write on it counts.
    ///
    /// A thing carrying only some of the keys comes back too, saying which it
    /// lacks — partial matches are the ones usually worth finding, so they are
    /// reported and never filtered out. Each hit carries an `answers_type`
    /// saying which keys it holds, which it lacks by name, and any whose value
    /// is not what the type said it holds.
    ///
    /// ⚠️ **Some of the keys is enough here.** If you want only the things
    /// with no gaps — *which of these ARE services*, rather than *which are
    /// described like one, and what is missing* — that is `fits_type`, below.
    ///
    /// A name no type answers to comes back blocked, naming the types that do
    /// exist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) answers_type: Option<String>,
    /// **Only the things that FIT this type**, by name — the ones carrying
    /// EVERY key it names, counted over every write on each.
    ///
    /// The strict half of the same question, and which one you are asking is
    /// yours to choose: `answers_type` asks *which of these are described like
    /// a service, and what is missing*, and this asks *which of these ARE
    /// services*.
    ///
    /// Reach for `answers_type` unless you have a reason not to. A thing that
    /// arrives with its gaps named can be judged; a thing this filter leaves
    /// out looks exactly like a thing that is not there.
    ///
    /// Pass one or the other, never both. A name no type answers to comes back
    /// blocked, naming the types that do exist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fits_type: Option<String>,
    /// Whether messages left in mailboxes are searched too. **Defaults to
    /// false, and worth passing true** when you are looking for what a session
    /// knows: a report filed for another session is exactly the context you
    /// would not know to go looking for, and this is the one call that finds
    /// it. It is off unless you ask, because a message hit carries somebody's
    /// box, sender and a snippet, and this verb is the one to reach for first.
    #[serde(default)]
    pub(crate) include_mail: Option<bool>,
    /// How many results; defaults to 20. There is no pagination — a second page
    /// is a better query.
    #[serde(default)]
    pub(crate) limit: Option<u32>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

/// One search result on the wire. **Every hit says what it is** (`hit`), so a
/// caller reads a mixed list without guessing from its shape — and each kind of
/// hit carries what makes it actionable: an entity its handle, a fact its whole
/// row and address, prose its title, the entity that owns it and the text
/// around the match.
///
/// **And every hit arrives with its surroundings.** A fact adds `about` and
/// `home` — its subject and its home entity, resolved to every name they
/// answer to — and an
/// entity or a stretch of prose adds `edges`, where it sits in the graph. The
/// enrichment is strictly additive: `subject` is still the same handle string
/// here as in `recall`, so one record has one spelling across every verb.
///
/// **What a hit does NOT carry is the id of the thing it was stored in.** A hit
/// carrying one is the only place on the whole surface where a caller could
/// learn that entities have documents at all, and it is not an address a caller
/// can use: every verb here is addressed by handle or by fact address.
/// Internally the id is what orders a hit list; that is the index's business
/// (`jojobot_adapters::search::tiebreak`) and it stops there.
fn hit_json(hit: &Hit, as_of: jiff::civil::Date) -> serde_json::Value {
    match hit {
        Hit::Entity {
            entity,
            edges,
            answers,
            ..
        } => {
            let mut body = entity_json(entity);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("hit".into(), "entity".into());
                obj.insert("edges".into(), edges.iter().map(edge_json).collect());
                // **How this THING answers the type that was asked for.**
                // Absent when no type was asked for, rather than rendered
                // empty: a caller who named no type is not being told this
                // thing answers nothing.
                if let Some(found) = answers {
                    obj.insert("answers_type".into(), answers_json(found));
                }
            }
            body
        }
        Hit::Fact {
            fact,
            subject,
            home,
            source,
        } => {
            let mut body = fact_json(fact, as_of);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("hit".into(), "fact".into());
                obj.insert("about".into(), entity_ref_json(subject));
                obj.insert("home".into(), entity_ref_json(home));
                // **What became of the claim under this one.** Absent — not
                // rendered empty — for a claim derived from nothing: there is
                // no source to report on, and a null here would read as a
                // source nobody could place.
                if let Some(standing) = source {
                    obj.insert("source_standing".into(), standing.as_token().into());
                }
            }
            body
        }
        // A mail hit is unmistakably mail: the whole envelope, so a reader can
        // tell live work from an archived report without a second call, and the
        // id that takes delivery of the rest. `body` is deliberately absent —
        // what is here is the snippet, and read_message is how the message is
        // taken whole.
        Hit::Message { message, snippet } => serde_json::json!({
            "hit": "message",
            "id": message.id.as_str(),
            "mailbox": message.mailbox.as_str(),
            "state": message.state.as_token(),
            "sender": message.sender,
            "subject": message.subject,
            "sent_at": message.sent_at.to_string(),
            "notes": message.notes,
            "snippet": snippet,
        }),
        // A beat from the caller's own run. It carries the run's handle rather
        // than the whole chronology: what a reader does next is resume it or
        // read on, and both take the handle.
        Hit::Session {
            session,
            bot,
            working_on,
            snippet,
        } => serde_json::json!({
            "hit": "session",
            "session": session.0,
            "bot": bot.to_string(),
            "working_on": working_on,
            "snippet": snippet,
        }),
        Hit::Prose {
            title,
            entity,
            edges,
            snippet,
            ..
        } => serde_json::json!({
            "hit": "prose",
            "title": title,
            "entity": entity.as_ref().map(entity_json),
            "edges": edges.iter().map(edge_json).collect::<Vec<_>>(),
            "snippet": snippet,
        }),
    }
}

/// **Why the QUERY left mail out** — the reasons that hold before the index is
/// consulted at all, so they answer ahead of [`Coverage`].
///
/// A type rather than three early returns carrying their prose inline. Every
/// sentence here is text an agent reads, so the surface's word check has to
/// gather all of them; a reason that is a variant is a reason it gathers by
/// construction, where a fourth `if` with a fresh string in it would be prose
/// nothing reads (rule 106).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MailExcluded {
    /// Mail was not asked for. **The default, so this is the ordinary case
    /// rather than a caller ruling something out** — which is why the note
    /// says how to ask rather than reporting a choice back.
    NotAsked,
    /// The query filters on a property only a fact has, so it is a question
    /// about facts and mail is not part of the answer it asks for.
    FactScoped,
    /// **A `kind` filter excludes mail, silently and structurally.** A message
    /// belongs to no entity, so it has no kind to match — the filter drops it
    /// exactly as it drops prose in a doc that is nobody's. Saying `searched:
    /// true` here was the field's one wrong answer, and a field a caller is
    /// told to trust has to be right in every case rather than in most of them.
    KindFiltered,
    /// **A type query asks about THINGS**, and a message is not one. It has no
    /// fields to answer a type with, so the same clause that selects the things
    /// that answer leaves mail out — structurally, exactly as a `kind` filter
    /// does.
    TypeFiltered,
}

impl MailExcluded {
    /// The reason this query leaves mail out, if one holds. First match wins:
    /// a query can be narrowed several ways at once and the answer names one.
    fn of(query: &SearchQuery) -> Option<Self> {
        if !query.include_mail {
            return Some(MailExcluded::NotAsked);
        }
        if query.is_fact_scoped() {
            return Some(MailExcluded::FactScoped);
        }
        if query.is_thing_scoped() {
            return Some(MailExcluded::TypeFiltered);
        }
        if query.kind.is_some() {
            return Some(MailExcluded::KindFiltered);
        }
        None
    }

    /// What the answer tells the caller, and how to get mail back.
    fn note(self) -> &'static str {
        match self {
            MailExcluded::NotAsked => {
                "mail was not searched, because this call did not ask for it — that is the \
                 default. Pass include_mail: true to search messages too, which is how you \
                 find a report another session filed."
            }
            MailExcluded::FactScoped => {
                "this query filters on a property only a fact has (status, provenance, \
                 standing, subject or edge), so it is a question about facts — messages, \
                 entities and prose are all out of it."
            }
            MailExcluded::TypeFiltered => {
                "this query names a type with answers_type, which asks which THINGS carry its \
                 keys — and a message is not a thing and carries none, so mail was left out of \
                 it. Drop answers_type to search messages too."
            }
            MailExcluded::KindFiltered => {
                "this query narrows to one entity kind, and a message belongs to no entity, so \
                 mail was left out of it. Drop `kind` to search messages too."
            }
        }
    }

    /// The next reason, so the note gathering walks every one of them — see
    /// [`walk`].
    #[cfg(test)]
    fn next(self) -> Option<Self> {
        match self {
            MailExcluded::NotAsked => Some(MailExcluded::FactScoped),
            MailExcluded::FactScoped => Some(MailExcluded::TypeFiltered),
            MailExcluded::TypeFiltered => Some(MailExcluded::KindFiltered),
            MailExcluded::KindFiltered => None,
        }
    }

    /// A query that this reason answers for, so the gathering below reaches
    /// each note through [`mail_coverage`] rather than reading the string out
    /// of [`note`](Self::note) and calling that the served text.
    #[cfg(test)]
    fn query(self) -> SearchQuery {
        match self {
            MailExcluded::NotAsked => SearchQuery {
                include_mail: false,
                ..SearchQuery::default()
            },
            // These two ask for mail, or they never reach their own reason:
            // not-asked answers first, and it is the default.
            MailExcluded::FactScoped => SearchQuery {
                status: Some(FactStatus::Active),
                ..asking_for_mail()
            },
            MailExcluded::TypeFiltered => SearchQuery {
                answers_type: Some(DeclaredType::new("kiln-firing", Vec::new())),
                ..asking_for_mail()
            },
            MailExcluded::KindFiltered => SearchQuery {
                kind: Some(EntityKind::PERSON),
                ..asking_for_mail()
            },
        }
    }
}

/// **Whether this answer covered mail, and why not when it didn't.**
///
/// One shape, always present, so a caller reads it in one pass instead of
/// branching on which keys came back — the same deal `owned_mailbox` makes.
///
/// It exists because silence is a lie here. A search is a read of an in-process
/// index: if the mailbox world was unreachable when that index was built, mail
/// is simply not in it, and an answer that comes back without mail hits and
/// without a word reads as "no message says that". That is a different claim
/// from "jojobot has read no messages", and it is the one a caller acts on.
fn mail_coverage(query: &SearchQuery, coverage: Coverage) -> serde_json::Value {
    let excluded = |note: &str| serde_json::json!({ "searched": false, "note": note });
    if let Some(left_out) = MailExcluded::of(query) {
        return excluded(left_out.note());
    }
    match coverage {
        Coverage::Unread => excluded(
            "NO message is searchable right now — this is not 'nothing matched'. The memory half \
             of this answer is complete. start_here's snapshot says whether the mailbox world is \
             reachable at all.",
        ),
        // Searched, and said so — hits are real. But the board read failed, so
        // only what this server has handled since is in there, and a caller
        // hunting an older message has to be told rather than shown an empty
        // list. Reporting this as `searched: false` was an answer that carried
        // message hits and denied having searched any.
        // One cause only: the mail half is filled by the board read and by
        // nothing else, so `unscanned` is the only way it goes behind. The
        // token still rides the answer, because a caller reads one vocabulary
        // across both halves rather than learning which of them can say what.
        Coverage::Partial(behind) => serde_json::json!({
            "searched": true,
            "behind": behind.as_token(),
            "note": "PARTIAL: the index holds only the messages jojobot has handled since this \
                     server started, so an older message may be missing. Any hit here is real — \
                     this is not a complete answer over mail. start_here's snapshot says whether \
                     the mailbox world is reachable at all.",
        }),
        Coverage::Loaded => serde_json::json!({ "searched": true }),
    }
}

/// **Whether this answer covered the memory graph, and what is missing when it
/// didn't** — [`mail_coverage`]'s question, asked of the other store, in the
/// same shape and the same words.
///
/// There is no caller-side reason for a gap here: memory is searched by every
/// query, so the only way this half is incomplete is that the index is behind
/// the store — a boot scan that failed, or a document jojobot wrote and could
/// not re-read afterwards. The write landed either way. What a caller loses is
/// the guarantee that this answer reflects it, and the way back is `recall`,
/// which reads the store.
fn memory_coverage(coverage: Coverage) -> serde_json::Value {
    match coverage {
        Coverage::Unread => serde_json::json!({
            "searched": false,
            "note": "NO entity, fact or prose is searchable right now — this is not 'nothing \
                     matched'. The memory verbs are unaffected: recall reads the store directly \
                     and is complete.",
        }),
        // **Two states, two notes.** They differ in how much is missing, which
        // is the whole of what a caller does with the answer: one says trust an
        // empty result about anything except the newest write, the other says
        // trust an empty result about nothing at all.
        Coverage::Partial(behind) => serde_json::json!({
            "searched": true,
            "behind": behind.as_token(),
            "note": match behind {
                Behind::Unscanned =>
                    "PARTIAL: the index holds only what jojobot has written since this server \
                     started, so anything older may be missing. Any hit here is real — this is \
                     not a complete answer over memory. recall reads the store directly and is \
                     complete.",
                Behind::Stale =>
                    "PARTIAL: the index holds an OLDER version of at least one entity than the \
                     store does. Every hit here is real, but one of them may be a version the \
                     store has moved past, and a fact written since may be missing. recall reads \
                     the store directly and is complete — use it when the answer matters.",
            },
        }),
        Coverage::Loaded => serde_json::json!({ "searched": true }),
    }
}

/// **Every state of a kind, walked rather than listed.**
///
/// Each state names its successor in a `match`, so a state added to the enum
/// fails to compile at that `match` and has to be placed in the walk. A list
/// literal has no such relationship to its enum: it accepts a new state's
/// absence silently, which for the gathering below means a note no check ever
/// reads.
#[cfg(test)]
fn walk<T: Copy>(first: T, next: impl Fn(T) -> Option<T>) -> Vec<T> {
    let mut walked = vec![first];
    while let Some(state) = next(*walked.last().expect("the walk starts with one")) {
        walked.push(state);
    }
    walked
}

/// A query that asks for mail and narrows nothing else — the one shape whose
/// mail coverage is decided by the INDEX rather than by the query, so it is
/// what the coverage notes below are gathered through. `SearchQuery::default()`
/// leaves mail out, which would gather the not-asked note four times over.
#[cfg(test)]
fn asking_for_mail() -> SearchQuery {
    SearchQuery {
        include_mail: true,
        ..SearchQuery::default()
    }
}

/// The next coverage state — [`MailExcluded::next`]'s job for the type this
/// module does not own.
#[cfg(test)]
fn next_coverage(coverage: Coverage) -> Option<Coverage> {
    match coverage {
        Coverage::Unread => Some(Coverage::Partial(Behind::Unscanned)),
        Coverage::Partial(Behind::Unscanned) => Some(Coverage::Partial(Behind::Stale)),
        Coverage::Partial(Behind::Stale) => Some(Coverage::Loaded),
        Coverage::Loaded => None,
    }
}

/// **Every note these two functions can put in an answer**, each labelled with
/// the state that produces it — the input to the surface's checks on the words
/// an agent is handed.
///
/// Both axes are walked rather than listed (see [`walk`]), so a coverage state
/// or a query-driven exclusion added later fails to compile here instead of
/// quietly going unread.
#[cfg(test)]
pub(crate) fn coverage_notes() -> Vec<(String, String)> {
    let mut found = Vec::new();
    // The query answers ahead of coverage, so these notes are the same whatever
    // the index holds. One pass, not one per coverage state.
    for left_out in walk(MailExcluded::NotAsked, MailExcluded::next) {
        found.push((
            format!("search's mail coverage note ({left_out:?})"),
            mail_coverage(&left_out.query(), Coverage::Loaded).to_string(),
        ));
    }
    for coverage in walk(Coverage::Unread, next_coverage) {
        found.push((
            format!("search's memory coverage note ({coverage:?})"),
            memory_coverage(coverage).to_string(),
        ));
        found.push((
            format!("search's mail coverage note ({coverage:?})"),
            mail_coverage(&asking_for_mail(), coverage).to_string(),
        ));
    }
    found
}

/// The front door: one ranked list over entities, facts and prose.
#[tool_router(router = search_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "The front door — use it first, and any time you do not already hold the \
                       exact handle or address. One ranked list over entities, facts, free \
                       prose AND the messages in mailboxes at once. `query` is free text (ALL \
                       words must match) and is optional when a filter narrows it: kind · status \
                       (default active; superseded is excluded unless named) · provenance · \
                       standing (`open` is how you ask which claims are still in doubt) · \
                       subject · edge {shape, object} · answers_type · fits_type; a call with \
                       neither query nor one of those filters is refused, and include_mail is not \
                       one of them \
                       — it says what to search, not what to narrow to. The two type filters are \
                       one question asked two ways, and a call names one of them, never both: \
                       answers_type keeps the things carrying SOME of a type's keys and says \
                       which each one lacks, fits_type keeps only the things with no gaps. Reach \
                       for answers_type unless you have a reason not to — a thing that arrives \
                       with its gaps named can be judged, and a thing fits_type leaves out looks \
                       exactly like a thing that is not there. kind + edge answers a cross-entity question in one \
                       call (\"which people are in X\") by walking typed edges — prose that \
                       merely mentions X is not an answer. No hit comes back bare: a fact \
                       carries the whole claim, its address (feed that to update_fact), and who it \
                       is `about` and where it is `home`d (a null name there means the handle \
                       names nothing — a real defect worth reporting); an entity or prose hit \
                       carries that entity's names and the edges its facts draw; a message hit \
                       carries its box, its state (new/read/processed — an archived report is \
                       findable, and the state is how you tell it from live work), its sender \
                       and the id read_message takes, plus a snippet rather than the whole body. \
                       Mail is OPT-IN: pass include_mail: true to search messages too, and \
                       reach for it when you want what a session knows — a report filed for \
                       another session is exactly the context you would not know to go looking \
                       for, and this is the one call that finds it. It is off unless you ask, \
                       because a message hit carries somebody's box, sender and a snippet, and \
                       this is the verb to reach for first. Note that a `kind` filter leaves \
                       mail out even when you ask, since a message belongs to no entity and so \
                       has no kind to match. ALWAYS read the \
                       `mail` field of the answer, in BOTH directions: searched: false means no \
                       message was searched at all, which is not the same as nothing matching; \
                       and searched: true can still be partial after a degraded start, where the \
                       hits are real but anything older than this server's start is missing. \
                       Whenever `mail` carries a `note`, that note says which case you are in — \
                       read it before concluding a message does not exist. `memory` answers the \
                       same question about entities, facts and prose: searched: false means \
                       nothing in memory is searchable right now, and searched: true with a \
                       note means the \
                       index is behind the store, where `behind` says how much: `unscanned` (the \
                       index holds only what jojobot has written since this server started) or \
                       `stale` (the index holds an older version of one entity than \
                       the store, so a hit may be a version the store has moved past). The hits \
                       are real either way, and \
                       `recall` reads the store itself. No \
                       pagination — raise `limit` or ask a better question."
    )]
    pub(crate) async fn search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Resolved before the query is built — see [`Jojobot::attributable`].
        // This verb publishes a `sid` and says it is what tells jojobot who is
        // asking, so a handle that addresses nothing is refused rather than
        // dropped.
        let asking = match self.caller(args.sid.as_deref()) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        if let Err(refused) = self.attributable(args.sid.as_deref()) {
            return Ok(refused);
        }
        // **The name is resolved to its declaration here, once.** Everything
        // below takes the keys rather than the name, so nothing deeper needs a
        // store to answer a type question — and a name nobody declared is
        // answered here, where the roster to offer instead is in reach.
        let declared = match &args.answers_type {
            None => None,
            Some(wanted) => match self.declared(wanted, "searched").await? {
                Ok(declared) => Some(declared),
                Err(refused) => return Ok(refused),
            },
        };
        // The strict half, resolved the same way and in the same place: the
        // two are one question with two answers, and a name that names no type
        // is answered here whichever of them carried it.
        let fits = match &args.fits_type {
            None => None,
            Some(wanted) => match self.declared(wanted, "searched").await? {
                Ok(declared) => Some(declared),
                Err(refused) => return Ok(refused),
            },
        };
        let edge = args
            .edge
            .as_ref()
            .map(|e| -> Result<EdgeFilter, McpError> {
                Ok(EdgeFilter {
                    shape: e.shape.as_deref().map(parse_shape).transpose()?,
                    object: EntityId(e.object.trim().to_string()),
                })
            })
            .transpose()?;
        let query = SearchQuery {
            answers_type: declared,
            fits_type: fits,
            asked_by: asking.as_ref().map(|c| c.bot.clone()),
            text: args.query,
            kind: args.kind.as_deref().map(parse_kind).transpose()?,
            status: args.status.as_deref().map(parse_status).transpose()?,
            provenance: args
                .provenance
                .as_deref()
                .map(parse_one_provenance)
                .transpose()?,
            standing: args.standing.as_deref().map(parse_standing).transpose()?,
            subject: args.subject.as_deref().map(EntityId::person),
            edge,
            include_mail: args.include_mail.unwrap_or(false),
            limit: args.limit.map_or(DEFAULT_LIMIT, |l| l as usize),
        };
        // Checked here as well as in the index: a malformed query is the caller's
        // mistake, and it should read as one no matter which adapter is behind us.
        // A query that narrows nothing, or narrows itself into a contradiction,
        // is a caller mistake and comes back as an answer with a way forward
        // (rule 68) rather than as a protocol error.
        if let Err(e) = query.validate() {
            return memory_declined("search", e);
        }
        // Awaited before the coverage below is read: the refresh this search
        // takes is what those two answers describe.
        let hits = match self.search.search(&query).await {
            Ok(hits) => hits,
            Err(e) => return memory_declined("search", e),
        };
        // **The day this answer is read against**, in the zone of the run that
        // asked: whether a claim has passed the day it stays good is a question
        // about a day, and two runs in two zones answer it differently for one
        // stored claim.
        let as_of = self.dated(None, args.sid.as_deref())?;
        let body = serde_json::json!({
            "count": hits.len(),
            // **A different question from the two coverage notes below.**
            // Those answer *was everything searched*; this answers *did the
            // query match what is there*, which is a property of the QUERY and
            // is true even when the index is complete.
            //
            // ⛔️ Kept apart on purpose: a caller told the index is loaded and
            // nothing else would read an empty answer as *nobody ever said
            // this*, which is the one inference this field exists to stop.
            "matching": matching_note(&query),
            "memory": memory_coverage(self.search.memory_coverage()),
            "mail": mail_coverage(&query, self.search.mail_coverage()),
            "results": hits
                .iter()
                .map(|hit| hit_json(hit, as_of))
                .collect::<Vec<_>>(),
        });
        json_result(&body)
    }
}

/// **What the matcher can and cannot promise about this query.**
///
/// Words are matched by stem and a query needs most of its terms rather than
/// all of them, so a hit list is what the matcher FOUND rather than everything
/// that is there. **An empty answer means the words did not match, never that
/// nobody said it** — and no coverage note says that, because coverage is about
/// what was read.
///
/// **A handle is exempt and says so**: it is matched whole, so an empty answer
/// under one really does mean no such thing is held.
fn matching_note(query: &search::SearchQuery) -> serde_json::Value {
    let exact = query
        .terms()
        .is_some_and(|text| EntityId(text.trim().to_string()).kind().is_some());
    serde_json::json!({
        "exact": exact,
        "note": if exact {
            "this is a handle, matched whole — an empty answer means jojobot holds no such thing"
        } else {
            "words are matched loosely, by stem, and a query needs most of its terms rather \
             than all of them. So these are the records that MATCHED, not everything that \
             is here: an empty or thin answer can mean the wording missed rather than that \
             nothing was ever recorded. Try fewer words, or the words the record itself \
             would use, before concluding jojobot was never told."
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;
    use crate::orientation::essay::ORIENTATION;
    use jojobot_domain::memory::{Boot, FactId};

    /// **Each query-driven exclusion is reachable, and by the query the note
    /// gathering uses to reach it.**
    ///
    /// The gathering serves every note through `mail_coverage`, the path a
    /// caller travels, rather than reading the sentence out of `note`. That is
    /// only worth anything while each reason's query really reaches that
    /// reason: a query that drifted would gather one reason's note twice and
    /// drop another, and the entry count would look exactly the same.
    #[test]
    fn every_query_driven_exclusion_is_reached_by_its_own_query() {
        for left_out in walk(MailExcluded::NotAsked, MailExcluded::next) {
            assert_eq!(
                MailExcluded::of(&left_out.query()),
                Some(left_out),
                "the query written for {left_out:?} reaches a different reason"
            );
        }
        // The positive half's pair: an unnarrowed query holds none of these
        // reasons, so the walk above is not simply matching everything.
        assert_eq!(
            MailExcluded::of(&asking_for_mail()),
            None,
            "a query that asks for mail and narrows nothing holds none of these reasons"
        );
    }

    /// **The note names every filter that can reach it.** A caller reading
    /// "this query filters on a property only a fact has" and then a list
    /// without the filter they passed is told their answer was narrowed by
    /// something they never sent — and goes looking for it.
    ///
    /// A type filter is NOT one of them, and gets a reason of its own: it asks
    /// which THINGS carry a type's keys, so mail is out because a message is
    /// not a thing rather than because the question is about rows.
    #[test]
    fn the_fact_scoped_note_names_every_filter_that_reaches_it() {
        let served = mail_coverage(
            &SearchQuery {
                status: Some(FactStatus::Superseded),
                ..asking_for_mail()
            },
            Coverage::Loaded,
        );
        let note = served["note"].as_str().expect("a note");
        for filter in ["status", "provenance", "subject", "edge"] {
            assert!(
                note.contains(filter),
                "the note leaves out {filter}, which is one of the filters that reaches it: {note}"
            );
        }
        assert!(
            !note.contains("answers_type"),
            "a type filter does not reach this reason, so naming it here would send a \
             caller looking for a narrowing that is not theirs: {note}"
        );

        let by_type = SearchQuery {
            answers_type: Some(DeclaredType::new("kiln-firing", Vec::new())),
            ..asking_for_mail()
        };
        assert_eq!(
            MailExcluded::of(&by_type),
            Some(MailExcluded::TypeFiltered),
            "a type filter has a reason of its own"
        );
        let typed = mail_coverage(&by_type, Coverage::Loaded);
        let note = typed["note"].as_str().expect("a note");
        assert!(
            note.contains("answers_type"),
            "…and the reason names the argument the caller actually passed: {note}"
        );
    }

    /// **Mail is opt-in at the door, and the opt-in reaches the port.**
    ///
    /// `search` is the verb the surface tells a session to reach for first, and
    /// a mail hit carries somebody's box, state, sender and a snippet. A
    /// session told to leave a box alone cannot use the front door safely if
    /// the unsafe branch is what a bare call does, so the default is the safe
    /// one and reaching mail is asked for (rule 62).
    ///
    /// Both directions, because the negative alone passes on a build where the
    /// flag never reaches the port at all.
    #[tokio::test]
    async fn mail_is_left_out_unless_a_caller_asks_for_it() {
        for (asked, wanted) in [(None, false), (Some(false), false), (Some(true), true)] {
            let spy = Arc::new(SpySearch::default());
            handler_with(spy.clone())
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    include_mail: asked,
                    ..search_args()
                }))
                .await
                .expect("search ok");
            assert_eq!(
                spy.query().include_mail,
                wanted,
                "include_mail: {asked:?} must reach the port as {wanted}"
            );
        }
    }

    /// Every argument reaches the port as the typed query it means — including the
    /// edge filter, which is the whole point of the verb.
    #[tokio::test]
    async fn search_translates_every_argument_into_the_query() {
        let spy = Arc::new(SpySearch::default());
        let jojobot = handler_with(spy.clone());
        jojobot
            .search(Parameters(SearchArgs {
                answers_type: None,
                query: Some("winter".into()),
                kind: Some("person".into()),
                status: Some("superseded".into()),
                provenance: Some("testimony".into()),
                standing: Some("open".into()),
                subject: Some("person:alpha".into()),
                edge: Some(EdgeFilterArgs {
                    shape: Some("location".into()),
                    object: "place:shelbyville".into(),
                }),
                include_mail: Some(false),
                limit: Some(5),
                sid: None,
                fits_type: None,
            }))
            .await
            .expect("search ok");

        let query = spy.query();
        assert_eq!(query.terms(), Some("winter"));
        assert!(
            !query.include_mail,
            "the caller's exclusion must reach the port"
        );
        assert_eq!(query.kind, Some(EntityKind::PERSON));
        assert_eq!(query.status, Some(FactStatus::Superseded));
        assert_eq!(query.provenance, Some(Provenance::Testimony));
        assert_eq!(query.standing, Some(Standing::Open));
        assert_eq!(
            query.subject.as_ref().map(|s| s.as_str()),
            Some("person:alpha")
        );
        let edge = query
            .edge
            .expect("the edge filter must survive translation");
        assert_eq!(edge.shape, Some(EdgeShape::Location));
        assert_eq!(edge.object.as_str(), "place:shelbyville");
        assert_eq!(query.limit, 5);
    }

    /// An edge filter with no shape means any edge pointing at the object, and the
    /// limit defaults to twenty.
    #[tokio::test]
    async fn a_shapeless_edge_filter_and_the_default_limit_reach_the_port() {
        let spy = Arc::new(SpySearch::default());
        handler_with(spy.clone())
            .search(Parameters(SearchArgs {
                edge: Some(EdgeFilterArgs {
                    shape: None,
                    object: "event:winter-fest".into(),
                }),
                ..search_args()
            }))
            .await
            .expect("search ok");
        let query = spy.query();
        assert_eq!(query.edge.as_ref().map(|e| e.shape), Some(None));
        assert_eq!(query.limit, DEFAULT_LIMIT);
    }

    /// Neither text nor a filter is a request for everything, which is not a
    /// search — and it is the caller's mistake, whatever adapter is behind us.
    #[tokio::test]
    async fn search_with_neither_text_nor_a_filter_is_a_blocked_answer() {
        let body = blocked(
            &handler()
                .search(Parameters(search_args()))
                .await
                .expect("a caller mistake is an answer, not a protocol failure"),
        );
        assert_eq!(body["wrote"], false, "{body}");
        assert!(
            body["how_to_proceed"]
                .as_str()
                .is_some_and(|advice| !advice.is_empty()),
            "a refusal names a way forward: {body}"
        );
    }

    /// Bad tokens are client errors, not silent fallbacks: a mistyped `status`
    /// that quietly became `active` would answer a question about superseded
    /// rows with the live ones and look like a straight answer.
    ///
    /// **Every case carries query text**, so the refusal can only be the bad
    /// token. Without it, an implementation that dropped the filter entirely
    /// would still error — as an unbounded search — and this would pass green
    /// over a `search` that ignored its filters.
    ///
    /// **Refused either way, and the CHANNEL splits on where the fault is
    /// decided.** A caller mistake the domain sees comes back as a blocked
    /// answer with a way forward (rule 68). A token the MCP edge cannot parse
    /// at all — one naming no kind, no status, no provenance, no shape — is
    /// refused before any domain error exists, so it never reaches that path
    /// and stays a protocol error.
    ///
    /// That split is a boundary rather than an oversight, and the two groups
    /// sit side by side here because it is what the boundary costs a caller:
    /// two shapes of refusal for two faults they cannot tell apart. Where a
    /// malformed argument is decided is its own question.
    #[tokio::test]
    async fn malformed_search_filters_are_refused() {
        let jojobot = handler();
        let searching = || SearchArgs {
            query: Some("winter".into()),
            ..search_args()
        };

        // Decided at the edge: still errors.
        let unparsable = [
            SearchArgs {
                kind: Some("receipt".into()),
                ..searching()
            },
            SearchArgs {
                status: Some("retired".into()),
                ..searching()
            },
            SearchArgs {
                provenance: Some("maybe".into()),
                ..searching()
            },
            SearchArgs {
                edge: Some(EdgeFilterArgs {
                    shape: Some("knows".into()),
                    object: "place:x".into(),
                }),
                ..searching()
            },
        ];
        for args in unparsable {
            let err = jojobot
                .search(Parameters(args))
                .await
                .expect_err("a malformed filter must be refused");
            assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        }

        // Decided by the domain: blocked answers.
        let refused = [
            // A *bare* subject is read as a person, as everywhere else — so the
            // malformed case is one that can't be an id at all.
            SearchArgs {
                subject: Some("person:a|b".into()),
                ..searching()
            },
            SearchArgs {
                edge: Some(EdgeFilterArgs {
                    shape: None,
                    object: "place:a|b".into(),
                }),
                ..searching()
            },
            SearchArgs {
                limit: Some(0),
                ..searching()
            },
        ];
        for args in refused {
            let body = blocked(
                &jojobot
                    .search(Parameters(args))
                    .await
                    .expect("a caller mistake is an answer, not a protocol failure"),
            );
            assert_eq!(body["wrote"], false, "{body}");
            assert!(
                body["how_to_proceed"]
                    .as_str()
                    .is_some_and(|advice| !advice.is_empty()),
                "a refusal names a way forward: {body}"
            );
        }
    }

    /// **Mail comes back in the one list, and unmistakably as mail.** A message
    /// hit says which box, which state, who sent it, and the id `read_message`
    /// takes — without those it is an anonymous paragraph and a reader cannot
    /// tell a live task from an archived report. The body is a snippet: taking
    /// the whole message is `read_message`'s job, and that is a deliberate act.
    /// 🚨 **An answer admits the WORDS may have missed, even when everything
    /// was searched.**
    ///
    /// Matching is loose now: stems, and most of a query's terms rather than
    /// all of them. **So a hit list is what the matcher FOUND, not everything
    /// that is there** — and an empty answer means the wording missed, never
    /// that nobody said it.
    ///
    /// ⛔️ **The coverage notes cannot carry this.** They answer *was
    /// everything searched*, and here everything WAS: `memory.searched` is
    /// true beside it. **A caller told the index is complete and nothing else
    /// reads an empty answer as never-recorded**, which is the fabrication
    /// shape this exists to stop.
    ///
    /// ⚠️ **Asserted on a fully-loaded index on purpose.** A case that only
    /// checked the note on a degraded one would pass against a build that
    /// folded this into coverage.
    #[tokio::test]
    async fn an_answer_says_the_wording_may_have_missed_even_when_all_was_searched() {
        let spy = Arc::new(SpySearch::answering(Vec::new()));
        let body = json_of(
            &handler_with(spy)
                .search(Parameters(SearchArgs {
                    query: Some("committee meets".into()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(body["count"], 0, "the empty answer is the case");
        assert_eq!(
            body["memory"]["searched"], true,
            "the point is that everything WAS searched and a miss is still possible",
        );
        assert_eq!(
            body["matching"]["exact"], false,
            "an answer over loose matching did not say it was loose",
        );
        let note = body["matching"]["note"]
            .as_str()
            .unwrap_or_else(|| panic!("the field is there and says nothing: {body}"));
        // ⚠️ **One clean line.** A wrapped literal that loses its continuation
        // renders runs of spaces, and this note is read by an agent — I shipped
        // exactly that for a few minutes and only a sabotage's own output
        // showed it.
        assert!(
            !note.contains('\n') && !note.contains("  "),
            "the note is not one clean line: {note:?}",
        );

        // ⭐ **And a handle is exempt and says so.** It is matched whole, so an
        // empty answer under one really does mean no such thing is held —
        // without this half, a build that called every query approximate would
        // pass.
        let spy = Arc::new(SpySearch::answering(Vec::new()));
        let handle = json_of(
            &handler_with(spy)
                .search(Parameters(SearchArgs {
                    query: Some("person:alpha".into()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(
            handle["matching"]["exact"], true,
            "a handle was reported as loosely matched: {handle}",
        );
    }

    #[tokio::test]
    async fn a_message_hit_arrives_with_its_whole_envelope() {
        let spy = Arc::new(SpySearch::answering(vec![Hit::Message {
            message: Message {
                id: MessageId("42".into()),
                mailbox: MailboxName("pm".into()),
                body: "The kiln rebuild landed; the damper is still hand-cut.".into(),
                subject: Some("the kiln slice".into()),
                sender: "dev (implementer)".into(),
                sent_at: jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant"),
                state: mailbox::MessageState::Processed,
                notes: Some("filed".into()),
                in_reply_to: None,
                taken_by: None,
            },
            snippet: "…the damper is still hand-cut…".into(),
        }]));

        let body = json_of(
            &handler_with(spy)
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    include_mail: Some(true),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        let hit = &body["results"][0];
        assert_eq!(
            hit["hit"], "message",
            "a caller must not have to guess from the shape"
        );
        assert_eq!(hit["id"], "42", "the id read_message takes");
        assert_eq!(hit["mailbox"], "pm");
        assert_eq!(hit["state"], "processed", "an archive reads as one");
        assert_eq!(hit["sender"], "dev (implementer)");
        assert_eq!(hit["subject"], "the kiln slice");
        assert_eq!(hit["notes"], "filed");
        assert!(hit["sent_at"].is_string());
        assert_eq!(hit["snippet"], "…the damper is still hand-cut…");
        assert!(
            hit["body"].is_null(),
            "the whole body is read_message's to hand over, not a hit's: {hit}"
        );
        assert_eq!(body["mail"]["searched"], true);
    }

    /// **A search that could not see mail says so.** Coming back without mail
    /// hits and without a word reads as "no message says that", which is a
    /// different claim from "jojobot has read no messages" — and it is the one a
    /// caller acts on. The memory half is unaffected: degrade, don't error.
    #[tokio::test]
    async fn a_search_says_when_no_message_was_searched_at_all() {
        let body = json_of(
            &handler_with(Arc::new(SpySearch::with_no_mail_indexed()))
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    include_mail: Some(true),
                    ..search_args()
                }))
                .await
                .expect("a down mailbox world must not break search"),
        );
        assert_eq!(body["mail"]["searched"], false);
        let note = body["mail"]["note"].as_str().expect("an absence says why");
        assert!(
            note.contains("not 'nothing matched'"),
            "the note has to draw the distinction it exists for: {note}"
        );

        // The caller's own exclusion is a different absence, and says so.
        let excluded = json_of(
            &handler_with(Arc::new(SpySearch::default()))
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    include_mail: Some(false),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(excluded["mail"]["searched"], false);
        assert!(
            excluded["mail"]["note"]
                .as_str()
                .expect("a note")
                .contains("include_mail"),
            "an exclusion the caller asked for must not read as an outage: {excluded}"
        );

        // …and so is a query that is about facts to begin with.
        let fact_scoped = json_of(
            &handler_with(Arc::new(SpySearch::default()))
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    provenance: Some("testimony".into()),
                    include_mail: Some(true),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(fact_scoped["mail"]["searched"], false);
        assert!(
            fact_scoped["mail"]["note"]
                .as_str()
                .expect("a note")
                .contains("only a fact has"),
            "got {fact_scoped}"
        );
    }

    /// **THE INVARIANT: no answer both returns a message hit and claims no
    /// message was searched.** After a failed boot board read, every verb still
    /// indexes the messages it touches and search still returns them — while the
    /// coverage flag stayed false for the life of the process. One answer said
    /// both things at once, and a caller reading the field it is told to trust
    /// would discard a hit that is real.
    ///
    /// The fix is a third state, not a flipped flag: hits are real, but the
    /// board was never read, so anything older than this process is missing —
    /// which a caller hunting an old message has to be told rather than shown an
    /// empty list.
    #[tokio::test]
    async fn an_answer_carrying_a_message_never_claims_no_mail_was_searched() {
        let hit = || {
            vec![Hit::Message {
                message: Message {
                    id: MessageId("42".into()),
                    mailbox: MailboxName("pm".into()),
                    body: "the damper is still hand-cut".into(),
                    subject: None,
                    sender: "dev".into(),
                    sent_at: jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant"),
                    state: mailbox::MessageState::New,
                    notes: None,
                    in_reply_to: None,
                    taken_by: None,
                },
                snippet: "…the damper…".into(),
            }]
        };

        // **`Unscanned` is the only way the mail half goes behind**, and the
        // double is fed a state the index can actually report: the board read
        // is what fills this half and nothing else touches it, so
        // `FullTextIndex::mail_coverage` reaches `Partial` from that one state.
        // A double handed `Stale` here tests an answer no store produces.
        for coverage in [Coverage::Partial(Behind::Unscanned), Coverage::Loaded] {
            let body = json_of(
                &handler_with(Arc::new(SpySearch::covering(coverage, hit())))
                    .search(Parameters(SearchArgs {
                        query: Some("damper".into()),
                        include_mail: Some(true),
                        ..search_args()
                    }))
                    .await
                    .expect("search ok"),
            );
            assert!(
                body["results"]
                    .as_array()
                    .expect("results")
                    .iter()
                    .any(|h| h["hit"] == "message"),
                "the double answered with a message: {body}"
            );
            assert_eq!(
                body["mail"]["searched"], true,
                "an answer carrying a message hit cannot claim no message was searched \
                 ({coverage:?}): {body}"
            );
        }

        // …and the degraded one still says it is degraded, or the caller reads a
        // partial answer over mail as a complete one.
        let partial = json_of(
            &handler_with(Arc::new(SpySearch::covering(
                Coverage::Partial(Behind::Unscanned),
                hit(),
            )))
            .search(Parameters(SearchArgs {
                query: Some("damper".into()),
                include_mail: Some(true),
                ..search_args()
            }))
            .await
            .expect("search ok"),
        );
        assert!(
            partial["mail"]["note"]
                .as_str()
                .expect("a partial answer says it is partial")
                .contains("PARTIAL"),
            "got {partial}"
        );
        // **The token beside that note, because a client is told to branch on
        // it.** Unasserted, it could carry any word — and the note it sits
        // beside describes one cause only, so a token naming another one
        // contradicts the sentence next to it.
        assert_eq!(
            partial["mail"]["behind"], "unscanned",
            "the mail half goes behind one way, and the token says which: {partial}"
        );
    }

    /// **An answer served from a memory half that is behind says so.**
    ///
    /// The mail half has said this since it shipped; the memory half never
    /// learned to, so a `search` answered out of an index holding the version
    /// before somebody's write read exactly like a complete one. Honesty to the
    /// caller who wrote does nothing for the sessions that read afterwards, and
    /// those are the ones with no way to tell.
    #[tokio::test]
    async fn an_answer_from_a_memory_half_that_is_behind_says_so() {
        let hit = || {
            vec![Hit::Entity {
                entity: Entity {
                    id: EntityId("person:alpha".into()),
                    kind: EntityKind::PERSON,
                    name: "Alpha".into(),
                    aliases: Vec::new(),
                    source: "user-named".into(),
                    crm: None,
                    parent: None,
                    boot: Boot::OnDemand,
                    merged_into: None,
                },
                doc_id: "doc-9".into(),
                edges: Vec::new(),
                answers: None,
            }]
        };

        let complete = json_of(
            &handler_with(Arc::new(SpySearch::over_memory(Coverage::Loaded, hit())))
                .search(Parameters(SearchArgs {
                    query: Some("alpha".into()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(
            complete["memory"]["searched"], true,
            "a loaded half searched everything: {complete}"
        );
        assert!(
            complete["memory"]["note"].is_null(),
            "and has nothing to warn about: {complete}"
        );

        let behind = json_of(
            &handler_with(Arc::new(SpySearch::over_memory(
                Coverage::Partial(Behind::Stale),
                hit(),
            )))
            .search(Parameters(SearchArgs {
                query: Some("alpha".into()),
                ..search_args()
            }))
            .await
            .expect("search ok"),
        );
        // Searched, and said so — the hits are real. The claim is that something
        // is MISSING, which is a different thing from nothing being searched, and
        // collapsing the two would make an answer carry hits and deny searching.
        assert_eq!(
            behind["memory"]["searched"], true,
            "partial is not empty: {behind}"
        );
        assert!(
            behind["memory"]["note"]
                .as_str()
                .expect("a partial answer says it is partial")
                .contains("PARTIAL"),
            "got {behind}"
        );

        let unread = json_of(
            &handler_with(Arc::new(SpySearch::over_memory(
                Coverage::Unread,
                Vec::new(),
            )))
            .search(Parameters(SearchArgs {
                query: Some("alpha".into()),
                ..search_args()
            }))
            .await
            .expect("search ok"),
        );
        assert_eq!(
            unread["memory"]["searched"], false,
            "an index that never read the store searched nothing: {unread}"
        );
        assert!(
            unread["memory"]["note"]
                .as_str()
                .expect("an absence says why")
                .contains("recall"),
            "…and names the verb that reads the store instead: {unread}"
        );
    }

    /// **Two states reach `PARTIAL`, and an answer says which one.**
    ///
    /// A boot scan that never ran leaves almost nothing searchable; a doc whose
    /// refresh failed leaves almost everything searchable. Told the second when
    /// it is in the first, a caller reads an empty result as "nothing says
    /// that" — which is the exact wrong conclusion the whole coverage block
    /// exists to prevent.
    ///
    /// The cause is pinned by its token rather than by a sentence: the wording
    /// is free to improve, and `behind` is what a client is meant to branch on.
    #[tokio::test]
    async fn the_two_ways_into_partial_come_back_as_different_answers() {
        let answer = async |behind: Behind| {
            json_of(
                &handler_with(Arc::new(SpySearch::over_memory(
                    Coverage::Partial(behind),
                    Vec::new(),
                )))
                .search(Parameters(SearchArgs {
                    query: Some("alpha".into()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
            )
        };

        let unscanned = answer(Behind::Unscanned).await;
        let stale = answer(Behind::Stale).await;

        for (body, token) in [(&unscanned, "unscanned"), (&stale, "stale")] {
            assert_eq!(
                body["memory"]["searched"], true,
                "either way the hits are real and the half was searched: {body}"
            );
            assert_eq!(
                body["memory"]["behind"], token,
                "the cause is on the answer, not left to be read out of the prose: {body}"
            );
        }

        // The pair the tokens depend on: two labels over one sentence would
        // satisfy every assertion above and tell the caller nothing.
        assert_ne!(
            unscanned["memory"]["note"], stale["memory"]["note"],
            "the two states make different claims, so they cannot share a note"
        );
    }

    /// **A `stale` answer says where the caller stands, never what the server
    /// did to find out.**
    ///
    /// The two surfaces that carry it — the coverage note on the answer and the
    /// tool description a caller plans against — are checked together, because
    /// a caller reads whichever one it reaches first and both are the surface.
    ///
    /// The needles are the mechanism vocabulary and the verb the caller acts
    /// on, never a sentence: the wording stays free to improve, and an
    /// assertion that only knows the mechanism is absent passes identically
    /// against an empty string.
    #[tokio::test]
    async fn a_stale_answer_states_the_callers_position_and_not_the_mechanism() {
        let stale = json_of(
            &handler_with(Arc::new(SpySearch::over_memory(
                Coverage::Partial(Behind::Stale),
                Vec::new(),
            )))
            .search(Parameters(SearchArgs {
                query: Some("alpha".into()),
                ..search_args()
            }))
            .await
            .expect("search ok"),
        );
        let note = stale["memory"]["note"]
            .as_str()
            .expect("a partial answer says it is partial")
            .to_string();

        let tools = Jojobot::tool_router().list_all();
        let description = tools
            .iter()
            .find(|t| t.name == "search")
            .expect("search is a tool")
            .description
            .as_deref()
            .unwrap_or_default()
            .to_string();

        for (surface, text, present) in [
            ("the coverage note", note.as_str(), "PARTIAL"),
            ("the search description", description.as_str(), "stale"),
        ] {
            assert!(
                text.contains(present),
                "{surface} has to state the caller is looking at a partial answer ({present:?}): \
                 {text}"
            );
            assert!(
                text.contains("recall"),
                "{surface} has to name the verb that reads the store instead: {text}"
            );
            for mechanism in ["re-read", "could not"] {
                assert!(
                    !text.contains(mechanism),
                    "{surface} narrates how jojobot checked itself ({mechanism:?}) instead of what \
                     the caller holds: {text}"
                );
            }
        }
    }

    /// **Every coverage note states where the caller stands, and none of them
    /// narrates what the server tried.**
    ///
    /// The `stale` note was converted on its own and the same construction
    /// survived in four more places, across both halves. Converting one half
    /// would leave the two telling a caller different kinds of thing, which is
    /// the one-vocabulary property [`Coverage`]'s own doc comment claims, so
    /// all of them are checked here together.
    ///
    /// **What must survive the rewording is where the line falls** — everything
    /// before this server started may be missing, everything written since is
    /// there. That is how the index is arranged and a caller acts on it; the
    /// attempt that left it that way is the server's business.
    #[tokio::test]
    async fn no_coverage_note_narrates_what_the_server_tried() {
        let asking = || SearchArgs {
            query: Some("alpha".into()),
            include_mail: Some(true),
            ..search_args()
        };
        let note = async |spy: SpySearch, half: &str| -> String {
            json_of(
                &handler_with(Arc::new(spy))
                    .search(Parameters(asking()))
                    .await
                    .expect("search ok"),
            )[half]["note"]
                .as_str()
                .expect("a half that is behind says so")
                .to_string()
        };

        // Each half sends a caller somewhere different when it is behind, so
        // each is pinned to its own verb rather than to a shared word.
        let surfaces = [
            (
                "the memory half, never read",
                note(
                    SpySearch::over_memory(Coverage::Unread, Vec::new()),
                    "memory",
                )
                .await,
                "recall",
            ),
            (
                "the memory half, unscanned",
                note(
                    SpySearch::over_memory(Coverage::Partial(Behind::Unscanned), Vec::new()),
                    "memory",
                )
                .await,
                "recall",
            ),
            (
                "the mail half, never read",
                note(SpySearch::with_no_mail_indexed(), "mail").await,
                "start_here",
            ),
            (
                "the mail half, unscanned",
                note(
                    SpySearch::covering(Coverage::Partial(Behind::Unscanned), Vec::new()),
                    "mail",
                )
                .await,
                "start_here",
            ),
            (
                "the search description",
                Jojobot::tool_router()
                    .list_all()
                    .iter()
                    .find(|t| t.name == "search")
                    .expect("search is a tool")
                    .description
                    .as_deref()
                    .unwrap_or_default()
                    .to_string(),
                "unscanned",
            ),
        ];

        for (surface, text, present) in surfaces {
            assert!(
                text.contains(present),
                "{surface} has to tell a caller what to do about it ({present:?}): {text}"
            );
            for mechanism in [
                "could not",
                "has not been able",
                "never ran",
                "was never read",
            ] {
                assert!(
                    !text.contains(mechanism),
                    "{surface} narrates an attempt the server made ({mechanism:?}) instead of \
                     what the caller holds: {text}"
                );
            }
        }
    }

    /// **A `kind` filter excludes every message, and the answer has to say so.**
    /// The exclusion is structural and silent — a message doc carries no `kind`
    /// field, so the filter's own MUST clause drops it, exactly as it drops
    /// prose in nobody's doc. The coverage block knew three reasons and not this
    /// one, so `kind`-filtered answers claimed `searched: true` while the tool
    /// description tells a caller to trust that field. A field worth reading is
    /// a field that has to be right in every case, not in most of them.
    #[tokio::test]
    async fn a_kind_filter_reports_that_mail_was_left_out() {
        let body = json_of(
            &handler_with(Arc::new(SpySearch::default()))
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    kind: Some("person".into()),
                    include_mail: Some(true),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(
            body["mail"]["searched"], false,
            "a kind filter leaves no message in the answer, so it cannot claim it searched them"
        );
        let note = body["mail"]["note"].as_str().expect("an absence says why");
        assert!(
            note.contains("kind"),
            "…and it says which filter did it, since the caller can drop that one: {note}"
        );

        // The tool description makes the same promise, so it names this case too.
        let tools = Jojobot::tool_router().list_all();
        let description = tools
            .iter()
            .find(|t| t.name == "search")
            .expect("search is a tool")
            .description
            .as_deref()
            .unwrap_or_default();
        assert!(
            description.contains("kind") && description.contains("mail"),
            "the description tells a caller kind and mail interact: {description}"
        );
    }

    /// **The description states a refusal rule, so it owes the list that rule
    /// runs on.**
    ///
    /// "A call with neither query nor one of those filters is refused" is a
    /// promise about what the validator does, and it is true only while the
    /// list beside it is the validator's own. A filter the validator accepts
    /// alone and the list leaves out is a caller told its call will be refused
    /// when it will not — so the caller invents a text query to get past a gate
    /// that was never there, and narrows an answer it wanted whole.
    ///
    /// **Both halves per filter**, because either alone passes on nothing: the
    /// validator really accepts that filter as a query's stand-in, and the
    /// description really names it. Matching a name the validator refuses would
    /// be pinning prose about a rule that does not exist.
    #[test]
    fn the_search_description_names_every_filter_that_stands_in_for_a_query() {
        // **This case runs in a booted process**, because the validator below
        // parses a kind against the set and this case asserts about the
        // DESCRIPTION rather than about the set. Standing a store up is where
        // the set comes from; nothing else here needs the store.
        let _booted = jojobot_domain::memory::testing::InMemoryMemory::booted();
        let a_type = || DeclaredType::new("kiln-firing", vec![Field::new("cone", ValueType::Text)]);
        let alone = [
            (
                "kind",
                SearchQuery {
                    kind: Some(EntityKind::PERSON),
                    ..SearchQuery::default()
                },
            ),
            (
                "status",
                SearchQuery {
                    status: Some(FactStatus::Superseded),
                    ..SearchQuery::default()
                },
            ),
            (
                "provenance",
                SearchQuery {
                    provenance: Some(Provenance::Testimony),
                    ..SearchQuery::default()
                },
            ),
            (
                "subject",
                SearchQuery {
                    subject: Some(EntityId("person:alpha".into())),
                    ..SearchQuery::default()
                },
            ),
            (
                "edge",
                SearchQuery {
                    edge: Some(EdgeFilter {
                        shape: Some(EdgeShape::Location),
                        object: EntityId("place:shelbyville".into()),
                    }),
                    ..SearchQuery::default()
                },
            ),
            (
                "answers_type",
                SearchQuery {
                    answers_type: Some(a_type()),
                    ..SearchQuery::default()
                },
            ),
            (
                "fits_type",
                SearchQuery {
                    fits_type: Some(a_type()),
                    ..SearchQuery::default()
                },
            ),
        ];

        let tools = Jojobot::tool_router().list_all();
        let description = tools
            .iter()
            .find(|t| t.name == "search")
            .expect("search is a tool")
            .description
            .as_deref()
            .unwrap_or_default()
            .to_string();

        for (name, query) in alone {
            assert!(
                query.text.is_none(),
                "{name} has to stand in for the query, so this case carries no text"
            );
            query.validate().unwrap_or_else(|e| {
                panic!(
                    "{name} alone is accepted as a query's stand-in, but the validator said: {e}"
                )
            });
            assert!(
                description.contains(name),
                "…and the description states the refusal without naming {name}, so a caller \
                 passing it alone reads that its call will be refused: {description}"
            );
        }
    }

    /// `search`'s description must never claim mail is unreachable from
    /// here — that is false, and it would send a caller to a second verb
    /// that does not exist. Pinned rather than fixed once, because the
    /// sentence is exactly the kind that survives a rewrite by being
    /// plausible.
    #[test]
    fn the_search_description_no_longer_says_mail_is_unsearchable() {
        let tools = Jojobot::tool_router().list_all();
        let search = tools
            .iter()
            .find(|t| t.name == "search")
            .expect("search is a tool");

        // **All three surfaces, not the one that was noticed.** The claim was
        // written down in three places — the tool description, the orientation
        // `start_here` hands over, and the server instructions every
        // client loads before it calls anything — and fixing one leaves a
        // session reading either of the others exactly as misinformed as before.
        let instructions = handler().get_info().instructions.unwrap_or_default();
        for (surface, text) in [
            (
                "the search description",
                search.description.as_deref().unwrap_or_default(),
            ),
            ("the orientation", ORIENTATION),
            ("the server instructions", instructions.as_str()),
        ] {
            for stale in [
                "Messages and mailboxes are not searchable",
                "not searchable here",
                "sees memory only",
                "never messages",
            ] {
                assert!(
                    !text.contains(stale),
                    "{surface} still claims mail is out of reach ({stale:?})"
                );
            }
            assert!(
                text.contains("searchable") || text.contains("include_mail"),
                "{surface} has to say that mail IS reachable — silence reads as the old claim"
            );
        }
        assert!(
            search
                .description
                .as_deref()
                .unwrap_or_default()
                .contains("include_mail"),
            "…and the description has to name the parameter that takes mail back out"
        );
    }

    /// **One list, every hit typed — and none of them bare.** An entity, a fact
    /// and a prose match come back together, each saying what it is, carrying
    /// what makes it actionable, *and* carrying its surroundings: the fact names
    /// the entities it is about and sits on, the entity and the prose doc carry
    /// the edges that place them in the graph.
    #[tokio::test]
    async fn search_renders_a_mixed_list_of_typed_hits() {
        let entity = Entity {
            id: EntityId("work:first-mix".into()),
            kind: EntityKind::WORK,
            name: "First Mix".into(),
            aliases: vec!["The First One".into()],
            source: "user-named".into(),
            crm: None,
            parent: None,
            boot: Boot::OnDemand,
            merged_into: None,
        };
        let fact = Fact {
            id: FactId("f3".into()),
            home: EntityId::person("person:alpha"),
            subject: EntityId::person("person:alpha"),
            content: "spending the winter away".into(),
            details: Some("said so in June".into()),
            provenance: Provenance::Testimony,
            standing: Standing::Open,
            status: FactStatus::Active,
            date: jiff::civil::date(2026, 7, 1),
            edge: Some(Edge::new(
                EdgeShape::Membership,
                EntityId("org:guild".into()),
            )),
            fields: Default::default(),
            refs: Vec::new(),
            derived_from: None,
            inserted_at: None,
            stale_after: None,
        };
        let alpha = Entity {
            id: EntityId::person("person:alpha"),
            kind: EntityKind::PERSON,
            name: "Alpha".into(),
            aliases: vec!["Al".into()],
            source: "user-named".into(),
            crm: None,
            parent: None,
            boot: Boot::OnDemand,
            merged_into: None,
        };
        let guild = Edge::new(EdgeShape::Membership, EntityId("org:guild".into()));
        let spy = Arc::new(SpySearch::answering(vec![
            Hit::Entity {
                entity,
                doc_id: "doc-9".into(),
                edges: vec![guild.clone()],
                answers: None,
            },
            Hit::Fact {
                fact: Box::new(fact),
                subject: EntityRef::resolved(&alpha),
                home: EntityRef::resolved(&alpha),
                source: None,
            },
            Hit::Prose {
                doc_id: "doc-1".into(),
                title: "Alpha".into(),
                entity: Some(alpha.clone()),
                edges: vec![guild],
                snippet: "…allergic to penicillin…".into(),
            },
        ]));

        let body = json_of(
            &handler_with(spy)
                .search(Parameters(SearchArgs {
                    query: Some("winter".into()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(body["count"], 3);
        let results = body["results"].as_array().expect("a list of results");

        assert_eq!(results[0]["hit"], "entity");
        assert_eq!(results[0]["id"], "work:first-mix");
        assert_eq!(results[0]["type"], "CreativeWork", "the schema.org name");
        // **And no id of the thing it was stored in.** It was the one place a
        // caller could learn that an entity has a document behind it, and it
        // addressed nothing — every verb here takes a handle or a fact address.
        assert!(
            results[0]["doc"].is_null(),
            "an entity hit says where it sits in the graph, never where it sits in a store"
        );
        assert_eq!(
            results[0]["edges"][0]["type"], "memberOf",
            "where it sits in the graph"
        );
        assert_eq!(results[0]["edges"][0]["object"], "org:guild");

        assert_eq!(results[1]["hit"], "fact");
        assert_eq!(
            results[1]["address"], "person:alpha#f3",
            "a fact hit is editable"
        );
        assert_eq!(
            results[1]["subject"], "person:alpha",
            "the row keeps one spelling across capture, recall and search"
        );
        assert_eq!(results[1]["content"], "spending the winter away");
        assert_eq!(results[1]["details"], "said so in June");
        assert_eq!(results[1]["provenance"], "testimony");
        assert_eq!(results[1]["status"], "active");
        assert_eq!(results[1]["date"], "2026-07-01");
        assert_eq!(results[1]["edge"]["type"], "memberOf");
        assert_eq!(results[1]["edge"]["object"], "org:guild");
        // …and the surroundings, resolved: who this is about, and whose page it
        // sits on. A handle alone costs the reader a call to find out.
        assert_eq!(results[1]["about"]["id"], "person:alpha");
        assert_eq!(results[1]["about"]["type"], "Person");
        assert_eq!(results[1]["about"]["name"], "Alpha");
        assert_eq!(results[1]["home"]["id"], "person:alpha");
        assert_eq!(results[1]["home"]["name"], "Alpha");
        // …under the same key an entity hit uses, so one shape means one thing.
        assert_eq!(
            results[1]["about"]["alternateName"][0], "Al",
            "a search on the nickname has to show the linkage on the hit itself"
        );
        assert_eq!(results[1]["home"]["alternateName"][0], "Al");

        assert_eq!(results[2]["hit"], "prose");
        assert!(
            results[2]["doc"].is_null(),
            "…and neither does a stretch of prose: what a caller can act on is its entity"
        );
        assert_eq!(results[2]["title"], "Alpha");
        assert_eq!(results[2]["entity"]["id"], "person:alpha");
        assert_eq!(results[2]["entity"]["name"], "Alpha");
        assert_eq!(
            results[2]["entity"]["alternateName"][0], "Al",
            "the names it answers to come with it"
        );
        assert_eq!(results[2]["edges"][0]["object"], "org:guild");
        assert_eq!(results[2]["snippet"], "…allergic to penicillin…");
    }
}
