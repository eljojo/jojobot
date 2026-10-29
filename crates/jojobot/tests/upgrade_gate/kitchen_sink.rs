//! The shapes still tolerated by the head coexist in one deployed store.
//! Each import that retires a reader removes its paired row here (rule 677).
//! The two narrow idempotence cases use this same prepared store twice: a
//! second boot changing nothing is a behaviour across boots, which the one
//! boot reading the old shapes cannot witness (PM ruling A).
use super::*;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_domain::memory::{EntityId, Memory, NewFact};
use serde_json::Value;

const TAU: &str = "thing:tau";
const FERN: &str = "thing:fern";
const HOLDER: &str = "thing:upgrade-fixture-holder";
const HUSK: &str = "thing:upgrade-fixture-husk";
const HEIR: &str = "thing:upgrade-fixture-heir";
const WAITING: &str = "role:upgrade-fixture-waiting";
const CARRIED: &str = "role:upgrade-fixture-part-carried";
const LEASE: &str = "role:upgrade-fixture-holder";
const MAIL: &str = "0ldquot";
const WORDS: &str = "you said @thing:upgrade-fixture-thing#f1 once";
const NOTE_BOT: &str = "bot:upgrade-fixture-ledger";
const HELD_NOTE: &str = "release-notes";
const NEW_NOTE: &str = "search-matching-passages";

type LeaseRows = Vec<(i64, String, Option<String>)>;
type MergeRows = Vec<(
    i64,
    i64,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
)>;

struct Planted {
    restored: Restored,
    holder: String,
    old: String,
    carried: String,
    tail_before: LeaseRows,
    retained_merges: MergeRows,
}

async fn entity(pool: &sqlx::MySqlPool, handle: &str, badge: &str, parent: Option<&str>) {
    let (kind, name) = handle.split_once(':').expect("a handle");
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, parent, boot, prose, badge)
         VALUES (?, ?, ?, 'test', ?, 'on-demand', '', ?)",
    )
    .bind(handle)
    .bind(kind)
    .bind(name)
    .bind(parent)
    .bind(badge)
    .execute(pool)
    .await
    .expect("a kitchen-sink entity");
}

async fn record(pool: &sqlx::MySqlPool, badge: &str, content: &str) {
    record_id(pool, badge, "f1", content).await;
}

async fn record_id(pool: &sqlx::MySqlPool, badge: &str, id: &str, content: &str) {
    sqlx::query(
        "INSERT INTO fact (entity, id, content, provenance, standing, status, recorded_at,
                           inserted_at)
         VALUES (?, ?, ?, 'testimony', 'settled', 'active', '2026-10-08',
                 '2026-10-08T14:02:10Z')",
    )
    .bind(badge)
    .bind(id)
    .bind(content)
    .execute(pool)
    .await
    .expect("the planted record");
    sqlx::query(
        "INSERT INTO fact_write (entity, fact_id, ordinal, content, provenance, standing, status,
                                 recorded_at, inserted_at, written_at)
         SELECT entity, id, 1, content, provenance, standing, status, recorded_at, inserted_at,
                inserted_at FROM fact WHERE entity = ? AND id = ?",
    )
    .bind(badge)
    .bind(id)
    .execute(pool)
    .await
    .expect("the record's first write");
}

async fn field(pool: &sqlx::MySqlPool, badge: &str, key: &str, value: &str, class: &str) {
    sqlx::query(
        "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id, write_class, written_at)
         VALUES (?, ?, 1, ?, 'f1', ?, '2026-10-08T14:02:10Z')",
    )
    .bind(badge)
    .bind(key)
    .bind(value)
    .bind(class)
    .execute(pool)
    .await
    .expect("the planted field");
}

async fn badge(pool: &sqlx::MySqlPool, handle: &str) -> String {
    sqlx::query_scalar("SELECT COALESCE(badge, id) FROM entity WHERE id = ?")
        .bind(handle)
        .fetch_one(pool)
        .await
        .expect("the existing entity's storage key")
}

// LEGACY_ROW(old-mail-address): work:q-gate-binary-coverage
async fn old_mail(pool: &sqlx::MySqlPool, holder: &str) {
    let sender = badge(pool, "bot:assistant").await;
    sqlx::query(
        "INSERT INTO message (id, mailbox, ordinal, body, sender, sent_at, state,
                              sender_mail_waiting_at_send, posted_by_session)
         SELECT ?, 'assistant', COALESCE(MAX(ordinal), 0) + 1, ?, ?,
                '2026-10-08T13:55:00Z', 'new', 0, ? FROM message WHERE mailbox = 'assistant'",
    )
    .bind(MAIL)
    .bind(WORDS)
    .bind(sender)
    .bind(holder)
    .execute(pool)
    .await
    .expect("mail written before address quotations");
    sqlx::query("DELETE FROM store_pass WHERE name = 'mail mentions'")
        .execute(pool)
        .await
        .expect("the old body has not been lowered yet");
}

