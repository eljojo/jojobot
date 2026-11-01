//! **Teaching, as rows.** One row per `(sid, domain)`; existence is the whole
//! answer, so there is nothing here to read back except whether the row was
//! just created.

use jojobot_domain::session::Sid;
use jojobot_domain::teaching::{TeachingError, Teachings};
use sqlx::MySqlPool;

/// TEMPORARY(9.3.3) the teaching ledger as rows. The log of teaching events
/// (`jojobot_domain::teaching::events`) replaces it, and it goes at 9.3.3.
///
/// The per-session teaching ledger, kept in the SQL store jojobot runs.
///
/// Cloning shares the one pool rather than opening a second: a pool is the
/// connection budget, and two of them against one server is two budgets
/// nobody set.
#[derive(Clone)]
pub struct DoltTeachings {
    pool: crate::writer::sql::Pool,
}

impl DoltTeachings {
    /// Open against an already-migrated pool.
    pub fn open(pool: MySqlPool) -> Self {
        Self { pool: pool.into() }
    }
}

impl DoltTeachings {
    /// **Every stored mark with the time it was taught** — the import's source.
    /// [`Teachings::taught`] returns only the pairs. A time that does not parse
    /// is an error: the import never invents one.
    pub async fn marks(&self) -> Result<Vec<crate::teaching_log::Mark>, TeachingError> {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT sid, domain, taught_at FROM session_teaching ORDER BY sid, domain",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        rows.into_iter()
            .map(|(sid, domain, taught_at)| {
                let taught_at = taught_at.parse().map_err(|e| {
                    TeachingError::Store(format!("a stored teaching time is unreadable: {e}"))
                })?;
                Ok(crate::teaching_log::Mark {
                    sid: Sid(sid),
                    domain,
                    taught_at,
                })
            })
            .collect()
    }
}

/// **Whether the refusal is the pair's own key**, which means the pair is
/// already taught. Another unique index refusing the insert is a refused rule,
/// not a repeat contact.
fn duplicate_pair(err: &sqlx::Error) -> bool {
    err.as_database_error().is_some_and(|db| {
        db.is_unique_violation() && db.message().to_ascii_lowercase().contains("primary")
    })
}

/// **A conflict is told apart from every other failure here, at the one
/// seam every write already passes through** — the same check the memory
/// rail's own `store` runs, see its doc for why.
fn store(err: sqlx::Error) -> TeachingError {
    if err.as_database_error().and_then(|db| db.code()).as_deref() == Some("40001") {
        tracing::warn!(error = %err, "a write conflicted with another landing the same instant");
        return TeachingError::Conflict;
    }
    if let Some(rule) = super::refused_rule(&err) {
        tracing::error!(error = %err, rule, "the teaching store refused a write on a rule it enforces");
        return TeachingError::Refused(rule.to_string());
    }
    crate::writer::sql::store_error(err)
}

#[async_trait::async_trait]
impl Teachings for DoltTeachings {
    async fn first_contact(
        &self,
        sid: &Sid,
        domain: &str,
        at: jiff::Timestamp,
    ) -> Result<bool, TeachingError> {
        // The key `(sid, domain)` decides: a second insert of the pair is the
        // duplicate-key refusal, which is the answer `false`. No read is made.
        let mut tx = self.pool.begin().await.map_err(store)?;
        let inserted =
            sqlx::query("INSERT INTO session_teaching (sid, domain, taught_at) VALUES (?, ?, ?)")
                .bind(sid.as_str())
                .bind(domain)
                .bind(at.to_string())
                .execute(&mut *tx)
                .await;
        match inserted {
            Ok(_) => {
                crate::writer::sql::commit(tx).await.map_err(store)?;
                Ok(true)
            }
            Err(err) if duplicate_pair(&err) => Ok(false),
            Err(err) => Err(store(err)),
        }
    }

    async fn forget(&self, sid: &Sid) -> Result<(), TeachingError> {
        sqlx::query("DELETE FROM session_teaching WHERE sid = ?")
            .bind(sid.as_str())
            .execute(&self.pool)
            .await
            .map_err(store)?;
        Ok(())
    }

    async fn taught(&self) -> Result<Option<Vec<(Sid, String)>>, TeachingError> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT sid, domain FROM session_teaching ORDER BY sid, domain")
                .fetch_all(&self.pool)
                .await
                .map_err(store)?;
        Ok(Some(
            rows.into_iter()
                .map(|(sid, domain)| (Sid(sid), domain))
                .collect(),
        ))
    }
}
