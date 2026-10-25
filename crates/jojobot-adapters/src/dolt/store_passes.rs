use super::*;

/// Read the ledger without treating an unavailable read as an unfinished pass.
pub(crate) async fn finished_passes_checked(
    pool: &MySqlPool,
) -> Result<std::collections::BTreeSet<String>, sqlx::Error> {
    Ok(
        sqlx::query_scalar::<_, String>("SELECT name FROM store_pass")
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect(),
    )
}

/// **Which passes have finished on this store**, read before any table a pass
/// would read.
///
/// A marker that cannot be read is not a finished pass: it reads as none, so
/// the passes run. They are idempotent, which is what makes that the safe
/// side, and a store that cannot answer this one small read fails the passes
/// themselves in the same way and says so there.
///
/// Any other one-time pass over stored rows asks here by its own name and
/// records its own end with [`finish_pass`]; the names in [`PASSES`] are only
/// the four that [`migrate_permanent_ids`] runs.
pub(crate) async fn finished_passes(pool: &MySqlPool) -> std::collections::BTreeSet<String> {
    match finished_passes_checked(pool).await {
        Ok(names) => names,
        Err(e) => {
            tracing::warn!(error = %e, "store: could not read which passes have finished");
            Default::default()
        }
    }
}

/// **Record that a pass examined every row of its table and failed on none.**
/// Written once and never rewritten: a second writer finding the row there has
/// nothing to add. A marker that cannot be written costs one more run of an
/// idempotent pass at the next boot, so it is logged and not raised.
pub(crate) async fn finish_pass(pool: &MySqlPool, name: &str) {
    if let Err(e) = sqlx::query("INSERT IGNORE INTO store_pass (name, finished_at) VALUES (?, ?)")
        .bind(name)
        .bind(jiff::Timestamp::now().to_string())
        .execute(pool)
        .await
    {
        tracing::warn!(pass = name, error = %e, "store: could not record that a pass finished");
    }
}