// LEGACY_ROW(legacy-write-classes): work:q-gate-binary-coverage
async fn old_classes(pool: &sqlx::MySqlPool) {
    entity(pool, TAU, "kstau1", None).await;
    record(pool, "kstau1", "the old bags and the newer bag beside them").await;
    for class in ["legacy", "own", "sets", "stays"] {
        field(pool, "kstau1", &format!("bag_{class}"), class, class).await;
    }
    field(pool, "kstau1", "donuts", "2", "sets").await;
}

// LEGACY_ROW(null-written-at): work:q-gate-binary-coverage
async fn unstamped(pool: &sqlx::MySqlPool) {
    field(pool, "kstau1", "unstamped", "kept", "sets").await;
    sqlx::query(
        "UPDATE field_write SET written_at = NULL WHERE entity = 'kstau1'
                 AND `key` = 'unstamped'",
    )
    .execute(pool)
    .await
    .expect("a write from before the store stamped writes");
    sqlx::query("UPDATE fact_write SET written_at = NULL WHERE entity = 'kstau1'")
        .execute(pool)
        .await
        .expect("the record's write was unstamped too");
}

// LEGACY_ROW(forwarded-field-link): work:q-gate-binary-coverage
async fn forwarded(pool: &sqlx::MySqlPool) {
    for (handle, key) in [(HOLDER, "kshold"), (HUSK, "kshusk"), (HEIR, "ksheir")] {
        entity(pool, handle, key, None).await;
    }
    sqlx::query("UPDATE entity SET merged_into = ? WHERE id = ?")
        .bind(HEIR)
        .bind(HUSK)
        .execute(pool)
        .await
        .expect("the old fold's forwarding row");
    // The old link is stale; the fold itself is a complete, recorded merge.
    let event: Option<Option<i64>> = sqlx::query_scalar("SELECT MAX(merge_no) FROM merge_event")
        .fetch_optional(pool)
        .await
        .expect("the last recorded merge number");
    sqlx::query(
        "INSERT INTO merge_event (merge_no, ordinal, folded, survivor, merged_at)
                 VALUES (?, 0, 'kshusk', 'ksheir', '2026-10-08T14:02:10Z')",
    )
    .bind(event.flatten().unwrap_or(0) + 1)
    .execute(pool)
    .await
    .expect("the old fold's event, with no claims to map");
    record(
        pool,
        "kshold",
        "points at a thing an older fold left behind",
    )
    .await;
    field(pool, "kshold", "watches", "@#kshusk", "sets").await;
    // The index agrees with the old write; only the forward-repair pass can fix it.
    sqlx::query(
        "INSERT INTO field_link (entity, `key`, ordinal, target)
                 VALUES ('kshold', 'watches', 1, 'kshusk')",
    )
    .execute(pool)
    .await
    .expect("the old link row");
    sqlx::query("DELETE FROM store_pass WHERE name = 'forwarded field links repaired'")
        .execute(pool)
        .await
        .expect("the older pass did not finish this repair");
}

// LEGACY_ROW(stored-describing-key): work:q-gate-binary-coverage
async fn describing(pool: &sqlx::MySqlPool) {
    for (name, key, holds, folds) in [
        ("legacy-starry", "starred", "number", "sum"),
        ("legacy-pantry", "subject", "text", "newest"),
        ("legacy-clean", "donuts", "number", "newest"),
    ] {
        sqlx::query(
            "INSERT INTO type_field (type_name, key_name, ordinal, holds, origin, folds,
                                            owner, required)
                     VALUES (?, ?, 1, ?, 'declared', ?, 'type', 0)",
        )
        .bind(name)
        .bind(key)
        .bind(holds)
        .bind(folds)
        .execute(pool)
        .await
        .expect("the declaration an older build accepted");
        sqlx::query(
            "INSERT INTO type_write (name, ordinal, written_at, fields)
                     VALUES (?, 1, '2026-08-01T00:00:00Z', ?)",
        )
        .bind(name)
        .bind(
            json!([{"key": key, "holds": holds, "folds": folds, "required": false,
                         "one_of": null}])
            .to_string(),
        )
        .execute(pool)
        .await
        .expect("the declaration's history");
    }
    field(pool, "kstau1", "starred", "3", "own").await;
}

// LEGACY_ROW(unreadable-loop): work:q-gate-binary-coverage
async fn unreadable(pool: &sqlx::MySqlPool) {
    entity(pool, FERN, "ksfern", None).await;
    record(
        pool,
        "ksfern",
        "watered on a schedule nobody wrote down properly",
    )
    .await;
    for (key, value) in [
        ("cadence_days", "often"),
        ("advances_from", "due_date"),
        ("counts_from", "2026-06-01"),
    ] {
        field(pool, "ksfern", key, value, "legacy").await;
    }
    sqlx::query("DELETE FROM store_pass WHERE name = 'unreadable due day'")
        .execute(pool)
        .await
        .expect("the unreadable loop has not been stamped");
}

