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
use jojobot_domain::memory::entries;
use jojobot_domain::memory::graph;
use jojobot_domain::memory::search::DocScan;
use jojobot_domain::memory::{EntityId, EntityKind, Memory, MemoryError};

use super::list_ticks::TickRead;
use super::ready;
pub use jojobot_query_api::{
    Answered, Asks, Failed, LeftOut, Narrowed, OwedPass, Question, Readiness, ReadyPass, TickCell,
    Verdict,
};

/// Build the read facade from the two read dependencies, without the MCP
/// handler, its teaching ledger, or the composition root's application state.
pub fn handle(
    memory: std::sync::Arc<dyn Memory>,
    sessions: std::sync::Arc<dyn jojobot_domain::session::Sessions>,
) -> jojobot_query_api::QueryHandle {
    struct Reader {
        memory: std::sync::Arc<dyn Memory>,
        sessions: std::sync::Arc<dyn jojobot_domain::session::Sessions>,
        carriers: Vec<Box<dyn attention::Carrier>>,
    }
    #[async_trait::async_trait]
    impl jojobot_query_api::Answerer for Reader {
        async fn ask(&self, mut question: Question) -> Result<Answered, Failed> {
            let stale =
                super::working_on::stale_working_on_bots(&*self.memory, &*self.sessions).await;
            question
                .query
                .masked
                .extend(super::working_on::masked_working_on(&stale));
            let instance = if question.asks.overdue.is_some() {
                crate::orientation::instance_zone::instance_fields_of(&*self.memory)
                    .await
                    .map_err(Failed::Selection)?
            } else {
                BTreeMap::new()
            };
            let carriers: Vec<&dyn attention::Carrier> =
                self.carriers.iter().map(AsRef::as_ref).collect();
            let mut read = answer(
                &*self.memory,
                &question,
                &Context {
                    carriers: &carriers,
                    instance: &instance,
                },
            )
            .await?;
            read.stale_working_on = super::working_on::stale_in(&read.selected.objects, &stale);
            for object in &mut read.selected.objects {
                object
                    .fields
                    .retain(|key, _| !jojobot_domain::memory::review::is_bookkeeping_key(key));
                object.facts.retain(|fact| visible_fact(fact, false, false));
            }
            Ok(read)
        }
    }
    jojobot_query_api::QueryHandle::from(std::sync::Arc::new(Reader {
        memory,
        sessions,
        carriers: attention::shipped(),
    }) as std::sync::Arc<dyn jojobot_query_api::Answerer>)
}

pub(crate) fn visible_fact(fact: &jojobot_domain::memory::Fact, review: bool, notes: bool) -> bool {
    (review || !jojobot_domain::memory::review::is_review_mark(&fact.fields))
        && (notes || !jojobot_domain::memory::review::is_release_notes_record(&fact.fields))
}

pub(crate) async fn backing_of(
    memory: &dyn Memory,
    objects: &[graph::Object],
    masked: &BTreeMap<EntityId, Vec<String>>,
) -> BTreeMap<EntityId, Result<jojobot_query_api::Backing, MemoryError>> {
    let mut all = BTreeMap::new();
    for object in objects {
        let backing = memory.backing(&object.entity.id).await.map(|mut backing| {
            for key in masked.get(&object.entity.id).into_iter().flatten() {
                backing.remove(key);
            }
            backing
        });
        all.insert(object.entity.id.clone(), backing);
    }
    all
}

