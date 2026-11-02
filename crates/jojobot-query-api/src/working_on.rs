//! The declarations of what a bot is working on, read out of the history of its
//! one `working_on` record. A page and a verb ask the same question, so the
//! answer's shape and the rule that builds it live at the boundary.

use std::collections::BTreeSet;

use jojobot_domain::memory::{ClaimWrite, Entity, EntityId, FieldWrite};
use jojobot_domain::session::SessionSummary;

/// **One time a session declared what its bot is working on**, read from the
/// history of the bot's one working-on record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// The bot that declared it.
    pub bot: EntityId,
    /// The thing it declared.
    pub on: EntityId,
    /// The public name of the run that declared it, when that can be read.
    pub run: Option<String>,
    /// When the store took the declaration in, when it recorded one.
    pub at: Option<jiff::Timestamp>,
    /// Whether this is the declaration the bot is working on NOW: the one the
    /// gate shows. Every other is a past one.
    pub live: bool,
}

/// **One bot's side of the declarations question**: every write of its
/// `working_on`, oldest first, what it is working on NOW (the value the gate
/// shows, and `None` when the gate shows none), and the runs the bot has, which
/// name the run of each write.
pub struct BotWrites<'a> {
    pub bot: &'a EntityId,
    pub writes: &'a [FieldWrite],
    pub live_on: Option<&'a str>,
    pub runs: &'a [SessionSummary],
    /// **The whole write chain of the one record that carries `writes`**, read by
    /// the caller, or empty when it was not read. Only a write the store kept
    /// with no origin reads its run from it (see [`declarations_from`]).
    pub record: &'a [ClaimWrite],
}

/// **Every declaration by these bots that points at a thing in `of`**, live or
/// past, oldest first for each bot.
///
/// Whether a declaration is live is the gate's answer and nothing else (rule
/// 498): the caller passes the value the gate shows, and the bot's newest write
/// is live when it points at that value. Nothing here looks at a session's state.
/// The run of a write is the bot's run it names, found by the one rule of
/// [`jojobot_domain::session::run_named`]; a run no card answers to leaves it empty.
///
/// **A write with no stored origin** (one a build kept before the store pointed
/// each write at its claim write) takes its run from the declaration's own claim
/// chain, the way 9.3.1.3 read it: only when every write is of one record and
/// that record's chain has as many writes, so the nth write is the nth claim
/// write. When the two do not line up, nothing is claimed. This is for the
/// declaration record alone; field history in general never invents an origin.
pub fn declarations_from(
    known: &[Entity],
    bots: &[BotWrites<'_>],
    of: &BTreeSet<EntityId>,
) -> Vec<Declaration> {
    let mut found = Vec::new();
    for BotWrites {
        bot,
        writes,
        live_on,
        runs,
        record,
    } in bots
    {
        let newest = writes.len().saturating_sub(1);
        for (at, write) in writes.iter().enumerate() {
            // **A write of a key that names a thing is kept as that thing's
            // permanent id**, behind the mark that says it is a link, so a stored
            // value is turned back into the handle the thing wears now.
            let Some(on) = write.value.as_deref().map(|stored| {
                let badge = stored
                    .strip_prefix(jojobot_domain::memory::mention::MARK)
                    .unwrap_or(stored);
                jojobot_domain::memory::entity_wearing(badge, known)
                    .map_or_else(|| EntityId(stored.to_string()), |thing| thing.id.clone())
            }) else {
                continue;
            };
            if !of.contains(&on) {
                continue;
            }
            let run = write
                .run
                .as_deref()
                .or_else(|| legacy_run(writes, record, at))
                .and_then(|stored| jojobot_domain::session::run_named::find_among(runs, stored))
                .and_then(|summary| summary.sid.as_ref())
                .map(|sid| sid.as_str().to_string());
            found.push(Declaration {
                bot: (*bot).clone(),
                live: at == newest && *live_on == Some(on.as_str()),
                on,
                run,
                at: write.written_at,
            });
        }
    }
    found
}

/// **The session that made write `at`, read off the record's claim chain**, when
/// the chain is known to line up with the writes: one record carries them all and
/// its chain is as long as they are.
fn legacy_run<'a>(writes: &[FieldWrite], record: &'a [ClaimWrite], at: usize) -> Option<&'a str> {
    let first = writes.first()?;
    if record.len() != writes.len() || writes.iter().any(|write| write.fact != first.fact) {
        return None;
    }
    record.get(at)?.session.as_deref()
}
