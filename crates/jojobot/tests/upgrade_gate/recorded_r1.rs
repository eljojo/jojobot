//! The recorded raw values a boot over the last deployed release may warn about, and the
//! rule for what a value held before the upgrade must be after it.
use super::*;

/// **A value held before the upgrade is held after it.** One key is allowed to
/// grow: the boot tells each bot the shipped notes its ledger (`release_notes_told`)
/// does not hold and adds their ids to it. What the ledger held stays in it.
/// Any other key must read back unchanged.
pub(super) fn assert_still_held(git_ref: &str, handle: &str, key: &str, was: &str, now: &str) {
    if key == "release_notes_told" {
        let kept: Vec<&str> = now.split(',').map(str::trim).collect();
        let lost: Vec<&str> = was
            .split(',')
            .map(str::trim)
            .filter(|id| !kept.contains(id))
            .collect();
        assert!(
            lost.is_empty(),
            "recorded at {git_ref}: {handle} held '{key}' = '{was}' before the upgrade and no \
             longer holds {lost:?}; it holds '{now}' after"
        );
        return;
    }
    assert_eq!(
        now, was,
        "recorded at {git_ref}: {handle} held '{key}' = '{was}' before the upgrade (ids read as \
         the handles they answered to) and holds '{now}' after"
    );
}

/// The marker of the pass that marks every link in a stored field value.
pub(super) const MARKING_PASS: &str = "field value links marked";

/// The marker of the pass the rule before it ran at each boot. A store that ever
/// ran under the rule before rule 646 holds it; one built from empty by a build that
/// runs the rule on never does.
pub(super) const LOWERING_PASS: &str = "field value lowering";

#[test]
fn only_the_recorded_r1_values_are_expected_warnings() {
    let warning = "WARN jojobot::wiring::field_links: FIELD VALUE LEFT AS TEXT value=person:upgrade-fixture-person key 'looks_at' item 'view:colleagues':";
    assert!(degradations(&[warning.into()], false, &[]).is_empty());
    for (from, to) in [
        ("person:upgrade-fixture-person", "thing:recorded-heir"),
        ("looks_at", "another_key"),
        ("view:colleagues", "thing:recorded-spare"),
        ("FIELD VALUE LEFT AS TEXT", "ANOTHER WARNING"),
        ("WARN", "ERROR"),
    ] {
        let changed = warning.replace(from, to);
        assert_eq!(
            degradations(std::slice::from_ref(&changed), false, &[]),
            [&changed]
        );
    }
    let expected = [r1_identity(
        "person:upgrade-fixture-person",
        "looks_at",
        "view:colleagues",
    )];
    assert!(recorded_r1_counts_match(&[warning.into()], &expected));
    assert!(!recorded_r1_counts_match(&[], &expected));
    assert!(!recorded_r1_counts_match(
        &[warning.into(), warning.into()],
        &expected
    ));
    assert!(!recorded_r1_counts_match(&[warning.into()], &[]));
    assert!(recorded_r1_counts_match(&[], &[]));
}

// The one audited raw held value that can warn on the first boot over the last
// deployed store. Every audited merged_from value is engine history and never
// warns, so a warning about one is a degradation like any other. The CI fixture
// decides whether this one remains; other identities fail closed.
const RECORDED_R1: [(&str, &str, &str); 1] = [(
    "person:upgrade-fixture-person",
    "looks_at",
    "view:colleagues",
)];

fn r1_identity(holder: &str, key: &str, item: &str) -> String {
    format!("value={holder} key '{key}' item '{item}':")
}

pub(super) fn is_recorded_r1_warning(line: &str) -> bool {
    line.contains("WARN")
        && !line.contains("ERROR")
        && line.contains("FIELD VALUE LEFT AS TEXT")
        && RECORDED_R1
            .iter()
            .any(|&(holder, key, item)| line.contains(&r1_identity(holder, key, item)))
}

pub(super) fn recorded_r1_counts_match(seen: &[String], expected: &[String]) -> bool {
    RECORDED_R1.iter().all(|&(holder, key, item)| {
        let identity = r1_identity(holder, key, item);
        let actual = seen
            .iter()
            .filter(|line| is_recorded_r1_warning(line) && line.contains(&identity))
            .count();
        actual == usize::from(expected.contains(&identity))
    })
}

