//! Legacy row companions retained until their partial-batch proof is rehomed.

/// **One change of the legacy rows**, as the batch that causes it carries it:
/// a mark recorded, or every mark of a run forgotten. The change is typed so no
/// statement text crosses to the log; the rows are written by this module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowChange {
    Record {
        sid: String,
        domain: String,
        taught_at: jiff::Timestamp,
    },
    Forget {
        sid: String,
    },
}

impl RowChange {
    /// Run the change on a connection that is inside the batch's transaction.
    pub(crate) async fn apply(
        &self,
        connection: &mut sqlx::MySqlConnection,
    ) -> Result<(), sqlx::Error> {
        match self {
            Self::Record {
                sid,
                domain,
                taught_at,
            } => {
                sqlx::query(
                    "INSERT INTO session_teaching (sid, domain, taught_at) VALUES (?, ?, ?)",
                )
                .bind(sid)
                .bind(domain)
                .bind(taught_at.to_string())
                .execute(connection)
                .await?;
            }
            Self::Forget { sid } => {
                sqlx::query("DELETE FROM session_teaching WHERE sid = ?")
                    .bind(sid)
                    .execute(connection)
                    .await?;
            }
        }
        Ok(())
    }
}