// LEGACY_ROW(half-moved-role-claims): work:q-gate-binary-coverage
async fn half_moved(pool: &sqlx::MySqlPool, holder: &str, old: &str, carried: &str) {
    let bot = badge(pool, "bot:assistant").await;
    for (handle, key) in [(WAITING, "kswait"), (CARRIED, "ksdone")] {
        entity(pool, handle, key, Some(&bot)).await;
    }
    record(
        pool,
        "ksdone",
        "claimed the upgrade-fixture-part-carried role",
    )
    .await;
    field(pool, "ksdone", "holder", holder, "sets").await;
    field(pool, "ksdone", "claimed_at", carried, "sets").await;
    // The pass removes these keys and preserves the record's other words and keys.
    for role in ["upgrade-fixture-waiting", "upgrade-fixture-part-carried"] {
        let old_holder = if role == "upgrade-fixture-part-carried" {
            "old-partial-holder"
        } else {
            holder
        };
        for (key, value) in [
            (format!("role/{role}/holder"), old_holder),
            (format!("role/{role}/claimed_at"), old),
        ] {
            let wrote = sqlx::query(
                "INSERT INTO field_write
                (entity, `key`, ordinal, value, fact_id, write_class)
                SELECT entity, ?, 1, ?, id, 'legacy' FROM fact
                WHERE entity = ? AND status = 'active' ORDER BY id LIMIT 1",
            )
            .bind(key)
            .bind(value)
            .bind(&bot)
            .execute(pool)
            .await
            .expect("the old role keys on their bot")
            .rows_affected();
            assert_eq!(wrote, 1, "an active record carries the old claim");
        }
    }
    sqlx::query("DELETE FROM store_pass WHERE name = 'old role claims'")
        .execute(pool)
        .await
        .expect("the move remains unfinished");
}

// LEGACY_ROW(appended-lease-tail): work:q-gate-binary-coverage
async fn lease_tail(pool: &sqlx::MySqlPool, old: &str) -> Vec<(i64, String, Option<String>)> {
    let role = badge(pool, LEASE).await;
    let (newest, fact, class): (i64, String, String) = sqlx::query_as(
        "SELECT ordinal, fact_id, write_class FROM field_write WHERE entity = ?
                                         AND `key` = 'claimed_at' ORDER BY ordinal DESC LIMIT 1",
    )
    .bind(&role)
    .fetch_one(pool)
    .await
    .expect("the recorded lease's claim");
    let baseline: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM field_write WHERE entity = ? AND `key` = 'claimed_at'",
    )
    .bind(&role)
    .fetch_one(pool)
    .await
    .expect("the recording's lease history count");
    // Retain the recording's history and make room immediately below its newest row.
    sqlx::query(
        "UPDATE field_write SET ordinal = ordinal + 20, value = ?, written_at = ?
                 WHERE entity = ? AND `key` = 'claimed_at' AND ordinal = ?",
    )
    .bind(old)
    .bind(old)
    .bind(&role)
    .bind(newest)
    .execute(pool)
    .await
    .expect("the newest moment moves up, aged to require a renewal");
    let latest: jiff::Timestamp = old.parse().expect("the planted lease moment");
    for ordinal in 0..20_i64 {
        let moment = (latest - jiff::SignedDuration::from_mins(3 * (20 - ordinal))).to_string();
        sqlx::query(
            "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id, written_at,
                                            write_class)
                     VALUES (?, 'claimed_at', ?, ?, ?, ?, ?)",
        )
        .bind(&role)
        .bind(newest + ordinal)
        .bind(&moment)
        .bind(&fact)
        .bind(&moment)
        .bind(&class)
        .execute(pool)
        .await
        .expect("a renewal the appending build left behind");
    }
    let rows = lease_rows(pool, &role).await;
    assert_eq!(
        rows.len() as i64,
        baseline + 20,
        "the planted tail adds twenty writes"
    );
    rows
}

async fn lease_rows(pool: &sqlx::MySqlPool, role: &str) -> Vec<(i64, String, Option<String>)> {
    sqlx::query_as(
        "SELECT ordinal, value, written_at FROM field_write
                    WHERE entity = ? AND `key` = 'claimed_at' ORDER BY ordinal",
    )
    .bind(role)
    .fetch_all(pool)
    .await
    .expect("each lease row's identity, value and stamp")
}

