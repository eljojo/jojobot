//! Resolve recall arguments into the shared prepared question.

use super::*;

pub(super) struct Prepared {
    pub(super) args: RecallArgs,
    pub(super) question: view_run::Question,
    pub(super) a_whole_read: bool,
    pub(super) asked_answers: Option<DeclaredType>,
    pub(super) asked_fits: Option<DeclaredType>,
    pub(super) type_displaced: Vec<(String, Option<jojobot_domain::memory::types::Displaced>)>,
    pub(super) today: jiff::civil::Date,
    pub(super) as_of: Option<jiff::civil::Date>,
    pub(super) keys: Vec<String>,
    pub(super) most_values: usize,
    pub(super) history_most: usize,
    pub(super) want_charter: bool,
    pub(super) asked_prose: bool,
    pub(super) only_keys: Option<Vec<String>>,
    pub(super) keys_from_view: bool,
    pub(super) wanted_status: Option<jojobot_domain::memory::FactStatus>,
    pub(super) near: Option<graph::Nearness>,
    pub(super) built_on_address: Option<FactAddress>,
    pub(super) stale_working_on: std::collections::BTreeSet<EntityId>,
    pub(super) readiness_asked: Option<readiness::Ask>,
    pub(super) view_named: Option<String>,
    pub(super) where_is_the_views: bool,
}

