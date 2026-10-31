//! **The claim write a key write points at, as a row reads it.**
//!
//! `field_write.fact_write_ordinal` names the write of the claim that carried the
//! key (migration 0118). Every read of key writes selects the same columns of
//! that claim write and reads them through [`origin_of`], so the writer, the run
//! and the app mark come off one row (rule 598).

use jojobot_domain::memory::write_origin::WriteOrigin;
use jojobot_domain::memory::{EntityId, MemoryError};
use sqlx::Row;

use super::{store, unreadable};

/// **The columns of the claim write**, to select beside the `field_write` row
/// aliased `w`. `os` is the run's session row, which names the bot behind a run
/// when the write recorded none.
pub(super) const ORIGIN_COLUMNS: &str = "w.fact_write_ordinal AS origin_ordinal, \
     o.bot AS origin_bot, o.session AS origin_run, o.app_stamp AS origin_app_stamp, \
     os.bot AS origin_run_bot";

/// **The joins that reach those columns.** Left joins: a write with no pointer
/// still comes back, with no origin.
pub(super) const ORIGIN_JOINS: &str = "LEFT JOIN fact_write o ON o.entity = w.entity \
     AND o.fact_id = w.fact_id AND o.ordinal = w.fact_write_ordinal \
     LEFT JOIN session os ON os.sid = o.session";

/// **What the claim write kept**, or `None` for a key write with no pointer: an
/// older build appended it, and nothing says which claim write it was.
pub(super) fn origin_of(row: &sqlx::mysql::MySqlRow) -> Result<Option<WriteOrigin>, MemoryError> {
    let pointed: Option<i64> = row.try_get("origin_ordinal").map_err(store)?;
    if pointed.is_none() {
        return Ok(None);
    }
    let text = |column: &str| -> Result<Option<String>, MemoryError> {
        row.try_get::<Option<String>, _>(column).map_err(store)
    };
    Ok(Some(WriteOrigin {
        bot: text("origin_bot")?.map(EntityId),
        run: text("origin_run")?,
        run_bot: text("origin_run_bot")?.map(EntityId),
        app_stamp: match text("origin_app_stamp")? {
            None => None,
            Some(cell) => Some(
                serde_json::from_str(&cell)
                    .map_err(|_| unreadable("its app mark cannot be read"))?,
            ),
        },
    }))
}