// LEGACY_ROW(rewritten-reference-list): work:q-gate-binary-coverage
async fn rewritten_list(pool: &sqlx::MySqlPool) {
    let project = badge(pool, "project:upgrade-fixture-project").await;
    for (handle, key) in [
        ("work:theta", "ksthet"),
        ("work:iota", "ksiota"),
        ("work:kappa", "kskapp"),
        ("work:lambda", "kslamb"),
    ] {
        entity(pool, handle, key, Some(&project)).await;
    }
    let lists: [&[&str]; 3] = [
        &["ksiota", "kskapp", "kslamb"],
        &["kskapp", "kslamb"],
        &["kslamb"],
    ];
    for (at, members) in lists.into_iter().enumerate() {
        let ordinal = (at + 1) as i64;
        let id = format!("f{ordinal}");
        record_id(pool, "ksthet", &id, "the dependency list was shortened").await;
        let value = members
            .iter()
            .map(|id| jojobot_domain::memory::marked_stored_handle(id))
            .collect::<Vec<_>>()
            .join(", ");
        sqlx::query(
            "INSERT INTO field_write
            (entity, `key`, ordinal, value, fact_id, written_at, write_class)
            VALUES ('ksthet', 'depends_on', ?, ?, ?, '2026-10-08T14:02:10Z', 'sets')",
        )
        .bind(ordinal)
        .bind(value)
        .bind(id)
        .execute(pool)
        .await
        .expect("a whole list write");
        for target in members {
            sqlx::query(
                "INSERT INTO field_link (entity, `key`, ordinal, target)
                         VALUES ('ksthet', 'depends_on', ?, ?)",
            )
            .bind(ordinal)
            .bind(*target)
            .execute(pool)
            .await
            .expect("the list write's index");
        }
    }
}

// LEGACY_ROW(release-note-ledger): work:q-gate-binary-coverage
async fn note_ledger(pool: &sqlx::MySqlPool) {
    use jojobot_domain::memory::capacity::{ENGINE_RECORD, RELEASE_NOTES_RECORD};
    entity(pool, NOTE_BOT, "ksnote", None).await;
    sqlx::query("INSERT INTO mailbox (name, owner) VALUES ('upgrade-fixture-ledger', ?)")
        .bind(NOTE_BOT)
        .execute(pool)
        .await
        .expect("the old bot's empty box");
    record(
        pool,
        "ksnote",
        "jojobot keeps here the release notes this bot has been told.",
    )
    .await;
    field(pool, "ksnote", ENGINE_RECORD, RELEASE_NOTES_RECORD, "stays").await;
    field(pool, "ksnote", "release_notes_told", HELD_NOTE, "sets").await;
    // No message can mask a broken ledger reader by recovering this id from mail.
    let messages: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM message
                                          WHERE mailbox = 'upgrade-fixture-ledger'",
    )
    .fetch_one(pool)
    .await
    .expect("the new box's count");
    assert_eq!(messages, 0);
}

// LEGACY_ROW(unmarked-field-link): work:q-gate-binary-coverage
async fn unmarked_link(pool: &sqlx::MySqlPool) {
    let target = badge(pool, "bot:assistant").await;
    // The preceding rule stored a bare badge under a declared reference key.
    field(pool, "ksnote", "reports_to", &target, "sets").await;
    sqlx::query(
        "INSERT INTO field_link (entity, `key`, ordinal, target)
                 VALUES ('ksnote', 'reports_to', 1, ?)",
    )
    .bind(&target)
    .execute(pool)
    .await
    .expect("the preceding rule's link row");
    field(pool, "kstau1", "marked_control", "@#ksheir", "sets").await;
    sqlx::query(
        "INSERT INTO field_link (entity, `key`, ordinal, target)
                 VALUES ('kstau1', 'marked_control', 1, 'ksheir')",
    )
    .execute(pool)
    .await
    .expect("the already marked control");
    sqlx::query("DELETE FROM store_pass WHERE name = 'field value links marked'")
        .execute(pool)
        .await
        .expect("the old store has not run the marking");
    sqlx::query(
        "INSERT IGNORE INTO store_pass (name, finished_at)
                 VALUES ('field value lowering', '2026-10-08T14:02:10Z')",
    )
    .execute(pool)
    .await
    .expect("the preceding pass had finished");
}

async fn merge_rows(pool: &sqlx::MySqlPool) -> MergeRows {
    sqlx::query_as(
        "SELECT merge_no, ordinal, folded, survivor, merged_at, session, folded_id,
                          survivor_id FROM merge_event ORDER BY merge_no, ordinal",
    )
    .fetch_all(pool)
    .await
    .expect("the exact merge-event rows")
}

