//! **What stops a piece of work from being started**, asked of the store.
//!
//! The question is the domain's ([`jojobot_domain::memory::readiness`]); what
//! this adds is the lookup it needs. A dependency is another thing, so the
//! things a batch of work names are read once, with the instance's own status
//! words, and every piece of work is asked against that one picture. Nothing
//! here is read for a call that did not ask the `ready` question.

use std::collections::{BTreeMap, BTreeSet};

use jojobot_domain::memory::filter::{Lookup, Subject};
use jojobot_domain::memory::readiness::{self, Blocker, Why};
use jojobot_domain::memory::types::list_items;
use jojobot_domain::memory::{Memory, MemoryError};

use crate::orientation::instance_zone::instance_fields_of;

/// **The most characters one object's blockers may take**, whole entries only.
/// A piece of work can name hundreds of dependencies, and an answer that listed
/// every one would be its largest object; what is left out is counted, and the
/// work's own `depends_on` already lists every dependency. The first blocker is
/// always kept, because an object that names none says it is ready. **This is
/// a bound beside the ceiling, not a share of it**: [`fit_under_ceiling`] cuts
/// further when the rest of the answer leaves less.
pub const BLOCKED_BY_ROOM: usize = 3_000;

/// **A piece of work as the question sees it.**
pub struct Asked<'a> {
    /// The kind token.
    pub kind: &'a str,
    /// The folded fields.
    pub fields: &'a BTreeMap<String, String>,
    /// Whether it is archived.
    pub archived: bool,
}

/// **The things a batch of work names, and the instance's status words.**
struct Referents {
    named: BTreeMap<String, (String, BTreeMap<String, String>, bool)>,
    instance: BTreeMap<String, String>,
}

impl Lookup for Referents {
    fn thing(&self, handle: &str) -> Option<Subject<'_>> {
        self.named
            .get(handle)
            .map(|(kind, fields, archived)| Subject {
                kind,
                fields,
                archived: *archived,
            })
    }

    fn instance(&self) -> &BTreeMap<String, String> {
        &self.instance
    }
}

/// **The verdict on one piece of work.**
pub struct Verdict {
    /// Whether the work can be started: the readiness tree's answer, and
    /// nothing else decides it.
    pub ready: bool,
    /// What stops it, to explain the verdict. Empty when it is ready.
    pub blockers: Vec<Blocker>,
}