/// **What the narrowings are read against**, which the caller supplies so no
/// answer here hardcodes the carriers a build ships.
pub struct Context<'a> {
    /// The carriers that say when a thing falls due.
    pub carriers: &'a [&'a dyn attention::Carrier],
    /// The operator's own status words. Read only for an overdue question.
    pub instance: &'a BTreeMap<String, String>,
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
    let mut ready = match &asks.ready {
        Some(asked) => Some(keep_ready(memory, found, asked).await?),
        None => None,
    };
    if asks.unreviewed {
        // The review pass drops and re-sorts objects, so the verdicts, one per
        // object in the readiness pass's order, are re-keyed by object.
        let held: Vec<EntityId> = found.iter().map(|o| o.entity.id.clone()).collect();
        keep_unreviewed(found);
        if let Some(pass) = ready.as_mut() {
            let mut by_object: BTreeMap<EntityId, Verdict> =
                held.into_iter().zip(pass.verdicts.drain(..)).collect();
            pass.verdicts = found
                .iter()
                .filter_map(|o| by_object.remove(&o.entity.id))
                .collect();
        }
    }
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
    let mut ticks = Vec::with_capacity(selected.objects.len());
    for object in &selected.objects {
        let read = match object.entity.id.kind() == Some(EntityKind::LIST) {
            true => match memory.recall(&object.entity.id).await {
                Ok(held) => {
                    TickCell::Read(TickRead::of(&held, entries::Setting::of(&object.fields)))
                }
                Err(e) => {
                    tracing::warn!(error = %e, list = %object.entity.id, "the ticks of a list could not be read");
                    TickCell::Unreadable
                }
            },
            false => TickCell::NotAList,
        };
        ticks.push(read);
    }
    let backing = if question.asks.backing {
        backing_of(memory, &selected.objects, &question.query.masked).await
    } else {
        BTreeMap::new()
    };
    Ok(Answered {
        backing,
        stale_working_on: Default::default(),
        selected,
        narrowed,
        ticks,
    })
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
/// a thing falls due at is its carrier's answer, so a thing whose keys no carrier
/// speaks for owes nothing, which keeps a person who holds none of them out of
/// an answer about what is late. `instance` is the operator's own status words, so a word mapped to
/// `done` finishes a work item or project here as it does in the verbs.
/// **Keep what defrag has not reviewed, oldest change first.** The question is
/// the domain's one (`review::is_unreviewed`) asked of the two moments each
/// object's ages already hold; a thing the store holds no moments for (a run) is
/// not defrag's to review and is left out. A thing whose newest write is not
/// known sorts last.
pub fn keep_unreviewed(found: &mut Vec<graph::Object>) {
    found.retain(|object| {
        object
            .ages
            .as_ref()
            .is_some_and(jojobot_domain::memory::review::is_unreviewed)
    });
    found.sort_by_key(|object| {
        jojobot_domain::memory::review::oldest_change_first(object.ages.as_ref())
    });
}

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
        .map(|object| {
            (
                object.entity.id.clone(),
                owed_by(carriers, object, instance).days_overdue(measured_from),
            )
        })
        .collect();
    let unreadable_because = found
        .iter()
        .filter_map(|object| {
            matches!(
                owed_by(carriers, object, instance),
                attention::Due::Unreadable
            )
            .then(|| attention::unreadable_because(carriers, &object.fields))
            .flatten()
            .map(|why| (object.entity.id.clone(), why))
        })
        .collect();
    OwedPass {
        excluded,
        excluded_because,
        by_days,
        unreadable_because,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn overdue_as_of(as_of: Option<&str>) -> Result<Question, MemoryError> {
        Question::of_view(
            &EntityId("view:my-week".into()),
            &graph::Asked {
                overdue: true,
                as_of: as_of.map(str::to_string),
                ..graph::Asked::default()
            },
            Some(EntityKind::RHYTHM),
            None,
            None,
            "2026-03-01".parse().expect("a day"),
        )
    }

    /// **A view's day that is no day is refused, as the verb refuses it.** The page
    /// runs this function, and it used to run the overdue list as of today and
    /// say nothing. Paired: a day that reads is the day asked, a blank day is the
    /// run's own (as the verb reads a blank), and a view with no day takes the run's.
    #[test]
    fn an_unreadable_day_is_refused_and_a_readable_or_blank_one_is_asked() {
        let refused = overdue_as_of(Some("2026-13-45"))
            .err()
            .expect("no day, no list");
        assert!(
            refused.to_string().contains("YYYY-MM-DD"),
            "the reason is the verb's: {refused}"
        );
        let march = overdue_as_of(Some("2026-03-10")).expect("a day that reads");
        assert_eq!(march.asks.overdue, "2026-03-10".parse().ok());
        for run_day in [overdue_as_of(None), overdue_as_of(Some("  "))] {
            let asked = run_day.expect("no day named is the run's day");
            assert_eq!(asked.asks.overdue, "2026-03-01".parse().ok());
        }
    }

    use crate::memory::testing::{Down, add_args, healthy_and_down};
    use rmcp::handler::server::wrapper::Parameters;

    /// **A list whose records cannot be read is told apart from a list nobody
    /// ticked**: the page of a view reads the same answer.
    #[tokio::test]
    async fn a_list_whose_records_cannot_be_read_is_not_a_list_with_no_ticks() {
        let (healthy, blind) =
            healthy_and_down(Down::RecallOfAfter("list:errands", 1, Default::default()));
        healthy
            .add_entity(Parameters(add_args("list", "list:errands", "Errands")))
            .await
            .expect("add ok");
        let question = Question::new(
            graph::Selection {
                kind: Some(EntityKind::LIST),
                ..Default::default()
            },
            graph::Include::default(),
            None,
            None,
        );
        let context = Context {
            carriers: &[],
            instance: &BTreeMap::new(),
        };

        let read = answer(healthy.memory.as_ref(), &question, &context)
            .await
            .unwrap_or_else(|_| panic!("the healthy store answers"));
        assert!(
            matches!(read.ticks.as_slice(), [TickCell::Read(_)]),
            "the list is read when its records can be"
        );

        let unread = answer(blind.memory.as_ref(), &question, &context)
            .await
            .unwrap_or_else(|_| panic!("the selection still answers"));
        assert_eq!(unread.selected.objects.len(), 1);
        assert!(
            matches!(unread.ticks.as_slice(), [TickCell::Unreadable]),
            "a list whose records failed to read is unreadable, not unticked"
        );
    }
}
