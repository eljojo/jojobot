//! **A view's question, built once and answered once**, in one place.
//!
//! A view is answered twice: by the `recall` verb and by the page of the view
//! record. The two once ran their own copies of the passes after the selection
//! and disagreed, so a page listed work that could not start; then they built
//! their own selections and disagreed again, so a page listed what the view's
//! type left out. Both call these functions now, and neither keeps a second
//! copy (rule 444, rule 473).
//!
//! [`Question::new`] is the one place a selection becomes a query,
//! [`Question::of_view`] is the one reading of a stored view as a question
//! with no caller beside it, and [`select`] and [`narrow`] (which [`answer`]
//! runs in turn) are the one executor.
//!
//! What stays with each caller is what it does with the answer: the verb fits
//! the objects to its ceiling and renders them as JSON, and the page renders a
//! table. Nothing here reads a caller's arguments or writes a word of an answer.

use std::collections::BTreeMap;

use jojobot_domain::attention;
use jojobot_domain::memory::graph;
use jojobot_domain::memory::readiness;
use jojobot_domain::memory::search::DocScan;
use jojobot_domain::memory::types::DeclaredType;
use jojobot_domain::memory::{EntityId, EntityKind, Memory, MemoryError};

use super::ready::{self, Verdict};

/// **The two narrowings a selection cannot express.**
#[derive(Default)]
pub struct Asks {
    /// **Only what is owed on this day**, oldest due first.
    pub overdue: Option<jiff::civil::Date>,
    /// **The day each kept object's lateness is measured from**, when it is not
    /// the day asked about. A view that asks what falls due by a later day
    /// measures from the run's own day, so an object not yet due says how many
    /// days away it is instead of a lateness below zero. `None` measures from the
    /// day asked about, as a `recall` of its own does.
    pub measure_from: Option<jiff::civil::Date>,
    /// **What of the work to keep, by readiness**: only what can be started, or
    /// the whole plan.
    pub ready: Option<Readiness>,
}

/// **A readiness question.**
pub struct Readiness {
    pub ask: readiness::Ask,
    /// The one thing a call named, which always comes back so a caller can read
    /// why.
    pub named: Option<EntityId>,
}

/// **A question, ready to be answered**: a query for the graph and its
/// narrowings.
pub struct Question {
    /// What the graph is asked. A caller may adjust it before the walk, as the
    /// verb does when an address or an index narrows the selection.
    pub query: graph::GraphQuery,
    /// What is kept of the objects the query finds.
    pub asks: Asks,
}

impl Question {
    /// **The one place a selection becomes a query.** No narrowing is asked: a
    /// caller adds `overdue` and `ready` to the question it gets back.
    pub fn new(
        select: graph::Selection,
        include: graph::Include,
        follow: Option<graph::Follow>,
        history: Option<graph::History>,
    ) -> Question {
        Question {
            query: graph::GraphQuery {
                select,
                include,
                follow,
                history,
            },
            asks: Asks::default(),
        }
    }

    /// **A stored view read as a question, with nothing beside it.** The kind
    /// is the one `selects` resolved to, and `answers_type` is the declaration
    /// its name resolved to; both are the caller's to look up, because the
    /// verb refuses a call where the page says the view cannot be run. `parent`
    /// is the thing the view lists the children of, once its inputs are filled.
    ///
    /// **What is asked of each object is the objects and nothing of each**: a
    /// view's `shows` says what of each one comes back on a read, and a
    /// listing of the answer needs none of it. `today` is the day a relative
    /// date in `where` and an overdue ask are read against.
    ///
    /// **A view that asks what is ready, with no kind, is asked of work**, as the
    /// verb asks it: only work is ready or blocked. **A view that narrows nothing
    /// is refused** with [`MemoryError::ViewNarrowsNothing`], decided by the same
    /// [`graph::Selection::narrows_nothing`] the verb asks, so a view the verb
    /// answers alone is answered here and one it refuses alone is refused here.
    pub fn of_view(
        view: &EntityId,
        asked: &graph::Asked,
        kind: Option<EntityKind>,
        answers_type: Option<DeclaredType>,
        parent: Option<EntityId>,
        today: jiff::civil::Date,
    ) -> Result<Question, MemoryError> {
        let (fields, order) = (asked.filters(today)?, asked.ordering()?);
        let kind = kind.or_else(|| asked.ready.is_some().then_some(EntityKind::WORK));
        let mut question = Question::new(
            graph::Selection {
                kind,
                answers_type,
                parent,
                fields,
                order,
                owed: asked.overdue,
                ready: asked.ready.is_some(),
                ..graph::Selection::default()
            },
            graph::Include {
                facts: false,
                prose: false,
                stood_for: false,
            },
            None,
            None,
        );
        question.asks = Asks {
            // The day the view's input named, or the run's own when the view
            // names none or the input was left out.
            overdue: asked.overdue.then(|| {
                asked
                    .as_of
                    .as_deref()
                    .and_then(|day| day.parse().ok())
                    .unwrap_or(today)
            }),
            measure_from: asked.overdue.then_some(today),
            ready: asked.ready.map(|ask| Readiness { ask, named: None }),
        };
        if question.query.select.narrows_nothing() {
            return Err(MemoryError::ViewNarrowsNothing {
                view: view.to_string(),
            });
        }
        Ok(question)
    }
}

