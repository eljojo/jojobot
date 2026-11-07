//! The import reader keeps every row of the last deployed store.
use super::*;
use jojobot_domain::log::claims::import::{self, Rows};
use std::collections::BTreeSet;

/// The rows of the recording, read by the one import reader and told what the
/// build supplies, as the boot import is. The boot import stops on any finding,
/// so the recording must read with none.
async fn read_import_rows(pool: &sqlx::MySqlPool) -> Rows {
    let read = jojobot_adapters::dolt::memory::DoltMemory::open(pool.clone())
        .knowing(jojobot_mcp::provisions())
        .import_rows()
        .await
        .expect("the recorded store reads");
    let findings: Vec<String> = read
        .findings
        .iter()
        .map(|finding| format!("{} {}: {}", finding.table, finding.row, finding.what))
        .collect();
    assert!(
        findings.is_empty(),
        "the import reader could not place {} rows: {}",
        findings.len(),
        findings.join("; ")
    );
    read.rows
}

#[tokio::test]
async fn the_last_deployed_claim_rows_import_without_losing_a_row() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        ..
    } = restore_the_fixture("claim-import-rows").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let pool = store.pool();
    jojobot_adapters::dolt::migrate::run(pool)
        .await
        .expect("the current schema reads the recording");
    let rows = read_import_rows(pool).await;
    let attributed: BTreeSet<_> = rows
        .entity_writes
        .iter()
        .map(|row| (&row.thing, &row.filed_under, row.ordinal))
        .collect();
    assert_eq!(
        attributed.len(),
        rows.entity_writes.len(),
        "each owner gets a row once"
    );
    let counts = [
        ("entity", "SELECT COUNT(*) FROM entity", rows.entities.len()),
        (
            "entity_alias",
            "SELECT COUNT(*) FROM entity_alias",
            rows.entities.iter().map(|row| row.aliases.len()).sum(),
        ),
        (
            "entity_write",
            "SELECT COUNT(*) FROM entity_write",
            rows.entity_writes
                .iter()
                .map(|row| (&row.filed_under, row.ordinal))
                .collect::<BTreeSet<_>>()
                .len(),
        ),
        (
            "field_link",
            "SELECT COUNT(*) FROM field_link",
            rows.field_writes.iter().map(|row| row.targets.len()).sum(),
        ),
        (
            "entity_former_handle",
            "SELECT COUNT(*) FROM entity_former_handle",
            rows.former_handles.len(),
        ),
        ("fact", "SELECT COUNT(*) FROM fact", rows.facts.len()),
        (
            "fact_write",
            "SELECT COUNT(*) FROM fact_write",
            rows.facts.iter().map(|row| row.writes.len()).sum(),
        ),
        (
            "field_write",
            "SELECT COUNT(*) FROM field_write",
            rows.field_writes.len(),
        ),
        (
            "merge_event",
            "SELECT COUNT(*) FROM merge_event",
            rows.merges.iter().map(|row| 1 + row.claims.len()).sum(),
        ),
        (
            "type_write",
            "SELECT COUNT(*) FROM type_write",
            rows.type_writes.len(),
        ),
        (
            "fact_event_metadata",
            "SELECT COUNT(*) FROM fact_event_metadata",
            rows.metadata.len(),
        ),
    ];
    for (table, query, decoded) in counts {
        let stored: i64 = sqlx::query_scalar(query)
            .fetch_one(pool)
            .await
            .expect("the recorded rows count");
        if table == "entity" || table == "fact" || table == "fact_write" {
            assert!(stored > 0, "the recording must hold {table} rows");
        }
        assert_eq!(decoded as i64, stored, "{table}: no recorded row is lost");
    }
    import::import(&rows).expect("the recorded rows map through e0 without an ImportError");
    store.stop().await;
    drop(scratch);
}
