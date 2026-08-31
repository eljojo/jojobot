//! **Teaching, as rows.** One row per `(sid, domain)`; existence is the whole
//! answer, so there is nothing here to read back except whether the row was
//! just created.

use jojobot_domain::session::Sid;
use jojobot_domain::teaching::{TeachingError, Teachings};
use sqlx::MySqlPool;

/// The per-session teaching ledger, kept in the SQL store jojobot runs.
///
/// Cloning shares the one pool rather than opening a second: a pool is the
/// connection budget, and two of them against one server is two budgets
/// nobody set.
#[derive(Clone)]
pub struct DoltTeachings {
    pool: MySqlPool,
}

impl DoltTeachings {
    /// Open against an already-migrated pool.
    pub fn open(pool: MySqlPool) -> Self {
        Self { pool }
    }
}

fn store(err: sqlx::Error) -> TeachingError {
    TeachingError::Store(err.to_string())
}

#[async_trait::async_trait]
impl Teachings for DoltTeachings {
    async fn first_contact(
        &self,
        sid: &Sid,
        domain: &str,
        at: jiff::Timestamp,
    ) -> Result<bool, TeachingError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let existing: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM session_teaching WHERE sid = ? AND domain = ?")
                .bind(sid.as_str())
                .bind(domain)
                .fetch_optional(&mut *tx)
                .await
                .map_err(store)?;
        if existing.is_some() {
            tx.commit().await.map_err(store)?;
            return Ok(false);
        }
        sqlx::query("INSERT INTO session_teaching (sid, domain, taught_at) VALUES (?, ?, ?)")
            .bind(sid.as_str())
            .bind(domain)
            .bind(at.to_string())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        tx.commit().await.map_err(store)?;
        Ok(true)
    }
}