/// **The verdict on each piece of work, in the order they were given.**
pub async fn verdicts_of(
    memory: &dyn Memory,
    asked: &[Asked<'_>],
) -> Result<Vec<Verdict>, MemoryError> {
    let wanted: BTreeSet<&str> = asked
        .iter()
        .filter_map(|work| work.fields.get(readiness::DEPENDS_ON))
        .flat_map(|held| list_items(held))
        .collect();
    let mut named = BTreeMap::new();
    if !wanted.is_empty() {
        for entity in memory.list_entities(None).await? {
            if wanted.contains(entity.id.as_str()) {
                let fields = memory.fields(&entity.id).await?;
                named.insert(
                    entity.id.to_string(),
                    (
                        entity.kind.as_token().to_string(),
                        fields,
                        entity.archived.is_some(),
                    ),
                );
            }
        }
    }
    let referents = Referents {
        named,
        instance: instance_fields_of(memory).await,
    };
    Ok(asked
        .iter()
        .map(|work| {
            let subject = Subject {
                kind: work.kind,
                fields: work.fields,
                archived: work.archived,
            };
            let ready = readiness::is_ready(&subject, &referents);
            Verdict {
                ready,
                blockers: if ready {
                    Vec::new()
                } else {
                    readiness::blocked_by(&subject, &referents)
                },
            }
        })
        .collect())
}

/// **One blocker as the answer carries it.**
fn blocker_json(blocker: &Blocker) -> serde_json::Value {
    match blocker {
        Blocker::Finished => serde_json::json!({ "blocker": "finished" }),
        Blocker::WaitingOn(handle) => {
            serde_json::json!({ "blocker": "waiting_on", "handle": handle })
        }
        Blocker::Dependency { handle, why } => serde_json::json!({
            "blocker": "dependency",
            "handle": handle,
            "why": match why {
                Why::Unfinished => "unfinished",
                Why::Archived => "archived",
                Why::Dangling => "dangling",
            },
        }),
    }
}

/// **The count of blockers left out, and the way to read them.**
fn left_out_marker(count: usize) -> serde_json::Value {
    serde_json::json!({
        "count": count,
        "how_to_read": "this object's depends_on lists every dependency; recall any of \
                        them to read whether it is finished",
    })
}

/// **The characters one entry of a list takes in the answer**, its comma included.
fn cost_of(entry: &serde_json::Value) -> usize {
    entry.to_string().len() + 1
}

/// **Write the readiness of one object onto its rendered form**: `ready`, and
/// when it is not, the blockers up to [`BLOCKED_BY_ROOM`] characters, and a
/// count of the rest with the way to read them.
pub fn render(rendered: &mut serde_json::Value, verdict: &Verdict) {
    rendered["ready"] = verdict.ready.into();
    let blockers = &verdict.blockers;
    if verdict.ready {
        return;
    }
    let mut spent = 0usize;
    let shown: Vec<serde_json::Value> = blockers
        .iter()
        .map(blocker_json)
        .take_while(|entry| {
            let first = spent == 0;
            spent += cost_of(entry);
            first || spent <= BLOCKED_BY_ROOM
        })
        .collect();
    let left_out = blockers.len() - shown.len();
    rendered["blocked_by"] = shown.into();
    if left_out > 0 {
        rendered["blocked_by_left_out"] = left_out_marker(left_out);
    }
}

/// **Take the blockers' room out of the answer ceiling.** Run on the finished
/// answer, once the rest of it can be counted: each object's blockers keep the
/// smaller of [`BLOCKED_BY_ROOM`] and what is left under the ceiling after the
/// rest of the answer and the object's own row, with room kept for the marker
/// that counts what was cut. The first blocker is always kept. What this cuts
/// is added to what [`render`] already left out.
pub fn fit_under_ceiling(body: &mut serde_json::Value) {
    use jojobot_domain::text::ANSWER_CEILING;
    let Some(mut objects) = body
        .get_mut("objects")
        .and_then(|objects| objects.as_array_mut())
        .map(std::mem::take)
    else {
        return;
    };
    let rest = crate::answer::served_len(body)
        + crate::answer::STATUS_BAR_ROOM
        + crate::answer::widest_not_shown("objects");
    for object in objects.iter_mut() {
        let Some(mut blockers) = object
            .get_mut("blocked_by")
            .and_then(|blockers| blockers.as_array_mut())
            .map(std::mem::take)
        else {
            continue;
        };
        let already_left_out =
            object["blocked_by_left_out"]["count"].as_u64().unwrap_or(0) as usize;
        if let Some(fields) = object.as_object_mut() {
            fields.remove("blocked_by_left_out");
        }
        // The row without its blockers, and the marker at its widest.
        object["blocked_by"] = serde_json::json!([]);
        let own = crate::answer::served_len(object) + 1;
        let marker = cost_of(&left_out_marker(usize::MAX)) + "blocked_by_left_out".len();
        let room = BLOCKED_BY_ROOM.min(ANSWER_CEILING.saturating_sub(rest + own + marker));
        let mut spent = 0usize;
        let kept = blockers
            .iter()
            .take_while(|entry| {
                let first = spent == 0;
                spent += cost_of(entry);
                first || spent <= room
            })
            .count();
        let left_out = already_left_out + blockers.len() - kept;
        blockers.truncate(kept);
        object["blocked_by"] = blockers.into();
        if left_out > 0 {
            object["blocked_by_left_out"] = left_out_marker(left_out);
        }
    }
    body["objects"] = objects.into();
}
