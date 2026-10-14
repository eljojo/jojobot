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
//! **Seven views ship, and that is deliberate rather than a first instalment of
//! a catalogue.** Two are the ones the operator named, three ask about work
//! (what can be started, what a plan holds and what a project's board shows),
//! one asks which things answer a type and one asks what falls due. A rich set
//! is generated once the shape is settled. If this file starts growing a query
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
            folded_names: Vec::new(),
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
        //
        // **It says who each bot reports to, from the bot's own fields, and
        // walks nothing.** A walk inward along `reports_to` would list who
        // reports to a bot, and it repeats every report in full under its
        // manager: measured, it made this answer larger, which is the one thing
        // the view is for avoiding. Who reports to a bot is a second call,
        // `recall` of it with `reports_to` followed inward. Display only:
        // nothing acts on the key.
        view(
            "view:colleagues",
            "The Colleagues",
            &[
                ("selects", "bot"),
                // **Only what the directory is for.** A bot holds keys that
                // run it, and an answer carrying every one of them for every
                // colleague buries the two a reader came for. The answer names
                // what it left out, and an operator extends this by editing
                // the record: a caller's own `keys` wins over it.
                ("shows_keys", "one_liner, reports_to"),
            ],
        ),
        // **What can be started.** Work that is not finished, whose
        // dependencies are all finished, and that waits on nothing. The ask is
        // the question; the answer counts what it left out and names the read
        // that says why.
        view(
            "view:ready",
            "Ready",
            &[
                ("selects", "work"),
                ("asks", jojobot_domain::memory::readiness::ASK),
                // **Only the work under one project, when a call names it.** Left
                // out, the question is every piece of ready work.
                ("inputs", "project: kind project?"),
                ("parent", "$project"),
            ],
        ),
        // **What is coming up by a day, and what is already late**, whatever
        // kinds owe something. The overdue question already answers only the kinds
        // that owe, so the view names no kind. `by` is the day asked about and
        // leaves a week ahead when a call sends none; each object says how late
        // it is from the run's own day (`overdue_by_days`) or how soon it falls due
        // (`due_in_days`), the oldest due first.
        view(
            "view:due",
            "Due",
            &[
                ("asks", "overdue"),
                ("inputs", "by: date? = today+7d"),
                ("as_of", "$by"),
            ],
        ),
        // **A whole plan in one call.** The work that is not finished, each
        // piece with whether it can be started and, when it cannot, what stops
        // it. The same readiness question as `ready`, kept differently: a plan
        // lists the blocked work too, so a session reads a project's blockers
        // without asking after each piece. Name a project as `parent` to read
        // its plan.
        view(
            "view:plan",
            "Plan",
            &[
                ("selects", "work"),
                ("asks", jojobot_domain::memory::readiness::ASK_PLAN),
                // **Only one project's plan, when a call names it**, as `ready`
                // does. Left out, the question is every project's plan.
                ("inputs", "project: kind project?"),
                ("parent", "$project"),
            ],
        ),
        // **The work of one project as a board.** Each piece with the three keys
        // a board is read by, grouped by status. The order is the status words
        // as text, so the groups run done, next, now, someday, waiting: the
        // engine orders a key it has no declared sequence for as text.
        // **The project is required**: a board with no project is every piece of
        // work, which is not a board.
        view(
            "view:board",
            "Board",
            &[
                ("selects", "work"),
                (
                    "one_liner",
                    "The work under one project, grouped by status.",
                ),
                ("inputs", "project: kind project"),
                ("parent", "$project"),
                ("order", "status"),
                ("shows_keys", "status, depends_on, owner"),
            ],
        ),
        // **The things that answer a declared type.** The tolerant question, so a
        // thing holding some of the type's keys comes back with the ones it lacks
        // named. **The type is an input**, bound into `answers_type`.
        // **It finds things only**: `selects` is one kind, so a type another kind
        // answers is asked of `recall` with `answers_type` directly, and the
        // description says so.
        view(
            "view:of-type",
            "Of Type",
            &[
                ("selects", "thing"),
                (
                    "one_liner",
                    "The things that answer a declared type, each with the keys it lacks. It finds \
                     things only: ask recall with answers_type for a type another kind answers.",
                ),
                ("inputs", "type: text"),
                ("answers_type", "$type"),
            ],
        ),
    ]
}
