//! **The passes a view's answer goes through after its selection**, in one place.
//!
//! A view is answered twice: by the `recall` verb and by the page of the view
//! record. The two once ran their own copies of these passes and disagreed, so a
//! page listed work that could not start. Both call these functions now, and
//! neither keeps a second copy (rule 444).
//!
//! What stays with each caller is what it does with the answer: the verb fits
//! the objects to its ceiling and renders them as JSON, and the page renders a
//! table. Nothing here reads a caller's arguments or writes a word of an answer.

use std::collections::BTreeMap;

use jojobot_domain::attention;
use jojobot_domain::memory::graph;
use jojobot_domain::memory::{EntityId, Memory, MemoryError};

use super::ready::{self, Verdict};

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
    /// How many days past the day asked about each kept object fell due, read
    /// off the same `Due` the order came from, so the number cannot disagree
    /// with the position it is in.
    pub by_days: Vec<Option<i64>>,
    /// Why each kept object that cannot say when it falls due cannot.
    pub unreadable_because: Vec<Option<attention::Unreadable>>,
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
) -> OwedPass {
    let before = found.len();
    found.retain(|object| owed_by(carriers, object, instance).owed_on(as_of));
    let excluded = before - found.len();
    found.sort_by_key(|object| owed_by(carriers, object, instance).staleness());
    let by_days = found
        .iter()
        .map(|object| owed_by(carriers, object, instance).days_overdue(as_of))
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

/// **Keep the work that can be started.** Asked of the work the selection
/// found, against the things its `depends_on` names. Work that is not ready is
/// left out and counted, **except the one thing a call named**, which always
/// comes back so a caller can read why.
pub async fn keep_ready(
    memory: &dyn Memory,
    found: &mut Vec<graph::Object>,
    named: Option<&EntityId>,
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
        if verdict.ready || named == Some(&object.entity.id) {
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
