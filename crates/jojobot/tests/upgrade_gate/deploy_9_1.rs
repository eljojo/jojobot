//! Stored-record readback on the last deployed release's CI-built store.
use super::*;

/// **A bot booted on the upgraded store is not told again a note its ledger
/// holds.** The recording holds `release_notes_told` on `bot:assistant`. The boot
/// sends each bot the shipped notes its ledger does not hold, so a boot that lost
/// the ledger would list a held note in a new message from the system sender.
/// The case reads the ids a message lists off its last line and never the text of
/// a note.
#[tokio::test]
async fn a_bot_is_not_told_again_a_note_it_held_before_the_upgrade() {
    let restored = restore_a_fixture(&fixture_dir(), "told-notes-not-resent").await;
    let held: Vec<String> = restored
        .held_before
        .iter()
        .find(|(handle, key, _)| handle == "bot:assistant" && key == "release_notes_told")
        .map(|(_, _, ids)| ids.split(',').map(|id| id.trim().to_string()).collect())
        .expect("the recording holds a ledger of told notes on bot:assistant");
    assert!(!held.is_empty(), "the ledger holds at least one note");

    let before = system_messages_in_box(&restored, "assistant").await;
    boot_current(&restored.state_dir, restored.store_port, &restored.git_ref)
        .await
        .stop()
        .await;
    let after = system_messages_in_box(&restored, "assistant").await;

    let listed_now: Vec<String> = after
        .iter()
        .filter(|(id, _)| !before.iter().any(|(was, _)| was == id))
        .flat_map(|(_, body)| {
            body.trim_end()
                .lines()
                .last()
                .and_then(|line| line.strip_prefix("Notes: "))
                .map(|ids| {
                    ids.split(',')
                        .map(|id| id.trim().to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .collect();
    let again: Vec<&String> = listed_now.iter().filter(|id| held.contains(id)).collect();
    assert!(
        again.is_empty(),
        "recorded at {}: bot:assistant held {held:?} and the boot told it {again:?} again",
        restored.git_ref
    );
}

/// The system sender's messages in one box, as `(id, body)`, read from the store
/// on its own with no binary over it.
async fn system_messages_in_box(restored: &Restored, mailbox: &str) -> Vec<(String, String)> {
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, body FROM message WHERE mailbox = ? AND sender = 'jojobot'")
            .bind(mailbox)
            .fetch_all(store.pool())
            .await
            .expect("the box's system messages read");
    store.stop().await;
    rows
}

/// **A value held before the upgrade is held after it.** One key is allowed to
/// grow: the boot tells each bot the shipped notes its ledger (`release_notes_told`)
/// does not hold and adds their ids to it. What the ledger held stays in it, and
/// `a_bot_is_not_told_again_a_note_it_held_before_the_upgrade` holds the notes the
/// boot told. Any other key must read back unchanged.
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

/// **A boot with the link rule on, over the last deployed release's store.**
/// The repairs of stored rows run first, the marking pass runs once and is marked
/// finished, and a second boot changes no row. Every gate boot explicitly selects
/// On; this case also checks its marker and repeat stability.
#[tokio::test]
async fn the_current_binary_boots_with_the_link_rule_on_over_the_last_deployed_store() {
    the_link_rule_preserves_a_recorded_store(&fixture_dir(), "boots-deploy9-1-link-rule-on").await;
}

async fn the_link_rule_preserves_a_recorded_store(fixture: &str, label: &str) {
    let restored = restore_a_fixture(fixture, label).await;
    as_a_recording_before_the_marking(&restored.state_dir, restored.store_port).await;
    let first = boot_current_with(
        &restored.state_dir,
        restored.store_port,
        &restored.git_ref,
        false,
        None,
        &[],
    )
    .await;
    first.stop().await;
    let after_first = dump_store(&restored.state_dir).await;
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let markers: Vec<String> = sqlx::query_scalar("SELECT name FROM store_pass ORDER BY name")
        .fetch_all(store.pool())
        .await
        .expect("the markers read");
    store.stop().await;
    assert!(
        markers
            .iter()
            .any(|name| name == "field value links marked"),
        "recorded at {}: the marking pass finished: {markers:?}",
        restored.git_ref
    );
    let second = boot_current_with(
        &restored.state_dir,
        restored.store_port,
        &restored.git_ref,
        false,
        None,
        &[],
    )
    .await;
    second.stop().await;
    assert!(
        dump_store(&restored.state_dir).await == after_first,
        "recorded at {}: a second boot with the rule on changed the store",
        restored.git_ref
    );
    let third = boot_current_with(
        &restored.state_dir,
        restored.store_port,
        &restored.git_ref,
        false,
        None,
        &[],
    )
    .await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", third.http_port))
        .await
        .unwrap();
    assert_every_thing_holds_what_it_held(&surface, &restored.git_ref, &restored.held_before).await;
    surface.finish().await;
    third.stop().await;
}

pub(super) const LOG_ONLY: &str = "log-only";

thread_local! {
    /// The word of `JOJOBOT_LINK_RULE` the gate's boots of this case's thread give the
    /// binary. Every boot is `on` unless a case says otherwise; a case runs on its own
    /// thread, so no other case sees the word.
    static LINK_RULE_WORD: std::cell::Cell<&'static str> = const { std::cell::Cell::new("on") };
}

/// The word the next boot of this thread gives the binary as `JOJOBOT_LINK_RULE`.
pub(super) fn link_rule_word() -> &'static str {
    LINK_RULE_WORD.with(std::cell::Cell::get)
}

/// **A boot of the current binary with the link rule at `word`**, over a restored
/// store, on a thread that is given `on` back when the boot is up.
pub(super) async fn boot_with_link_rule(word: &'static str, restored: &Restored) -> Booted {
    boot_with_link_rule_tolerating(word, restored, &[]).await
}

/// [`boot_with_link_rule`] for a boot that logs a warning the case expects.
pub(super) async fn boot_with_link_rule_tolerating(
    word: &'static str,
    restored: &Restored,
    tolerated_warnings: &[&str],
) -> Booted {
    LINK_RULE_WORD.with(|held| held.set(word));
    let booted = boot_current_with(
        &restored.state_dir,
        restored.store_port,
        &restored.git_ref,
        false,
        None,
        tolerated_warnings,
    )
    .await;
    LINK_RULE_WORD.with(|held| held.set("on"));
    booted
}

/// **The recorded store as a build before the marking left it.** The recording
/// is made by the newest deployed build. Once that build runs the link rule on by
/// default, it marks the store at its own boot and leaves the marker of the pass
/// in what it records, and a marker makes a boot skip the marking. A case that
/// needs the marking to run takes the marker off first. The values the recording
/// holds are already marked, and the pass keeps a mark it finds.
pub(super) async fn as_a_recording_before_the_marking(
    state_dir: &std::path::Path,
    store_port: u16,
) {
    let mut store = open_the_store(state_dir, store_port).await;
    sqlx::query("DELETE FROM store_pass WHERE name = 'field value links marked'")
        .execute(store.pool())
        .await
        .expect("the marker of the marking pass is taken off");
    store.stop().await;
}

/// Whether the marking pass has finished in the store, read with no binary over it.
async fn links_are_marked(restored: &Restored) -> bool {
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let markers: Vec<String> = sqlx::query_scalar("SELECT name FROM store_pass ORDER BY name")
        .fetch_all(store.pool())
        .await
        .expect("the markers read");
    store.stop().await;
    markers
        .iter()
        .any(|name| name == "field value links marked")
}

/// Every link row, read with no binary over the store.
async fn link_rows(restored: &Restored) -> Vec<(String, String, i64, String)> {
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let rows = sqlx::query_as(
        "SELECT entity, `key`, ordinal, target FROM field_link ORDER BY entity, `key`, ordinal, target",
    )
    .fetch_all(store.pool())
    .await
    .expect("the link rows read");
    store.stop().await;
    rows
}

fn says_it_is_log_only(booted: &Booted) -> bool {
    booted
        .log
        .iter()
        .any(|line| line.contains("link rule (log-only)"))
}

/// **The rollback the operator would fall back to, over the last deployed release's
/// store.** The rule is set to `log-only` explicitly. A boot over the recorded store
/// comes up clean, logs the summary of what the rule would do, and leaves the
/// marking pass undone; a boot with the rule on over the same store marks it and logs
/// no such summary; and a boot with `log-only` again, over the store the rule
/// marked, comes up clean, changes no link row or marker, and every thing reads
/// back as it was. Both modes run on the one store, so the case cannot pass on a
/// build that ignores the word.
#[tokio::test]
async fn a_rollback_to_log_only_boots_clean_over_the_recorded_store_and_over_one_the_rule_marked() {
    let restored = restore_a_fixture(&fixture_dir(), "boots-link-rule-log-only-rollback").await;
    as_a_recording_before_the_marking(&restored.state_dir, restored.store_port).await;

    let first = boot_with_link_rule(LOG_ONLY, &restored).await;
    assert!(
        says_it_is_log_only(&first),
        "recorded at {}: a boot with the rule at log-only logs its summary: {:?}",
        restored.git_ref,
        first.log
    );
    first.stop().await;
    assert!(
        !links_are_marked(&restored).await,
        "recorded at {}: log-only ran the marking pass",
        restored.git_ref
    );

    let on = boot_with_link_rule("on", &restored).await;
    assert!(
        !says_it_is_log_only(&on),
        "recorded at {}: a boot with the rule on logs the log-only summary: {:?}",
        restored.git_ref,
        on.log
    );
    on.stop().await;
    assert!(
        links_are_marked(&restored).await,
        "recorded at {}: the boot with the rule on did not mark the store",
        restored.git_ref
    );
    let marked_links = link_rows(&restored).await;

    let rolled_back = boot_with_link_rule(LOG_ONLY, &restored).await;
    assert!(
        says_it_is_log_only(&rolled_back),
        "recorded at {}: the rolled-back boot logs the log-only summary: {:?}",
        restored.git_ref,
        rolled_back.log
    );
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", rolled_back.http_port))
        .await
        .expect("connecting to the rolled-back binary");
    assert_every_thing_holds_what_it_held(&surface, &restored.git_ref, &restored.held_before).await;
    surface.finish().await;
    rolled_back.stop().await;
    assert!(
        links_are_marked(&restored).await,
        "recorded at {}: the rolled-back boot undid the marker",
        restored.git_ref
    );
    assert!(
        link_rows(&restored).await == marked_links,
        "recorded at {}: the rolled-back boot changed a link row",
        restored.git_ref
    );
}

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

// Audited raw held values in deploy5 (first three), and deploy6 through 9.2.1
// (the first six), plus the two merges the repair case plants over the recorded
// store (the last two). The CI fixture decides which remain; other identities
// fail closed.
const RECORDED_R1: [(&str, &str, &str); 8] = [
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
    (
        "thing:upgrade-fixture-holder",
        "merged_from",
        "thing:upgrade-fixture-heir",
    ),
    (
        "thing:upgrade-fixture-heir",
        "merged_from",
        "thing:upgrade-fixture-husk",
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
    // Log-only runs no marking pass, so it reports none of the marking's values.
    if link_rule_word() == LOG_ONLY {
        return (false, Vec::new());
    }
    let mut store = open_the_store(state_dir, store_port).await;
    let passes: Vec<String> = sqlx::query_scalar("SELECT name FROM store_pass")
        .fetch_all(store.pool())
        .await
        .expect("the pre-boot markers read");
    let marked = passes.iter().any(|name| name == "field value links marked");
    let mut expected = Vec::new();
    if !marked && passes.iter().any(|name| name == "field value lowering") {
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
    (marked, expected)
}
