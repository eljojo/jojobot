//! The runs of the bots, read through the shared boundary.
//!
//! The page of a bot, `recall` of kind session and `list_runs` read the same
//! thing: a bot's runs in the one order of runs, which
//! `jojobot_domain::session::history::newest_first` owns and nothing here sorts.
//! This is where the read says who may ask for which runs (rule 444). What each
//! asker does with the read stays with it: a table, a document, a page of lines.

use std::collections::BTreeMap;

use jojobot_domain::actor::Actor;
use jojobot_domain::memory::EntityId;
use jojobot_domain::session::{Session, SessionError, SessionSummary};
use jojobot_domain::usage::{RunCall, ServedDay};

/// **What a question about runs asks for.** Each part past `bot` is read only
/// when it is asked for, so a caller that renders none of it does not pay for it.
#[derive(Debug, Clone, Default)]
pub struct RunsAsk {
    /// Whose runs are wanted. `None` reads no run: only the counts a caller asks
    /// for come back, which is what a caller with no identity is owed.
    pub bot: Option<EntityId>,
    /// How many runs each bot has, for a browse that is told how many it was
    /// not shown.
    pub counts: bool,
    /// What each run was served, per day.
    pub served: bool,
    /// The verbs each run called, and what was refused.
    pub calls: bool,
    /// How many of the newest runs that wrote a beat have their chronology read
    /// whole.
    pub chronologies: usize,
}

/// **What a read of runs came to.**
#[derive(Debug, Default)]
pub struct Runs {
    /// Every run of the bot, newest first. A run whose record could not be read
    /// is here and says so on itself.
    pub summaries: Vec<SessionSummary>,
    /// How many runs each bot has. Empty unless asked for.
    pub counts: BTreeMap<EntityId, usize>,
    /// What the runs in `summaries` were served, per day, and no other run. Empty
    /// unless asked for.
    pub served: Vec<ServedDay>,
    /// What each run called. Empty unless asked for.
    pub calls: Vec<RunCall>,
    /// The chronologies of the newest runs that wrote at least one beat, in the
    /// same order, at most as many as the call asked for.
    pub chronologies: Vec<Session>,
}

/// **Why a read of a bot's runs gave none.**
#[derive(Debug)]
pub enum RunsFailed {
    /// The asker may not read this bot's runs.
    NotYours,
    /// The runs could not be read.
    Unreadable(SessionError),
}

/// **Whether an asker may read the runs of a bot.** The operator reads every
/// bot's. A bot reads its own and no other's. An app and the system read none: a
/// run is a bot's own record.
pub fn may_read_runs(asker: &Actor, bot: &EntityId) -> bool {
    match asker {
        Actor::Operator => true,
        Actor::Bot(own) => own == bot,
        Actor::App(_) | Actor::System(_) => false,
    }
}
