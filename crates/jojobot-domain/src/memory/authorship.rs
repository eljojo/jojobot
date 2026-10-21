//! **Whose thought a connection claim is.**
//!
//! A bot's thought is a connection claim the bot wrote on itself, so the claim
//! faces the bot's body cap and takes a seat in its room. A claim another bot
//! wrote on it is an ordinary claim. A thing that is not a bot has no author to
//! be told apart from: every connection claim on it faces what it set, whoever
//! wrote it, so a cap an explicit setting put there binds wherever it is set.
//!
//! **One function answers, and every site that needs the answer asks it**
//! (rule 498): each store's read of a claim, each store's check of a write, and
//! with them the writers of a thread and the receipt's reserve line. A store
//! keeps only its own session lookup and its own way of telling a bot's key from
//! its handle.

use super::{EntityId, EntityKind};

/// **Whether a connection claim on `home` was written by a bot other than
/// `home`.** `writer` is the bot behind the run that first wrote the claim, or
/// `None` when no run is recorded or none answers to it, which reads as the
/// home's own. `names_home` says whether a bot, as a store keeps it, is the thing
/// `home` answers to: the key it is filed under as well as its handle.
pub fn another_bots_claim(
    home: &EntityId,
    writer: Option<&EntityId>,
    names_home: impl Fn(&EntityId) -> bool,
) -> bool {
    home.kind() == Some(EntityKind::BOT)
        && writer.is_some_and(|writer| writer != home && !names_home(writer))
}

/// **The bot that first wrote a claim**: the bot its first write recorded, else the
/// bot behind the run that made it. A write kept before the bot was recorded has
/// none, so it is read through its run as it always was; a run no row answers to
/// names nobody. The one place both stores take the writer from (rule 498).
pub fn first_writer(
    recorded: Option<EntityId>,
    behind_the_run: impl FnOnce() -> Option<EntityId>,
) -> Option<EntityId> {
    recorded.or_else(behind_the_run)
}