/// **What the narrowings are read against**, which the caller supplies so no
/// answer here hardcodes the carriers a build ships.
pub struct Context<'a> {
    /// The carriers that say when a thing falls due.
    pub carriers: &'a [&'a dyn attention::Carrier],
    /// The operator's own status words. Read only for an overdue question.
    pub instance: &'a BTreeMap<String, String>,
}

/// **What the narrowings found**, aligned with the objects they kept.
pub struct Narrowed {
    /// Present when the question asked what is owed.
    pub owed: Option<OwedPass>,
    /// Present when the question asked what is ready.
    pub ready: Option<ReadyPass>,
}

/// **The selection, walked.** `sessions` are the runs the caller joins to the
/// walk, and `window` is the part of a browse the caller will serve.
pub async fn select(
    memory: &dyn Memory,
    sessions: &[DocScan],
    query: &graph::GraphQuery,
    window: Option<graph::Window<'_>>,
) -> Result<graph::Selected, MemoryError> {
    graph::walk_in_window(memory, sessions, query, window).await
}

/// **The narrowings a selection cannot express**, run in the order the verb
/// has always run them: what is owed, then what is ready.
pub async fn narrow(
    memory: &dyn Memory,
    found: &mut Vec<graph::Object>,
    asks: &Asks,
    context: &Context<'_>,
) -> Result<Narrowed, MemoryError> {
    let owed = asks.overdue.map(|as_of| {
        keep_owed(
            found,
            context.carriers,
            context.instance,
            as_of,
            asks.measure_from.unwrap_or(as_of),
        )
    });
    let ready = match &asks.ready {
        Some(asked) => Some(keep_ready(memory, found, asked).await?),
        None => None,
    };
    Ok(Narrowed { owed, ready })
}

/// **The part of an already-narrowed answer a window serves**, cut where
/// [`graph::walk_in_window`] cuts it: both spend a [`graph::Room`].
///
/// A window cut during the walk is spent on what the selection found, and a
/// narrowing that runs after it removes objects the window already paid for:
/// a page of finished work crowds out the one ready piece behind it. When the
/// question narrows by readiness the walk is run whole, the narrowing runs, and
/// the window is applied here, so it is spent on the objects that remain.
///
/// `offset` objects stand before the window and are dropped. Of the rest each
/// adds `size_of` to the room, and the object that takes the total past the room
/// is kept, as the walk keeps it. `aligned` are lists with one entry per object,
/// cut the same way. Returns how many stood before the window and how many stand
/// after it.
pub fn cut_to_window<T>(
    found: &mut Vec<graph::Object>,
    aligned: &mut [&mut Vec<T>],
    window: &graph::Window<'_>,
) -> (usize, usize) {
    let sizes: Vec<usize> = found
        .iter()
        .map(|object| (window.size_of)(object))
        .collect();
    let total = sizes.len();
    let (skipped, passed) = graph::window_bounds(&sizes, window.offset, window.budget);
    found.truncate(passed);
    found.drain(..skipped);
    for list in aligned {
        list.truncate(passed);
        list.drain(..skipped);
    }
    (skipped, total - passed)
}

