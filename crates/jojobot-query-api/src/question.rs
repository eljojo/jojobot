//! The prepared question shared by the verbs and their other renderers.

use jojobot_domain::memory::types::DeclaredType;
use jojobot_domain::memory::{EntityId, EntityKind, MemoryError, graph, readiness};

/// **The two narrowings a selection cannot express.**
#[derive(Default)]
pub struct Asks {
    /// The winning claim behind each folded key.
    pub backing: bool,
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
    /// **Only what defrag has not reviewed**, oldest change first. See
    /// the shared unreviewed pass.
    pub unreviewed: bool,
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
                masked: Default::default(),
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
                unreviewed: asked.unreviewed,
                ..graph::Selection::default()
            },
            graph::Include {
                facts: false,
                prose: false,
                stood_for: false,
                status: None,
            },
            None,
            None,
        );
        // **The day the view's input named, read by the parser the verb reads it
        // with**, or the run's own when the view names none or the input was left
        // out. A day that is no day is refused, never read as today.
        let overdue = if asked.overdue {
            let named = parse_date(asked.as_of.as_deref()).map_err(MemoryError::InvalidQuery)?;
            Some(named.unwrap_or(today))
        } else {
            None
        };
        question.asks = Asks {
            backing: false,
            overdue,
            measure_from: asked.overdue.then_some(today),
            ready: asked.ready.map(|ask| Readiness { ask, named: None }),
            unreviewed: asked.unreviewed,
        };
        if question.query.select.narrows_nothing() {
            return Err(MemoryError::ViewNarrowsNothing {
                view: view.to_string(),
            });
        }
        Ok(question)
    }
}

/// Parse the optional day once for the prepared question and its callers.
pub fn parse_date(raw: Option<&str>) -> Result<Option<jiff::civil::Date>, String> {
    let Some(named) = raw.map(str::trim).filter(|day| !day.is_empty()) else {
        return Ok(None);
    };
    named
        .parse()
        .map(Some)
        .map_err(|error| format!("date must be YYYY-MM-DD, got '{named}': {error}"))
}
