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

/// **How many characters one object's blockers may take**, whole entries only.
/// A piece of work can name hundreds of dependencies, and an answer that listed
/// every one would be its largest object; what is left out is counted, and the
/// work's own `depends_on` already lists every dependency. The first blocker is
/// always kept, because an object that names none says it is ready.
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
            spent += entry.to_string().len() + 1;
            first || spent <= BLOCKED_BY_ROOM
        })
        .collect();
    let left_out = blockers.len() - shown.len();
    rendered["blocked_by"] = shown.into();
    if left_out > 0 {
        rendered["blocked_by_left_out"] = serde_json::json!({
            "count": left_out,
            "how_to_read": "this object's depends_on lists every dependency; recall any of \
                            them to read whether it is finished",
        });
    }
}
