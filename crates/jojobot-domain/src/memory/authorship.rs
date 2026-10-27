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

use super::{Edge, EdgeShape, EntityId, EntityKind};

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

/// **The identity a write keeps for the bot that made it**: the id the bot is filed
/// under, which a rename never changes, and never the handle it wore at the
/// moment of the write (rule 268). `key_of` is a store's own way of telling what a
/// bot is filed under; a bot nothing answers to is kept as it was named. Both
/// stores take the writer they record from here, and every read of a writer goes
/// back through it ([`first_writer`] and [`another_bots_claim`]).
pub fn stored_writer(
    bot: &EntityId,
    key_of: impl FnOnce(&EntityId) -> Option<EntityId>,
) -> EntityId {
    key_of(bot).unwrap_or_else(|| bot.clone())
}

/// **Whether a claim draws a `connection` edge**: the one test every site asks
/// before it asks whose the claim is (rule 498). A claim that draws one is never a
/// rule of its home's ([`super::rules_in_force`]); it is that home's thought only
/// when no other bot wrote it ([`is_thought_of`]).
pub fn is_connection(edge: Option<&Edge>) -> bool {
    edge.is_some_and(|edge| edge.shape == EdgeShape::Connection)
}

/// **Whether a claim is a thought of its home's**: a connection claim no other
/// bot wrote. Another bot's connection claim on a bot is an ordinary linked claim,
/// neither that bot's thought nor its rule.
pub fn is_thought_of(edge: Option<&Edge>, written_by_another: bool) -> bool {
    !written_by_another && is_connection(edge)
}

#[cfg(test)]
mod tests;