// LEGACY_ROW(old-merge-event): work:q-gate-binary-coverage
async fn old_merges(pool: &sqlx::MySqlPool) -> MergeRows {
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds used by the observed merges");
    for (chain, keep) in [
        (
            [
                ("thing:zeta", "ksmra1"),
                ("thing:eta", "ksmrb1"),
                ("thing:theta", "ksmrc1"),
            ],
            false,
        ),
        (
            [
                ("thing:phi", "ksctla"),
                ("thing:chi", "ksctlb"),
                ("thing:psi", "ksctlc"),
            ],
            true,
        ),
    ] {
        for (handle, key) in chain {
            entity(pool, handle, key, None).await;
        }
        let [old, middle, last] = chain.map(|(handle, _)| EntityId(handle.into()));
        for content in ["kept the first ledger", "kept the second ledger"] {
            memory
                .capture(NewFact::about(
                    middle.clone(),
                    content,
                    jiff::civil::date(2026, 9, 1),
                ))
                .await
                .expect("the history the older merge left on its survivor")
                .written()
                .expect("the claim lands");
        }
        for (gone, kept) in [(&old, &middle), (&middle, &last)] {
            memory
                .merge(
                    gone,
                    kept,
                    Some("filed twice"),
                    jiff::civil::date(2026, 9, 2),
                    &EntityId("bot:assistant".into()),
                    None,
                )
                .await
                .expect("the observed merge lands");
        }
        if !keep {
            sqlx::query("DELETE FROM merge_event WHERE folded = ?")
                .bind(chain[0].1)
                .execute(pool)
                .await
                .expect("the merge from before events existed");
        }
    }
    sqlx::query("DELETE FROM store_pass WHERE name = 'old merge events'")
        .execute(pool)
        .await
        .expect("the old merge is still owed its event");
    sqlx::query(
        "INSERT IGNORE INTO field_link (entity, `key`, ordinal, target)
         SELECT w.entity, w.`key`, w.ordinal, COALESCE(e.badge, e.id)
         FROM field_write w JOIN entity e ON e.id = w.value
         WHERE w.`key` = 'merged_from'
           AND w.entity IN ('ksmrb1', 'ksmrc1', 'ksctlb', 'ksctlc')",
    )
    .execute(pool)
    .await
    .expect("the historical engine links held before marking");
    merge_rows(pool).await
}

async fn prepared(label: &str) -> Planted {
    let restored = restore_the_fixture(label).await;
    let holder = std::fs::read_to_string(format!("{}/role_holder.txt", fixture_dir()))
        .expect("the recorded holder")
        .trim()
        .to_string();
    let old = (jiff::Timestamp::now() - jiff::SignedDuration::from_mins(10)).to_string();
    let carried = (jiff::Timestamp::now() - jiff::SignedDuration::from_mins(2)).to_string();
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let pool = store.pool();
    jojobot_adapters::dolt::migrate::run(pool)
        .await
        .expect("the planting schema");
    old_mail(pool, &holder).await;
    old_classes(pool).await;
    unstamped(pool).await;
    forwarded(pool).await;
    describing(pool).await;
    unreadable(pool).await;
    half_moved(pool, &holder, &old, &carried).await;
    let tail_before = lease_tail(pool, &old).await;
    rewritten_list(pool).await;
    note_ledger(pool).await;
    unmarked_link(pool).await;
    let retained_merges = old_merges(pool).await;
    store.stop().await;
    Planted {
        restored,
        holder,
        old,
        carried,
        tail_before,
        retained_merges,
    }
}

fn reached(answer: &Value) -> Vec<String> {
    let mut handles: Vec<_> = answer["objects"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|o| o["connected"].as_array().into_iter().flatten())
        .filter_map(|o| o["id"].as_str().map(str::to_string))
        .collect();
    handles.sort();
    handles
}

async fn read(surface: &Surface, verb: &str, args: Value) -> Value {
    let raw = surface.call(verb, args).await;
    serde_json::from_str(&raw).unwrap_or_else(|_| panic!("{verb} did not answer JSON"))
}

