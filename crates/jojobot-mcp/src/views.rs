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
fn view(slug: &str, name: &str, keys: &[(&str, &str)]) -> Provision {
    let id = EntityId::new(EntityKind::VIEW, slug);
    Provision::record(
        Entity {
            id: id.clone(),
            kind: EntityKind::VIEW,
            name: name.to_string(),
            aliases: Vec::new(),
            source: "jojobot".to_string(),
            crm: None,
            parent: None,
            boot: Default::default(),
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
            "loops",
            "The Loops",
            &[("selects", "rhythm"), ("shows", "facts")],
        ),
        // **Who your colleagues are.** The identities on this server with what
        // each is for, which is the charter — so a bot asking who else is here
        // reads it as a question rather than as a verb of its own (rule 139).
        view(
            "colleagues",
            "The Colleagues",
            &[("selects", "bot"), ("shows", "charter")],
        ),
    ]
}