/// **Where a question failed**, because a caller says different things about a
/// selection it could not read and a dependency it could not read.
pub enum Failed {
    /// The walk over the records the question selects.
    Selection(MemoryError),
    /// The read of the work a ready question depends on.
    Readiness(MemoryError),
}

/// **A question, answered**: the selection and then its narrowings.
pub struct Answered {
    /// The objects the question found, after the narrowings.
    pub selected: graph::Selected,
    /// What the narrowings found.
    pub narrowed: Narrowed,
}

/// **Answer a question** as the page of a view does: the whole selection, no
/// window, no runs joined.
pub async fn answer(
    memory: &dyn Memory,
    question: &Question,
    context: &Context<'_>,
) -> Result<Answered, Failed> {
    let mut selected = select(memory, &[], &question.query, None)
        .await
        .map_err(Failed::Selection)?;
    let narrowed = narrow(memory, &mut selected.objects, &question.asks, context)
        .await
        .map_err(Failed::Readiness)?;
    Ok(Answered { selected, narrowed })
}

/// **What an object owes, asked of its kind too**: a finished work item or
/// project owes nothing — see [`attention::owed_as`].
pub(crate) fn owed_by(
    carriers: &[&dyn attention::Carrier],
    object: &graph::Object,
    instance: &BTreeMap<String, String>,
) -> attention::Due {
    attention::owed_as(
        object.entity.kind.as_token(),
        carriers,
        &object.fields,
        instance,
    )
}

/// **What the overdue pass found**, aligned with the objects it kept.
pub struct OwedPass {
    /// How many objects the pass dropped.
    pub excluded: usize,
    /// **Why the dropped objects were dropped, and of what kind.** The counts
    /// add up to [`OwedPass::excluded`]: this groups the same objects and
    /// counts none of them differently.
    pub excluded_because: BTreeMap<LeftOut, BTreeMap<String, usize>>,
    /// How many days past the day it is measured from each kept object fell
    /// due (below zero: how many days ahead), read off the same `Due` the order
    /// came from, so the number cannot disagree with the position it is in.
    pub by_days: Vec<Option<i64>>,
    /// Why each kept object that cannot say when it falls due cannot.
    pub unreadable_because: Vec<Option<attention::Unreadable>>,
}

/// **Why the owed pass left an object out.** An object is in exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LeftOut {
    /// It has a due day and the day is after the day asked about.
    NotDueYet,
    /// It has a cadence and has never been checked in, so its first cycle has
    /// not started.
    NotYetOpened,
    /// A work item or project that is finished, or a thing its carrier reads as
    /// ended, owes nothing.
    Finished,
    /// Its carrier answers that nothing falls due: it carries none of the keys a
    /// due day is worked out from, or its kind has no carrier.
    NoSchedule,
}

impl LeftOut {
    /// Every reason there is.
    pub const ALL: [LeftOut; 4] = [
        LeftOut::NotDueYet,
        LeftOut::NotYetOpened,
        LeftOut::Finished,
        LeftOut::NoSchedule,
    ];

    /// The word a served answer uses for this reason.
    pub fn token(self) -> &'static str {
        match self {
            LeftOut::NotDueYet => "not_due_yet",
            LeftOut::NotYetOpened => "not_yet_opened",
            LeftOut::Finished => "finished",
            LeftOut::NoSchedule => "no_schedule",
        }
    }
}

/// **Why this object owes nothing on the day asked about.** Asked of the
/// objects the pass dropped only; the finished decision is
/// [`attention::owes_as`]'s, the same one the pass itself went through.
fn left_out_because(
    carriers: &[&dyn attention::Carrier],
    object: &graph::Object,
    instance: &BTreeMap<String, String>,
) -> LeftOut {
    let kind = object.entity.kind.as_token();
    match attention::owes_as(kind, carriers, &object.fields, instance) {
        attention::Owes::Finished => LeftOut::Finished,
        attention::Owes::Due(attention::Due::On(_)) => LeftOut::NotDueYet,
        attention::Owes::Due(attention::Due::NotYetOpened) => LeftOut::NotYetOpened,
        attention::Owes::Due(attention::Due::Never) => LeftOut::NoSchedule,
        // `owed_on` keeps an unreadable thing, so the pass never drops one.
        attention::Owes::Due(attention::Due::Unreadable) => {
            unreachable!("an unreadable schedule is owed, so it is never left out")
        }
    }
}

