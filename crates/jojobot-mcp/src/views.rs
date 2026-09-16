//! **The views the software ships** — questions a session can ask by name
//! without anybody having declared them.
//!
//! A view is a record, so shipping one is supplying a record: the same shape
//! the operator's own views have, through the same door. **There is no
//! catalogue of queries in the code** — no function returning the questions
//! somebody thought of, and no second path for the operator's own (rule 106).
//! What is here is data, and the read that runs a view cannot tell which half
//! supplied it.
//!
//! # Mechanism now, list later
//!
//! **Two views ship, and that is deliberate rather than a first instalment of
//! a catalogue.** They are the two the operator named, and a rich set is
//! generated once the shape is settled. If this file starts growing a query
//! per question somebody asks, it has taken the wrong turn.

use std::collections::BTreeMap;

use jojobot_domain::memory::owned::Provision;
use jojobot_domain::memory::{Entity, EntityId, EntityKind};

/// One shipped view: its handle, what a reader calls it, and its query.
///
/// **The handle is written whole rather than assembled from a kind and a bare
/// slug.** The bright-line gate reads handle-shaped text, so a slug passed on
/// its own is a name no gate compares against the fictional roster (rule 164).
/// Writing the handle puts the shipped record inside a check that already runs
/// (rule 45).
fn view(handle: &str, name: &str, keys: &[(&str, &str)]) -> Provision {
    Provision::record(
        Entity {
            id: EntityId(handle.to_string()),
            kind: EntityKind::VIEW,
            name: name.to_string(),
            aliases: Vec::new(),
            source: "jojobot".to_string(),
            crm: None,
            parent: None,
            boot: Default::default(),
            merged_into: None,
            // **A record the build supplies is in no table**, so it wears no
            // badge: a badge is the name a ROW keeps, and there is no row here
            // to rename. Text cannot store a mention of one.
            badge: None,
            archived: None,
        },
        keys.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<BTreeMap<_, _>>(),
    )
}

/// **What this build ships as views.**
pub fn provisions() -> Vec<Provision> {
    vec![
        // **What has gone quiet.** The loops, each with the day it was last
        // done — which is a key a rhythm holds, so selecting the kind answers
        // it without asking for anything else.
        view(
            "view:loops",
            "The Loops",
            &[("selects", "rhythm"), ("shows", "facts")],
        ),
        // **Who your colleagues are.** The identities on this server, each
        // with its one-liner — the short written line saying what it is for
        // (rule 139). **Charters are not shown by default**: a charter can run
        // to thousands of characters, so asking for six of them at once is the
        // most expensive answer this server has. A caller who wants one asks
        // `recall` for it directly, with `charter: true`.
        view("view:colleagues", "The Colleagues", &[("selects", "bot")]),
    ]
}