#[tokio::test]
async fn the_kitchen_sink_boots_once_and_reads_every_tolerated_shape() {
    let Planted {
        restored,
        holder,
        old,
        carried,
        tail_before,
        retained_merges,
    } = prepared("kitchen-sink").await;
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        holder_key,
        held_before,
        ..
    } = restored;

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("the one head boot");
    // Taking delivery can renew every role this run holds. Observe the boot's
    // old tail and both migrated moments before making an authenticated call.
    let before = read(
        &surface,
        "recall",
        json!({"subject": LEASE, "history": "claimed_at", "history_most": 100}),
    )
    .await;
    let waiting = read(&surface, "recall", json!({"subject": WAITING})).await;
    let moved = read(&surface, "recall", json!({"subject": CARRIED})).await;
    let bot = read(&surface, "recall", json!({"subject": "bot:assistant"})).await;
    let sid = holder_key.expect("the recording is of a build that keys its runs");
    // Collect independent observations before asserting, so a fault names its family
    // and also proves which other families still answered on the same boot.
    let mut results = Vec::new();
    let mail = read(
        &surface,
        "read_message",
        json!({"message_id": MAIL, "sid": sid}),
    )
    .await;
    results.push(("old-mail-address", mail["body"] == WORDS));
    let tau = read(&surface, "recall", json!({"subject": TAU})).await;
    let fields = &tau["objects"][0]["fields"];
    let claim = read(
        &surface,
        "recall",
        json!({"subject": TAU, "history_record": format!("{TAU}#f1"), "facts": false}),
    )
    .await;
    results.push((
        "legacy-write-classes",
        fields["bag_legacy"] == "legacy"
            && fields["bag_own"] == "own"
            && fields["bag_sets"] == "sets"
            && fields.get("bag_stays").is_none(),
    ));
    let holder_read = read(&surface, "recall", json!({"subject": HOLDER})).await;
    results.push((
        "forwarded-field-link",
        holder_read["objects"][0]["fields"]["watches"] == HEIR,
    ));
    let starry = read(&surface, "recall", json!({"answers_type": "legacy-starry"})).await;
    let pantry = read(&surface, "recall", json!({"answers_type": "legacy-pantry"})).await;
    let clean = read(&surface, "search", json!({"fits_type": "legacy-clean"})).await;
    results.push((
        "stored-describing-key",
        starry["status"] != "blocked"
            && starry["count"] == 0
            && starry.to_string().contains("starred")
            && pantry["status"] != "blocked"
            && pantry.to_string().contains("subject")
            && clean.to_string().contains(TAU),
    ));
    let owed = read(
        &surface,
        "recall",
        json!({"fields": [{"key": "due_on"}], "overdue": {}}),
    )
    .await;
    let out = read(
        &surface,
        "recall",
        json!({"subject": "work:theta",
        "follow": {"relation": "depends_on", "direction": "out"}}),
    )
    .await;
    let mut list_ok = reached(&out) == ["work:lambda"]
        && out["objects"][0]["fields"]["depends_on"] == "work:lambda";
    for (target, wanted) in [
        ("work:iota", false),
        ("work:kappa", false),
        ("work:lambda", true),
    ] {
        let answer = read(
            &surface,
            "recall",
            json!({"subject": target,
            "follow": {"relation": "depends_on", "direction": "in"}}),
        )
        .await;
        list_ok &= reached(&answer).contains(&"work:theta".to_string()) == wanted;
    }
    let ready = read(
        &surface,
        "recall",
        json!({"view": "ready", "subject": "work:theta"}),
    )
    .await;
    let blockers: Vec<_> = ready["objects"][0]["blocked_by"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|b| b["handle"].as_str())
        .collect();
    results.push((
        "rewritten-reference-list",
        list_ok && blockers == ["work:lambda"],
    ));
    let repaired = read(
        &surface,
        "recall",
        json!({"subject": "thing:theta", "facts": true}),
    )
    .await;
    let ledger_history = ["kept the first ledger", "kept the second ledger"]
        .iter()
        .all(|content| repaired.to_string().contains(*content));
    results.push((
        "unreadable-loop",
        owed["objects"].as_array().is_some_and(|objects| {
            objects.iter().any(|o| {
                o["id"] == FERN && o["unreadable_because"].to_string().contains("cadence_days")
            })
        }) && booted
            .log
            .iter()
            .any(|line| line.contains("due days: stamped what could not be read")),
    ));
    results.push((
        "half-moved-role-claims",
        waiting["objects"][0]["fields"]["holder"] == holder
            && waiting["objects"][0]["fields"]["claimed_at"] == old
            && moved["objects"][0]["fields"]["claimed_at"] == carried
            && moved["objects"][0]["fields"]["holder"] == holder
            && bot["objects"][0]["fields"]
                .as_object()
                .is_some_and(|fields| !fields.keys().any(|key| key.starts_with("role/"))),
    ));
    let renewed = read(
        &surface,
        "journal",
        json!({"entry": "renewed the old lease tail", "sid": sid}),
    )
    .await;
    let after = read(
        &surface,
        "recall",
        json!({"subject": LEASE, "history": "claimed_at", "history_most": 100}),
    )
    .await;
    let rival = read(
        &surface,
        "start_here",
        json!({"bot": "assistant", "claim": "upgrade-fixture-holder",
                                                 "resume": "new", "brief": true}),
    )
    .await;
    results.push((
        "appended-lease-tail",
        before["objects"][0]["fields"]["claimed_at"] == old
            && before["objects"][0]["history"]["count"] == tail_before.len()
            && renewed["status"] != "blocked"
            && after["objects"][0]["fields"]["claimed_at"] != old
            && after["objects"][0]["history"]["count"] == tail_before.len()
            && rival["session"]["claim"]["status"] == "refused"
            && rival["session"]["claim"]["holder"] == holder,
    ));
    let unchanged: Vec<_> = held_before
        .into_iter()
        .filter(|(handle, key, _)| !(handle == LEASE && key == "claimed_at"))
        .collect();
    for (case, okay) in &results {
        if !okay {
            eprintln!("kitchen-sink {case}: RED");
        }
    }
    assert_every_thing_holds_what_it_held(&surface, &git_ref, &unchanged).await;
    surface.finish().await;
    booted.stop().await;

    let mut store = open_the_store(&state_dir, store_port).await;
    let forwarded_value: String = sqlx::query_scalar(
        "SELECT value FROM field_write WHERE entity = 'kshold' AND `key` = 'watches'",
    )
    .fetch_one(store.pool())
    .await
    .expect("the repaired forward's stored value");
    let forwarded_links: Vec<String> = sqlx::query_scalar(
        "SELECT target FROM field_link WHERE entity = 'kshold' AND `key` = 'watches'",
    )
    .fetch_all(store.pool())
    .await
    .expect("the repaired forward's stored index");
    if let Some((_, okay)) = results
        .iter_mut()
        .find(|(case, _)| *case == "forwarded-field-link")
    {
        *okay &= forwarded_value == "@#ksheir" && forwarded_links == ["ksheir"];
    }
    let memory = DoltMemory::open(store.pool().clone());
    let trail = memory
        .history(&EntityId(TAU.into()), "unstamped")
        .await
        .expect("the unstamped history");
    results.push((
        "null-written-at",
        fields["unstamped"] == "kept"
            && trail.len() == 1
            && trail[0].written_at.is_none()
            && claim["objects"][0]["record_history"]["writes"]
                .as_array()
                .is_some_and(|writes| {
                    writes.len() == 1
                        && writes[0]["nth"] == 1
                        && writes[0]["content"] == "the old bags and the newer bag beside them"
                        && writes[0].get("written_at").is_none()
                }),
    ));
    let role = badge(store.pool(), LEASE).await;
    let tail = lease_rows(store.pool(), &role).await;
    let newest = tail_before.len() - 1;
    let older_intact = tail.len() == tail_before.len()
        && tail[..newest] == tail_before[..newest]
        && tail[newest].0 == tail_before[newest].0
        && tail[newest].1 != tail_before[newest].1
        && tail[newest].2 != tail_before[newest].2;
    if let Some((_, okay)) = results
        .iter_mut()
        .find(|(case, _)| *case == "appended-lease-tail")
    {
        *okay &= older_intact;
    }
    let messages: Vec<String> = sqlx::query_scalar(
        "SELECT body FROM message
        WHERE mailbox = 'upgrade-fixture-ledger' AND sender = 'jojobot'",
    )
    .fetch_all(store.pool())
    .await
    .expect("the new messages in the old ledger's box");
    let sent: std::collections::BTreeSet<_> = messages
        .iter()
        .filter_map(|body| {
            body.trim_end()
                .lines()
                .last()
                .and_then(|line| line.strip_prefix("Notes: "))
        })
        .flat_map(|line| line.split(',').map(str::trim))
        .collect();
    let ledger = memory
        .fields(&EntityId(NOTE_BOT.into()))
        .await
        .expect("the retained ledger");
    results.push((
        "release-note-ledger",
        !sent.contains(HELD_NOTE)
            && sent.contains(NEW_NOTE)
            && ledger["release_notes_told"]
                .split(',')
                .map(str::trim)
                .any(|id| id == HELD_NOTE),
    ));
    let bot_key = badge(store.pool(), "bot:assistant").await;
    let marked: String = sqlx::query_scalar(
        "SELECT value FROM field_write
        WHERE entity = 'ksnote' AND `key` = 'reports_to'",
    )
    .fetch_one(store.pool())
    .await
    .expect("the old bare badge after marking");
    let mark_links: Vec<String> = sqlx::query_scalar(
        "SELECT target FROM field_link
        WHERE entity = 'ksnote' AND `key` = 'reports_to'",
    )
    .fetch_all(store.pool())
    .await
    .expect("the marking's link row");
    let control: String = sqlx::query_scalar(
        "SELECT value FROM field_write
        WHERE entity = 'kstau1' AND `key` = 'marked_control'",
    )
    .fetch_one(store.pool())
    .await
    .expect("the marked control");
    let passes: Vec<String> = sqlx::query_scalar("SELECT name FROM store_pass")
        .fetch_all(store.pool())
        .await
        .expect("the finished pass markers");
    results.push((
        "unmarked-field-link",
        marked == jojobot_domain::memory::marked_stored_handle(&bot_key)
            && mark_links == [bot_key]
            && control == "@#ksheir"
            && passes.iter().any(|p| p == "field value links marked"),
    ));
    let events = merge_rows(store.pool()).await;
    let chain: Vec<_> = events
        .iter()
        .filter(|row| row.1 == 0 && matches!(row.2.as_str(), "ksmra1" | "ksmrb1"))
        .collect();
    assert_eq!(chain.len(), 2, "both merges retain their event");
    assert!(
        chain[0].2 == "ksmrb1" && chain[1].2 == "ksmra1" && chain[0].0 < chain[1].0,
        "the repaired event sorts after the later merge: {chain:?}"
    );
    results.push((
        "old-merge-event",
        ledger_history
            && events.len() == retained_merges.len() + 1
            && retained_merges.iter().all(|row| events.contains(row))
            && events
                .iter()
                .filter(|row| row.1 == 0 && row.2 == "ksmra1" && row.3 == "ksmrb1")
                .count()
                == 1
            && passes.iter().any(|p| p == "old merge events"),
    ));
    store.stop().await;
    for (case, okay) in &results {
        eprintln!(
            "kitchen-sink {case}: {}",
            if *okay { "GREEN" } else { "RED" }
        );
    }
    assert_eq!(results.len(), 12, "every tolerated family was observed");
    assert!(
        results.iter().all(|(_, okay)| *okay),
        "kitchen-sink observations: {results:?}; boot role fields: waiting={}, expected moment={old}; \
         carried={}, expected moment={carried}; claim history={}",
        waiting["objects"][0]["fields"],
        moved["objects"][0]["fields"],
        claim["objects"][0]["record_history"],
    );
    drop(scratch);
}