/// Whether the store holds a snapshot of the event log, a row of the table the snapshots are
/// kept in. A store with no such table holds none, which is every store a build before the
/// table recorded: the snapshot it held was a row of the event log, which this build does not
/// read. Any other failure of the count is an error, because reading it as "none held" would
/// let a boot that lost its snapshot pass the gate.
pub(super) async fn snapshots_held(pool: &sqlx::MySqlPool) -> Result<bool, sqlx::Error> {
    match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM log_snapshot")
        .fetch_one(pool)
        .await
    {
        Ok(held) => Ok(held > 0),
        Err(sqlx::Error::Database(refusal)) if refusal.message().contains("table not found") => {
            Ok(false)
        }
        Err(other) => Err(other),
    }
}

/// Only a not-yet-marked store can warn about its held R1 values. A superseded,
/// inactive or cleared value is historical, even though its raw row survives.
/// **The third answer is whether the store holds a snapshot of the event log**:
/// the same open store is read for it, and a boot over one that holds none logs the
/// warning that no snapshot was used. A store with no event log table holds none.
pub(super) async fn recorded_r1_before_boot(
    state_dir: &std::path::Path,
    store_port: u16,
) -> (bool, Vec<String>, bool) {
    let mut store = open_the_store(state_dir, store_port).await;
    let snapshot_held = snapshots_held(store.pool())
        .await
        .expect("the snapshot count reads");
    let passes: Vec<String> = sqlx::query_scalar("SELECT name FROM store_pass")
        .fetch_all(store.pool())
        .await
        .expect("the pre-boot markers read");
    let marked = passes.iter().any(|name| name == MARKING_PASS);
    let mut expected = Vec::new();
    if !marked && passes.iter().any(|name| name == LOWERING_PASS) {
        // The MAX includes NULL clears. Joining the owning fact excludes orphan
        // writes, and only the newest active write of each key can still be held.
        let held: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT e.id, w.`key`, w.value FROM field_write w
             JOIN entity e ON e.badge = w.entity
             JOIN fact f ON f.entity = w.entity AND f.id = w.fact_id
             WHERE f.status = 'active' AND w.value IS NOT NULL
               AND w.ordinal = (
                 SELECT MAX(newer.ordinal) FROM field_write newer
                 JOIN fact owner ON owner.entity = newer.entity AND owner.id = newer.fact_id
                 WHERE newer.entity = w.entity AND newer.`key` = w.`key`
                   AND owner.status = 'active')",
        )
        .fetch_all(store.pool())
        .await
        .expect("the raw held values read before marking");
        for (holder, key, value) in held {
            if RECORDED_R1.contains(&(holder.as_str(), key.as_str(), value.as_str())) {
                expected.push(r1_identity(&holder, &key, &value));
            }
        }
    }
    store.stop().await;
    (marked, expected, snapshot_held)
}

#[tokio::test]
async fn a_store_with_no_event_log_table_holds_no_snapshot() {
    let scratch = Scratch::new("gate-snapshot-held-no-table");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("a store comes up");
    let held = snapshots_held(server.pool()).await;
    server.stop().await;
    assert!(
        matches!(held, Ok(false)),
        "no table reads as none held: {held:?}"
    );
}

#[tokio::test]
async fn a_snapshot_row_reads_as_held_and_an_empty_log_as_none() {
    let scratch = Scratch::new("gate-snapshot-held-rows");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("a store comes up");
    let pool = jojobot_adapters::testing::migrated_database(&server, "alpha")
        .await
        .expect("a migrated database");
    let empty = snapshots_held(&pool).await;
    let thirty_two_bytes = "00".repeat(32);
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO log_snapshot (slot, covered_position, snapshot_format_version, \
         projection_version, log_identity, representation_epoch, erasure_epoch, batch_checksum, \
         created_at, checksum, serialized_length, body) \
         VALUES ('snapshot:1', 1, 1, 1, 'alpha', 0, 0, X'{thirty_two_bytes}', \
         '1970-01-01T00:00:00Z', X'{thirty_two_bytes}', 1, X'00')"
    )))
    .execute(&pool)
    .await
    .expect("a snapshot row lands");
    let held = snapshots_held(&pool).await;
    drop(pool);
    server.stop().await;
    assert!(
        matches!(empty, Ok(false)),
        "an empty log holds none: {empty:?}"
    );
    assert!(matches!(held, Ok(true)), "a snapshot row is held: {held:?}");
}

#[tokio::test]
async fn a_count_that_fails_for_another_reason_is_an_error_not_none_held() {
    let scratch = Scratch::new("gate-snapshot-held-closed");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("a store comes up");
    let pool = jojobot_adapters::testing::migrated_database(&server, "alpha")
        .await
        .expect("a migrated database");
    pool.close().await;
    let held = snapshots_held(&pool).await;
    server.stop().await;
    assert!(
        held.is_err(),
        "a closed pool is not a store with no snapshot: {held:?}"
    );
}
