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
    let warning = "WARN jojobot::wiring::field_links: FIELD VALUE LEFT AS TEXT value=thing:upgrade-fixture-thing key 'merged_from' item 'thing:recorded-twin':";
    assert!(degradations(&[warning.into()], false, &[]).is_empty());
    for (from, to) in [
        ("thing:upgrade-fixture-thing", "thing:recorded-heir"),
        ("merged_from", "another_key"),
        ("thing:recorded-twin", "thing:recorded-spare"),
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
        "thing:upgrade-fixture-thing",
        "merged_from",
        "thing:recorded-twin",
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

// Audited raw held values in deploy5 (first three) and deploy6 through 9.2.1
// (the last three). The CI fixture decides which remain; other identities fail
// closed.
const RECORDED_R1: [(&str, &str, &str); 6] = [
    (
        "thing:upgrade-fixture-thing",
        "merged_from",
        "thing:recorded-twin",
    ),
    ("thing:jukebox", "merged_from", "thing:recorded-spare"),
    (
        "person:upgrade-fixture-person",
        "looks_at",
        "view:colleagues",
    ),
    (
        "thing:upgrade-fixture-fold-b",
        "merged_from",
        "thing:upgrade-fixture-fold-a",
    ),
    (
        "thing:upgrade-fixture-fold-c",
        "merged_from",
        "thing:upgrade-fixture-fold-b",
    ),
    (
        "document:upgrade-fixture-manual",
        "merged_from",
        "document:upgrade-fixture-appendix",
    ),
];

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

/// Only a not-yet-marked store can warn about its held R1 values. A superseded,
/// inactive or cleared value is historical, even though its raw row survives.
pub(super) async fn recorded_r1_before_boot(
    state_dir: &std::path::Path,
    store_port: u16,
) -> (bool, Vec<String>) {
    let mut store = open_the_store(state_dir, store_port).await;
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
            // The audited merged_from values are engine history, never R1 warnings.
            if key != "merged_from"
                && RECORDED_R1.contains(&(holder.as_str(), key.as_str(), value.as_str()))
            {
                expected.push(r1_identity(&holder, &key, &value));
            }
        }
    }
    store.stop().await;
    (marked, expected)
}