async fn account_links(restored: &Restored) -> Vec<(String, i64, String)> {
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let rows = sqlx::query_as(
        "SELECT entity, ordinal, target FROM field_link WHERE `key` = 'merged_from'
         ORDER BY entity, ordinal, target",
    )
    .fetch_all(store.pool())
    .await
    .expect("the historical engine account links");
    store.stop().await;
    rows
}

async fn repair_tables(restored: &Restored) -> Vec<String> {
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let mut hashes = Vec::new();
    for table in ["field_write", "field_link", "merge_event", "entity"] {
        hashes.push(
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "SELECT DOLT_HASHOF_TABLE('{table}')"
            )))
            .fetch_one(store.pool())
            .await
            .expect("the repair table's hash"),
        );
    }
    let marked: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM store_pass WHERE name = 'old merge events'")
            .fetch_one(store.pool())
            .await
            .expect("the repair marker");
    assert_eq!(marked, 1, "the repair's marker is present");
    store.stop().await;
    hashes
}

async fn a_second_boot_keeps_the_store(label: &str, settled: &str, replay: bool) {
    let planted = prepared(label).await;
    let restored = planted.restored;
    let links = account_links(&restored).await;
    assert!(
        !links.is_empty(),
        "the planted merges must supply account links"
    );
    let first = boot_current(&restored.state_dir, restored.store_port, &restored.git_ref).await;
    first.stop().await;
    assert_eq!(
        account_links(&restored).await,
        links,
        "the first marking boot changed an engine account link row"
    );
    let mut after_first = dump_store(&restored.state_dir).await;
    if replay {
        let repaired = repair_tables(&restored).await;
        // A death after the repair commits but before its marker lands.
        let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
        sqlx::query("DELETE FROM store_pass WHERE name = 'old merge events'")
            .execute(store.pool())
            .await
            .expect("the interrupted repair has no marker");
        store.stop().await;
        let repeated =
            boot_current(&restored.state_dir, restored.store_port, &restored.git_ref).await;
        let ran = repeated
            .log
            .iter()
            .any(|line| line.contains("repair pass finished") && line.contains("old merge events"));
        repeated.stop().await;
        assert!(ran, "the missing-marker boot must repeat the repair");
        assert_eq!(
            repair_tables(&restored).await,
            repaired,
            "the repeated repair changed a table"
        );
        // The replacement marker has a new finished_at; the settled boot keeps it.
        after_first = dump_store(&restored.state_dir).await;
    }
    let second = boot_current(&restored.state_dir, restored.store_port, &restored.git_ref).await;
    let quiet = second.log.iter().any(|line| line.contains(settled));
    second.stop().await;
    assert!(
        quiet,
        "the marked pass must report that it is already done: {settled}"
    );
    assert!(
        dump_store(&restored.state_dir).await == after_first,
        "the second boot changed a row of the same prepared store"
    );
}

#[tokio::test]
async fn link_marking_changes_no_row_on_the_second_boot() {
    a_second_boot_keeps_the_store(
        "kitchen-marked-again",
        "field value links marked: already done",
        false,
    )
    .await;
}

#[tokio::test]
async fn old_merge_repair_changes_no_row_on_the_second_boot() {
    a_second_boot_keeps_the_store("kitchen-repaired-again", "repair pass already done", true).await;
}
