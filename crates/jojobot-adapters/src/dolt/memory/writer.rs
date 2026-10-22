//! **The bot that made a write, as the store keeps it and reads it**
//! (work: q-writer-identity-stable, rule 268).
//!
//! A write keeps the writing bot as the id the bot is filed under, the same
//! pointer every other row holds, so a rename of the bot changes what the write
//! reads as naming and nothing stored. Every reader of a claim's writer comes
//! here: whether the claim is another bot's, the writers of a thread, and the bot a
//! history names. A value a build kept as a handle (a row only a store of the
//! build before this one holds) is read through the handles the store still
//! resolves, so such a row reads as it was meant.

use jojobot_domain::memory::{EntityId, MemoryError, authorship};
use sqlx::{MySql, Transaction};

use super::DoltMemory;

impl DoltMemory {
    /// **The writer to record for `bot`**, a handle as the verb names it.
    pub(super) async fn stored_writer(
        &self,
        tx: &mut Transaction<'_, MySql>,
        bot: &EntityId,
    ) -> Result<EntityId, MemoryError> {
        let key = self.resolve(tx, bot).await?.map(|(key, _)| key);
        Ok(authorship::stored_writer(bot, |_| key))
    }

    /// **Whether a recorded writer is the thing filed under `against`.** The id it
    /// is, or a handle the thing wore or wears.
    pub(super) async fn writer_names(
        &self,
        tx: &mut Transaction<'_, MySql>,
        writer: &EntityId,
        against: &EntityId,
    ) -> Result<bool, MemoryError> {
        if writer == against {
            return Ok(true);
        }
        Ok(self
            .resolve(tx, writer)
            .await?
            .is_some_and(|(key, _)| key == *against))
    }

    /// **The handle a recorded writer answers to now**, whether the row holds the id
    /// or a handle.
    pub(super) async fn writer_handle(
        &self,
        tx: &mut Transaction<'_, MySql>,
        recorded: &EntityId,
    ) -> Result<EntityId, MemoryError> {
        let named = self.current_handle(tx, recorded).await?;
        Ok(self
            .resolve(tx, &named)
            .await?
            .map_or(named, |(_, handle)| handle))
    }
}