/// **Keep what is owed on `as_of`, oldest due first.**
///
/// The pass compares a moment to a day and computes none of them: which moment
/// a thing falls due at is its carrier's answer, so a kind no carrier speaks for
/// owes nothing, which is what keeps a person out of an answer about what is
/// late. `instance` is the operator's own status words, so a word mapped to
/// `done` finishes a work item or project here as it does in the verbs.
pub fn keep_owed(
    found: &mut Vec<graph::Object>,
    carriers: &[&dyn attention::Carrier],
    instance: &BTreeMap<String, String>,
    as_of: jiff::civil::Date,
    measured_from: jiff::civil::Date,
) -> OwedPass {
    let before = found.len();
    let mut excluded_because: BTreeMap<LeftOut, BTreeMap<String, usize>> = BTreeMap::new();
    found.retain(|object| {
        let owed = owed_by(carriers, object, instance).owed_on(as_of);
        if !owed {
            *excluded_because
                .entry(left_out_because(carriers, object, instance))
                .or_default()
                .entry(object.entity.kind.as_token().to_string())
                .or_default() += 1;
        }
        owed
    });
    let excluded = before - found.len();
    found.sort_by_key(|object| owed_by(carriers, object, instance).staleness());
    let by_days = found
        .iter()
        .map(|object| owed_by(carriers, object, instance).days_overdue(measured_from))
        .collect();
    let unreadable_because = found
        .iter()
        .map(|object| {
            matches!(
                owed_by(carriers, object, instance),
                attention::Due::Unreadable
            )
            .then(|| attention::unreadable_because(carriers, &object.fields))
            .flatten()
        })
        .collect();
    OwedPass {
        excluded,
        excluded_because,
        by_days,
        unreadable_because,
    }
}

/// **What the readiness pass found**, aligned with the objects it kept.
pub struct ReadyPass {
    /// The verdict on each kept object.
    pub verdicts: Vec<Verdict>,
    /// How many pieces of work the pass left out.
    pub left_out: usize,
}

/// **Keep the work the ask keeps**: what can be started, or the whole plan, see
/// [`readiness::Ask`]. Asked of the work the selection found, against the things
/// its `depends_on` names. Work the ask does not keep is left out and counted,
/// **except the one thing a call named**, which always comes back so a caller
/// can read why. There is one readiness question for both asks.
pub async fn keep_ready(
    memory: &dyn Memory,
    found: &mut Vec<graph::Object>,
    asked: &Readiness,
) -> Result<ReadyPass, MemoryError> {
    let verdicts = {
        let asked: Vec<ready::Asked<'_>> = found
            .iter()
            .map(|object| ready::Asked {
                kind: object.entity.kind.as_token(),
                fields: &object.fields,
                archived: object.entity.archived.is_some(),
            })
            .collect();
        ready::verdicts_of(memory, &asked).await?
    };
    let mut kept = Vec::with_capacity(found.len());
    let mut kept_verdicts = Vec::with_capacity(found.len());
    let mut left_out = 0usize;
    for (object, verdict) in found.drain(..).zip(verdicts) {
        if asked.ask.keeps(verdict.ready, &verdict.blockers)
            || asked.named.as_ref() == Some(&object.entity.id)
        {
            kept.push(object);
            kept_verdicts.push(verdict);
        } else {
            left_out += 1;
        }
    }
    *found = kept;
    Ok(ReadyPass {
        verdicts: kept_verdicts,
        left_out,
    })
}

/// **What a view that narrows nothing is told**, as the verb answers it and the
/// page shows it: the view, every key that would narrow it, and what else
/// narrows the call. One function, so the page cannot word it differently
/// (rule 498). A key written `$name` narrows only when its input is sent.
pub fn narrows_nothing_words(view: &str) -> String {
    let refused = MemoryError::ViewNarrowsNothing {
        view: view.to_string(),
    };
    format!(
        "Nothing was read: {refused}. A view narrows by one of these keys: {}. A key \
         written `$name` narrows only when its input is sent. Or ask with a subject, \
         kind, parent, answers_type or fields of your own beside the view, which win \
         over it.",
        graph::Narrowing::view_keys().join(", ")
    )
}