impl Jojobot {
    pub(super) async fn prepare_recall_question(
        &self,
        args: RecallArgs,
        asked_by: Option<EntityId>,
    ) -> Result<Result<Prepared, CallToolResult>, McpError> {
        // **Whether this read takes a whole kind**, judged on the call as it was sent,
        // before a view fills anything in.
        let a_whole_read = crate::teaching::is_a_whole_read(&args);
        // **A view fills the call in before anything reads it.** What the view
        // says is what this call asks; what the caller sent beside it wins, so
        // a view is a starting point rather than a cage. **Done before
        // anything below resolves a name to a declaration or a shape to a
        // link**, because those read `args` as the caller left it — a view
        // filling in `answers_type` or `follow` after that point would fill
        // in fields nothing downstream ever looks at again.
        //
        // **The view's own key filters ride separately from `args`**, because
        // `RecallArgs.fields` is the caller-facing shape (`KeyFilterArgs`) and
        // a view's are already the domain shape — carrying them through as
        // `RecallArgs` would mean converting them twice for no reason. Used
        // only when the caller named no `fields` of its own; a caller that did
        // meant that filter, exactly as every other view-filled argument works.
        // **Whether the caller named `keys` itself**, taken before a view fills
        // the call in: the note an object leaves when it is narrowed tells a
        // different way back for keys a view supplied.
        let caller_named_keys = args.keys.is_some();
        // **Whether the question's `where` is the view's own**: the caller named
        // none, and the view fills one in. A refusal of that question names the
        // view, because the caller never wrote the line it is about.
        let view_named = args.view.clone();
        let caller_named_scope = args.scope.is_some();
        let where_is_the_views = args.expression.is_none();
        let (args, view_filters, readiness_asked, view_overdue) = match args.view.clone() {
            None if args.inputs.is_some() => {
                return memory_declined(
                    "recall",
                    MemoryError::InvalidQuery(
                        "inputs fill the inputs of a view, so send `view` beside them: recall \
                         a view's handle (`subject: \"view:ready\"`) to read what it takes"
                            .into(),
                    ),
                )
                .map(Err);
            }
            None => (args, Vec::new(), None, false),
            Some(named) => {
                let day = self.dated(None, args.sid.as_deref()).await?;
                match self.asked_by_name(&named, args, day).await {
                    Ok(filled) => filled,
                    Err(ViewStop::Said(refused)) => return Ok(Err(*refused)),
                    Err(ViewStop::Outage(e)) => return memory_declined("recall", e).map(Err),
                }
            }
        };
        // **Readiness is a question about work, and only work answers it.** The
        // view selects the work kind; a caller that sent another kind beside it,
        // or named a subject of another kind, would be handed `ready` for things
        // that have no dependencies to wait on.
        if readiness_asked.is_some() {
            if args
                .kind
                .as_deref()
                .map(parse_kind)
                .transpose()?
                .is_some_and(|kind| kind != EntityKind::WORK)
            {
                return memory_declined(
                    "recall",
                    MemoryError::InvalidQuery(
                        "the ready view answers for the work kind only: leave kind off, or send \
                         kind: work"
                            .into(),
                    ),
                )
                .map(Err);
            }
            if let Some(subject) = args.subject.as_deref()
                && EntityId::person(subject).kind_token() != EntityKind::WORK.as_token()
            {
                return memory_declined(
                    "recall",
                    MemoryError::InvalidQuery(format!(
                        "the ready view answers for the work kind only, and '{subject}' is not \
                         work: name a piece of work in subject, or leave subject off"
                    )),
                )
                .map(Err);
            }
        }
        // **A readiness ask with no kind means the work kind.** Only work is ever
        // ready or blocked, and a walk over every kind would count a person as
        // ready, so a view that asks it and selects nothing is asked of work. A
        // call that named a subject keeps the one it named.
        let args = if readiness_asked.is_some() && args.kind.is_none() && args.subject.is_none() {
            RecallArgs {
                kind: Some(EntityKind::WORK.as_token().to_string()),
                ..args
            }
        } else {
            args
        };
        // **The name is resolved to its declaration here, once**, exactly as
        // `search` resolves it: everything below takes the keys rather than
        // the name, and a name nobody declared is answered where the roster to
        // offer instead is in reach.
        let mut type_displaced = Vec::new();
        let answers_type = match &args.answers_type {
            None => None,
            Some(wanted) => match self.declared(wanted, "recalled").await? {
                Ok((declared, displaced)) => {
                    type_displaced.push((wanted.clone(), displaced));
                    Some(declared)
                }
                Err(refused) => return Ok(Err(refused)),
            },
        };
        // Resolved here for the same reason `answers_type` is, and separately
        // from it: the two narrow different halves of one call, and a name that
        // names no type has to be answered where the roster to offer instead is
        // in reach.
        let fits_type = match args.follow.as_ref().and_then(|f| f.fits_type.as_ref()) {
            None => None,
            Some(wanted) => match self.declared(wanted, "walked to").await? {
                Ok((declared, displaced)) => {
                    type_displaced.push((wanted.clone(), displaced));
                    Some(declared)
                }
                Err(refused) => return Ok(Err(refused)),
            },
        };
        // **Kept for the answer.** Both resolved declarations move into the
        // query below, and an answer that selects nothing under them has to be
        // able to say what they are.
        let asked_answers = answers_type.clone();
        let asked_fits = fits_type.clone();
        // **Two link vocabularies, and a call names one of them.** An edge
        // shape and a relation describe different things — a link nobody typed,
        // and a link a declaration made — so a call carrying both is refused
        // rather than one of them being picked.
        if let Some(f) = args.follow.as_ref()
            && f.shape.is_some()
            && f.relation.is_some()
        {
            return memory_declined(
                "recall",
                MemoryError::InvalidQuery(
                    "follow a shape or a relation, not both: a shape is one of the five names for \
                     a link nobody typed, and a relation is a key some type declared to hold a \
                     reference"
                        .into(),
                ),
            )
            .map(Err);
        }
        // **The one clock read, taken here whether or not a question named a
        // day.** What is held is asked as of a day like everything else, so a
        // call that named one for `overdue` asks about the same day here, and
        // the answer says which day it used either way.
        let today = self.dated(None, args.sid.as_deref()).await?;
        // **An expression is read here, once, against the asker's day**, so a
        // relative date means that day and a mistake is refused before any read.
        let keeping_expression = match args.follow.as_ref().and_then(|f| f.keeping.as_ref()) {
            Some(Keeping::Expression(text)) => match filter::expression::parse(text, today) {
                Ok(tree) => Some(tree),
                Err(refused) => return memory_declined("recall", refused).map(Err),
            },
            _ => None,
        };
        let where_expression = match args.expression.as_deref() {
            None => None,
            Some(text) => match self.parse_naming_members(text, today).await {
                Ok(tree) => Some(tree),
                Err(refused) => return memory_declined("recall", refused).map(Err),
            },
        };
        let where_scope = parse_scope(args.scope.as_deref())?;
        // **A `scope` the caller sent says how a `where` is asked**, so beside no
        // `where` (the view's own counts) it would be ignored without a word. A
        // walk's `keeping` expression has a refusal of its own just below.
        if caller_named_scope && where_expression.is_none() && keeping_expression.is_none() {
            return memory_declined(
                "recall",
                MemoryError::InvalidQuery(
                    "scope says how a where is asked, and this call has no where: send one, \
                     or leave scope off. fields carry a scope of their own"
                        .into(),
                ),
            )
            .map(Err);
        }
        // **A walk's expression is asked of what each reached thing holds now.**
        // The call's `scope` would be silently dropped beside it, so it is
        // refused, naming the form that does take a record scope.
        if keeping_expression.is_some() && where_scope == graph::Scope::Record {
            return memory_declined(
                "recall",
                MemoryError::InvalidQuery(
                    "a keeping expression is asked of what each reached thing holds now, and \
                     scope: record does not apply to it. To keep what a walk reaches by one \
                     record, send keeping as a list of key filters, each with scope: record"
                        .into(),
                ),
            )
            .map(Err);
        }
        let order = match args.order.as_ref() {
            None => None,
            Some(asked) => match filter::Order::asked(&asked.key, asked.direction.as_deref()) {
                Ok(order) => Some(order),
                Err(refused) => return memory_declined("recall", refused).map(Err),
            },
        };
        let follow = args
            .follow
            .as_ref()
            .map(|f| -> Result<graph::Follow, McpError> {
                Ok(graph::Follow {
                    along: match (&f.relation, &f.shape) {
                        (Some(relation), _) => graph::Along::Relation(relation.trim().to_string()),
                        (None, Some(shape)) => graph::Along::Edge(parse_shape_filter(shape)?),
                        (None, None) => graph::Along::AnyEdge,
                    },
                    // Kept as an Option all the way down, because a relation
                    // already says which way it goes and the domain refuses the
                    // pair — which it can only do if "unset" and "out" are
                    // still telling apart by the time it looks.
                    direction: f
                        .direction
                        .as_deref()
                        .map(|d| parse_direction(Some(d)))
                        .transpose()?,
                    depth: f.depth.map_or(1, |d| d as usize),
                    keeping: match (&f.keeping, &keeping_expression) {
                        (Some(Keeping::Filters(named)), _) => key_filters(named)?,
                        (_, Some(tree)) => vec![graph::FieldFilter::expression(
                            tree.clone(),
                            graph::Scope::Thing,
                        )],
                        _ => Vec::new(),
                    },
                    fits_type: fits_type.clone(),
                })
            })
            .transpose()?;
        // **The one clock read, and only when the question needs it.** The
        // domain reads no clock at all: it is handed the date and stays a
        // function of its arguments, so *what is overdue as of next Friday* is
        // the same code path as *what is overdue now*.
        let as_of = match &args.overdue {
            None => None,
            Some(overdue) => Some(
                self.dated(overdue.as_of.as_deref(), args.sid.as_deref())
                    .await?,
            ),
        };
        // Every key some declaration made a reference — what makes a value a
        // link. Read once, here, so the ranking stays a function of what it is
        // handed.
        let keys = match self.memory.declared_types().await {
            Ok(declared) => graph::reference_keys(&declared),
            Err(e) => return memory_declined("recall", e).map(Err),
        };
        // The same window the writes behind a key get, for the same reason: a
        // key with a thousand spellings would otherwise bury the caller who
        // asked one narrow question.
        let most_values = args
            .values_most
            .map_or(graph::WRITES_SHOWN, |most| most as usize);
        // **A charter IS the page**, so asking for one asks the walk for prose
        // whether or not the caller wanted it under that name. What the caller
        // asked for is what is SHIPPED: the charter replaces the page rather
        // than arriving beside it, because a reader who did not ask for the
        // page would otherwise be sent the same text twice.
        let history_most = args
            .history_most
            .map_or(graph::WRITES_SHOWN, |most| most as usize);
        // **Two words and nothing between them**, so a typo is refused rather
        // than read as "every record".
        let wanted_status = match args.status.as_deref().map(str::trim) {
            None | Some("") => None,
            Some("active") => Some(jojobot_domain::memory::FactStatus::Active),
            Some("archived") => Some(jojobot_domain::memory::FactStatus::Archived),
            Some(other) => {
                return memory_declined(
                    "recall",
                    MemoryError::InvalidQuery(format!(
                        "status is active or archived, and '{other}' is neither — leave it out \
                         to read every record"
                    )),
                )
                .map(Err);
            }
        };
        if args.limit == Some(0) {
            return memory_declined(
                "recall",
                MemoryError::InvalidQuery(
                    "limit must be at least 1: it is the size of a page, and a page of nothing \
                     reads nothing — leave it out to read whatever fits"
                        .into(),
                ),
            )
            .map(Err);
        }
        let want_charter = args.charter.unwrap_or(false);
        let asked_prose = args.prose.unwrap_or(false);
        // **Which keys of each object's fields come back**, when the call or
        // the view it named said. Kept as sent: matching is by key name.
        let only_keys = args.keys.clone();
        let keys_from_view = args.view.is_some() && !caller_named_keys && only_keys.is_some();
        // **Naming the keys of each record asks for the records.**
        let record_keys = args.record_keys.as_deref();
        let include = graph::Include {
            facts: args.facts.unwrap_or(false) || record_keys.is_some(),
            prose: asked_prose || want_charter,
            stood_for: args.stood_for.unwrap_or(false),
            status: wanted_status,
        };
        // **The same one clock read as `overdue`, and only when asked.** The
        // domain is handed the day and reads none itself.
        let near = match &args.near {
            None => None,
            Some(asked) => Some(graph::Nearness {
                day: self
                    .dated(asked.day.as_deref(), args.sid.as_deref())
                    .await?,
                within_days: asked.within_days.unwrap_or(NEAR_WINDOW),
                clock: parse_clock(asked.clock.as_deref())?,
            }),
        };
        // **One axis, two things it can trace.** Naming both asks two questions
        // of one answer, so it is refused rather than answered with whichever
        // the code happened to check first.
        let trace = match (args.history.as_deref(), args.history_record.as_deref()) {
            (Some(_), Some(_)) => {
                return memory_declined(
                    "recall",
                    MemoryError::InvalidQuery(
                        "history names a KEY and history_record names a RECORD, and they are the \
                         same axis: a call can trace one or the other. Name whichever you meant \
                         and drop the other"
                            .into(),
                    ),
                )
                .map(Err);
            }
            // Trimmed like every other key a caller names, so `donuts_eaten `
            // asks the question `donuts_eaten` answers.
            (Some(key), None) => Some(graph::History {
                of: graph::Trace::Key(key.trim().to_string()),
                most: history_most,
            }),
            // **A thing's own handle has no `#`**, and names the thing rather
            // than one of its records.
            (None, Some(handle)) if !handle.contains('#') => Some(graph::History {
                of: graph::Trace::Entity(EntityId::person(handle.trim())),
                most: history_most,
            }),
            (None, Some(address)) => Some(graph::History {
                of: graph::Trace::Record(match FactAddress::parse(address) {
                    Ok(parsed) => parsed,
                    Err(refused) => return memory_declined("recall", refused).map(Err),
                }),
                most: history_most,
            }),
            (None, None) => None,
        };
        // **Parsed here, once, so the fill below and the walk further down
        // share the same parse** — never a second `FactAddress::parse` of
        // the same string.
        let built_on_address = match args.built_on.as_deref() {
            None => None,
            Some(address) => match FactAddress::parse(address) {
                Ok(parsed) => Some(parsed),
                Err(refused) => return memory_declined("recall", refused).map(Err),
            },
        };
        // **A thing named for its history is the subject**, as an address is: a
        // `subject` beside it has to be the same thing.
        let entity_traced = match trace.as_ref().map(|history| &history.of) {
            Some(graph::Trace::Entity(named)) => Some(named.clone()),
            _ => None,
        };
        if let Some(named) = &entity_traced
            && let Some(asked) = args.subject.as_deref().map(EntityId::person)
            && &asked != named
        {
            return memory_declined(
                "recall",
                MemoryError::InvalidQuery(format!(
                    "history_record names {named}, a thing, and asks for {asked}. Drop the \
                     subject — the handle names its own — or name the thing you meant"
                )),
            )
            .map(Err);
        }
        let query = view_run::Question::new(
            graph::Selection {
                subject: args
                    .subject
                    .as_deref()
                    .map(EntityId::person)
                    .or_else(|| entity_traced.clone()),
                kind: args.kind.as_deref().map(parse_kind).transpose()?,
                parent: args.parent.as_deref().map(EntityId::person),
                answers_type,
                candidates: None,
                candidates_exact: false,
                ordered_head: None,
                fields: {
                    // **The caller's own `fields` win whole, never merged item
                    // by item.** A caller that named a filter meant that
                    // filter; the view's own is a starting point, not
                    // something to add to. This is the one place that decides.
                    let mut asked = match &args.fields {
                        Some(named) => key_filters(named)?,
                        None => view_filters,
                    };
                    asked.extend(
                        where_expression
                            .map(|tree| graph::FieldFilter::expression(tree, where_scope)),
                    );
                    asked
                },
                order,
                near,
                asked_by,
                usage_reach: Default::default(),
                owed: view_overdue,
                ready: readiness_asked.is_some(),
                unreviewed: args.unreviewed == Some(true),
            },
            include,
            follow,
            trace,
        )
        .query;
        // 🚨 **An address is a selection.** A record's address contains its
        // subject, and a call that named only a record — to trace, or to walk
        // lineage from — was refused for naming nothing to recall, which is
        // what the first callers of these arguments met on their first use
        // (rule 236). The subject is filled in from the address when the
        // call chose nothing else, and the condition is the refusal's own,
        // so the two cannot come to disagree. `history_record` and
        // `built_on` share this one path rather than each carrying a copy
        // of it.
        let mut query = query;
        // **A bot's `working_on` is read through the one gate before the walk**, and
        // left out of what the walk reads for a bot whose session is gone, so a
        // filter, an order and the objects all see the same bot.
        let stale_working_on = self.stale_working_on_bots().await;
        query.masked = super::working_on::masked_working_on(&stale_working_on);
        // **Whether `subject` is the caller's own word, or a fill this loop
        // is about to do itself.** The two read the same on `query.select`
        // once filled, and the refusal below has to tell them apart: a
        // caller-supplied subject disagreeing with an address is a caller
        // mistake worth naming as "asked for"; a subject THIS LOOP filled
        // from an earlier address disagreeing with a later one is a
        // different mistake — two addresses on two different things — and
        // saying the caller "asked for" the first address's entity would be
        // inventing a subject nobody sent.
        let subject_was_explicit = query.select.subject.is_some();
        let mut filled_by: Option<(&str, FactAddress)> = None;
        let selecting_addresses = [
            query.history.as_ref().and_then(|h| match &h.of {
                graph::Trace::Record(address) => Some(("history_record", address.clone())),
                graph::Trace::Key(_) | graph::Trace::Entity(_) => None,
            }),
            built_on_address
                .clone()
                .map(|address| ("built_on", address)),
        ];
        for (arg_name, address) in selecting_addresses.into_iter().flatten() {
            match &query.select.subject {
                // **The two DISAGREEING is the case worth a refusal.** A
                // caller whose arguments point at two different things has made
                // a mistake, and answering one of them quietly picks for them.
                Some(named) if named != &address.home && subject_was_explicit => {
                    let named = named.clone();
                    return memory_declined(
                        "recall",
                        MemoryError::InvalidQuery(format!(
                            "{arg_name} names {address}, which is a record on {}, and asks for \
                             {named}. Drop the subject — the address names its own — or name the \
                             record you meant on {named}",
                            address.home,
                        )),
                    )
                    .map(Err);
                }
                // **The disagreement is between two ADDRESSES, not between
                // an address and anything the caller sent.** `filled_by` is
                // always `Some` here: `subject_was_explicit` is false, so the
                // only way `query.select.subject` holds a value that
                // disagrees is that an earlier iteration of this same loop
                // set it.
                Some(named) if named != &address.home => {
                    let (earlier_arg, earlier_address) = filled_by
                        .clone()
                        .expect("a subject that disagrees with no caller input was filled here");
                    return memory_declined(
                        "recall",
                        MemoryError::InvalidQuery(format!(
                            "{earlier_arg} names {earlier_address}, a record on {}, and \
                             {arg_name} names {address}, a record on {} — two different things. \
                             Narrow to whichever record you meant, or send a subject naming the \
                             one you want",
                            earlier_address.home, address.home,
                        )),
                    )
                    .map(Err);
                }
                None if query.select.narrows_nothing() => {
                    query.select.subject = Some(address.home.clone());
                    filled_by = Some((arg_name, address.clone()));
                }
                _ => {}
            }
        }

        // **A view that narrows nothing is refused by name.** The call chose
        // nothing beside it either, so the whole store is what it asks for. The
        // generic refusal below lists `view` among the ways to choose, which sends
        // a caller who named a view round in a circle; this one names the view and
        // the keys that would narrow it. Checked after the address fill above, so
        // a view traced by a record's address is not refused.
        if let Some(named) = &view_named
            && query.select.narrows_nothing()
        {
            return memory_declined(
                "recall",
                MemoryError::ViewNarrowsNothing {
                    view: view_handle(named).to_string(),
                },
            )
            .map(Err);
        }

        let named = query.select.subject.clone();
        let question = view_run::Question {
            query,
            asks: view_run::Asks {
                backing: args.backing.unwrap_or(false),
                overdue: as_of,
                measure_from: (view_overdue && as_of.is_some()).then_some(today),
                ready: readiness_asked.map(|ask| view_run::Readiness { ask, named }),
                unreviewed: args.unreviewed == Some(true),
            },
        };
        Ok(Ok(Prepared {
            args,
            question,
            a_whole_read,
            asked_answers,
            asked_fits,
            type_displaced,
            today,
            as_of,
            keys,
            most_values,
            history_most,
            want_charter,
            asked_prose,
            only_keys,
            keys_from_view,
            wanted_status,
            near,
            built_on_address,
            stale_working_on,
            readiness_asked,
            view_named,
            where_is_the_views,
        }))
    }
}
