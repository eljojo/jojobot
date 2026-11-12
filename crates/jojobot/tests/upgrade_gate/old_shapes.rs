//! Stored shapes that 9.3.3 reads differently from the build that wrote them.
//!
//! Each case restores the last deployed store, writes below the surface the rows
//! that build would have written, boots the head over them and reads the result
//! through the served surface. A case is held to the boot's own account of itself
//! (no unexpected warning or error) as every other gate case is.
use super::kitchen_sink::{badge, entity, field, record};
use super::*;
use serde_json::Value;

fn parsed(what: &str, git_ref: &str, body: &str) -> Value {
    serde_json::from_str(body)
        .unwrap_or_else(|_| panic!("recorded at {git_ref}: {what} did not answer JSON: {body}"))
}

/// The head, connected to as the assistant in a run of its own.
async fn as_the_assistant(booted: &Booted, git_ref: &str) -> (Surface, String) {
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let boot = parsed(
        "booting as the assistant",
        git_ref,
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new"}),
            )
            .await,
    );
    let sid = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("recorded at {git_ref}: no run to write in: {boot}"))
        .to_string();
    (surface, sid)
}

/// A teaching delivered on the restored store stays spent after a restart.
/// Another note may have been deferred by the receipt budget; it may still ship.
#[tokio::test]
async fn a_run_on_the_restored_store_is_not_taught_the_same_note_after_a_restart() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_the_fixture("teaching-once").await;
    let first = boot_current(&state_dir, store_port, &git_ref).await;
    let (surface, sid) = as_the_assistant(&first, &git_ref).await;
    let made = parsed(
        "the thing to write about",
        &git_ref,
        &surface
            .call(
                "add_entity",
                json!({"kind": "thing", "handle": "gate-pointer", "name": "Gate Pointer",
                       "source": "test", "sid": sid}),
            )
            .await,
    );
    assert_eq!(made["id"], "thing:gate-pointer", "{made}");
    let one = parsed(
        "the first claim",
        &git_ref,
        &surface
            .call(
                "capture",
                json!({"subject": "thing:gate-pointer", "content": "the first claim", "sid": sid}),
            )
            .await,
    );
    assert_ne!(one["status"], "blocked", "{one}");
    let taught = one["teaching"]
        .as_array()
        .filter(|notes| !notes.is_empty())
        .expect("the first capture delivered a teaching")
        .clone();
    surface.finish().await;
    first.stop().await;

    let second = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", second.http_port))
        .await
        .expect("the restarted head");
    let two = parsed(
        "the second claim in the same run",
        &git_ref,
        &surface
            .call(
                "capture",
                json!({"subject": "thing:gate-pointer", "content": "the second claim", "sid": sid}),
            )
            .await,
    );
    assert_ne!(two["status"], "blocked", "{two}");
    for note in two["teaching"].as_array().into_iter().flatten() {
        assert!(
            !taught.contains(note),
            "the restored store taught the same run twice: {one} {two}"
        );
    }
    surface.finish().await;
    second.stop().await;
    drop(scratch);
}

/// ── (d) A thought stored over its cap ───────────────────────────────────────
const LONG_DETAILS: usize = 400;

/// A thought the way a build that measured only the content kept it: a short line
/// and a long reason, on `subject`, drawing a connection to `object`.
async fn plant_a_long_thought(pool: &sqlx::MySqlPool, subject: &str, id: &str, object: &str) {
    let subject = badge(pool, subject).await;
    let object = badge(pool, object).await;
    let details = "z".repeat(LONG_DETAILS);
    sqlx::query(
        "INSERT INTO fact (entity, id, content, details, provenance, standing, status,
                           recorded_at, edge_shape, edge_object, inserted_at)
         VALUES (?, ?, 'a short line', ?, 'inference', 'open', 'active', '2026-10-08',
                 'connection', ?, '2026-10-08T14:02:10Z')",
    )
    .bind(&subject)
    .bind(id)
    .bind(&details)
    .bind(&object)
    .execute(pool)
    .await
    .expect("the long thought");
    sqlx::query(
        "INSERT INTO fact_write (entity, fact_id, ordinal, content, details, provenance,
                                 standing, status, recorded_at, edge_shape, edge_object,
                                 inserted_at, written_at)
         VALUES (?, ?, 1, 'a short line', ?, 'inference', 'open', 'active', '2026-10-08',
                 'connection', ?, '2026-10-08T14:02:10Z', '2026-10-08T14:02:10Z')",
    )
    .bind(&subject)
    .bind(id)
    .bind(&details)
    .bind(&object)
    .execute(pool)
    .await
    .expect("the long thought's first write");
}

/// **A thought stored with content plus details over the cap is still read, an
/// edit that touches neither lands, and a fold that would bring it refuses with
/// a way forward.** 9.3.3 counts the details against the cap; a row the earlier
/// build stored over it is neither rewritten nor made unreadable. The cap is
/// 200 for a bot that carries `thought_capacity` and no `thought_body_cap`.
#[tokio::test]
async fn a_thought_stored_over_its_cap_is_read_and_edited_and_refuses_a_merge_with_a_way_forward() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_the_fixture("thought-over-cap").await;
    let reports = usize::from(recording_holds_the_plain_ambiguous_handle(&git_ref));
    // **The whole room is the earlier build's rows, planted before the head's first
    // boot.** A real upgrade has the old rows in the store before the head imports
    // them, and the import runs once, at that first boot. Rows planted under a head
    // that has already imported are written beneath the one writer (decision 292),
    // so the State would not hold them and an edit of the thought would find no
    // claim. The room is two bots and a thing, each bot with a ceiling of seven.
    let mut store = open_the_store(&state_dir, store_port).await;
    for (handle, badge_text) in [
        ("bot:gate-thinker", "gthink"),
        ("bot:gate-heir", "gheir"),
        ("thing:gate-whisk", "gwhisk"),
    ] {
        entity(store.pool(), handle, badge_text, None).await;
    }
    // A bot of the earlier build has its box, which the release notes go to.
    for (name, owner) in [
        ("gate-thinker", "bot:gate-thinker"),
        ("gate-heir", "bot:gate-heir"),
    ] {
        sqlx::query("INSERT INTO mailbox (name, owner) VALUES (?, ?)")
            .bind(name)
            .bind(owner)
            .execute(store.pool())
            .await
            .expect("the old bot's empty box");
    }
    for badge_text in ["gthink", "gheir"] {
        record(store.pool(), badge_text, "the room holds seven").await;
        field(store.pool(), badge_text, "thought_capacity", "7", "sets").await;
    }
    // The bot's own thought, and a thing's claim that a fold would carry over to a
    // bot as a thought.
    plant_a_long_thought(store.pool(), "bot:gate-thinker", "f90", "bot:assistant").await;
    plant_a_long_thought(store.pool(), "thing:gate-whisk", "f90", "bot:assistant").await;
    store.stop().await;

    let booted = boot_current_expecting(&state_dir, store_port, &git_ref, Some(reports), &[]).await;
    let (surface, sid) = as_the_assistant(&booted, &git_ref).await;
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not hold: {body}")
    };

    // ── it reads, in full ───────────────────────────────────────────────────
    let read = surface
        .call(
            "recall",
            json!({"subject": "bot:gate-thinker", "facts": true, "details": true,
                   "status": "active", "sid": sid}),
        )
        .await;
    if !read.contains("a short line") || !read.contains(&"z".repeat(LONG_DETAILS)) {
        fail("the stored thought reading back whole", &read);
    }

    // ── an edit that touches neither content nor details lands ──────────────
    let edited = surface
        .call(
            "update_fact",
            json!({"address": "bot:gate-thinker#f90", "stale_after": "2027-01-01", "sid": sid}),
        )
        .await;
    if edited.contains("\"status\":\"blocked\"") || edited.contains("ThoughtTooLong") {
        fail("an edit that touches neither content nor details", &edited);
    }

    // ── an edit of the details is judged by the new rule ────────────────────
    let rewritten = parsed(
        "an edit of the details",
        &git_ref,
        &surface
            .call(
                "update_fact",
                json!({"address": "bot:gate-thinker#f90",
                       "details": "y".repeat(LONG_DETAILS), "sid": sid}),
            )
            .await,
    );
    if rewritten["refusal_code"] != "ThoughtTooLong" {
        fail(
            "a rewrite of the long details refused as ThoughtTooLong",
            &rewritten.to_string(),
        );
    }

    // ── a fold that would bring the claim refuses, with a way forward ───────
    let folded = parsed(
        "the fold",
        &git_ref,
        &surface
            .call(
                "merge_entities",
                json!({"duplicate": "thing:gate-whisk", "survivor": "bot:gate-heir",
                       "reason": "the gate folds the whisk", "sid": sid}),
            )
            .await,
    );
    if folded["refusal_code"] != "MergeThoughtTooLong" || folded["fix_by"] != "change" {
        fail(
            "the fold refused as MergeThoughtTooLong",
            &folded.to_string(),
        );
    }
    let way = folded["how_to_proceed"].as_str().unwrap_or_default();
    if !way.contains("#f90") || !way.contains("archive") || !way.contains("merge_entities") {
        fail("the refusal naming the thought and the way forward", way);
    }
    surface.finish().await;
    booted.stop().await;
    drop(scratch);
}

/// ── (e) A new message its sender replaced ───────────────────────────────────
const FIRST_DATE: &str = "gate0ldfirst";
const SECOND_DATE: &str = "gate0ldsecond";

/// **A replaced message the earlier build left in `new`.** The row pair is what
/// that build wrote: the second names the first in `supersedes`, and both read
/// `new`.
async fn plant_a_replaced_message(pool: &sqlx::MySqlPool) {
    let sender = badge(pool, "bot:assistant").await;
    for (id, body, replaces) in [
        (FIRST_DATE, "the party is on the third", None),
        (SECOND_DATE, "the party is on the fourth", Some(FIRST_DATE)),
    ] {
        sqlx::query(
            "INSERT INTO message (id, mailbox, ordinal, body, subject, sender, sent_at, state,
                                  sender_mail_waiting_at_send, supersedes)
             SELECT ?, 'assistant', COALESCE(MAX(ordinal), 0) + 1, ?, 'the party', ?,
                    '2026-10-08T13:55:00Z', 'new', 0, ? FROM message WHERE mailbox = 'assistant'",
        )
        .bind(id)
        .bind(body)
        .bind(&sender)
        .bind(replaces)
        .execute(pool)
        .await
        .expect("the replaced pair");
    }
}

/// **A message a sender replaced under the earlier build is superseded on the
/// head: counted apart, never delivered, readable by its id.** This is the change
/// the release note `a-replaced-message-is-final-and-never-delivered` names. The
/// case holds the head's reading of the old rows to it: the box counts one
/// superseded, delivers the correction alone, and closing the pair leaves the
/// first one untouched.
#[tokio::test]
async fn a_message_the_earlier_build_left_replaced_in_new_reads_as_superseded() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_the_fixture("replaced-message").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    plant_a_replaced_message(store.pool()).await;
    store.stop().await;

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let (surface, sid) = as_the_assistant(&booted, &git_ref).await;
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not hold: {body}")
    };

    let counted = parsed(
        "counting the box",
        &git_ref,
        &surface
            .call("read_mailbox", json!({"counts_only": true, "sid": sid}))
            .await,
    );
    if counted["counts"]["superseded"] != 1 {
        fail(
            "the box counting one superseded message",
            &counted.to_string(),
        );
    }

    // **By message, not by text**: the correction's own envelope names the id it
    // replaces, so the id may appear in the answer; the replaced message may not.
    let delivered_raw = surface.call("read_mailbox", json!({"sid": sid})).await;
    let delivered = parsed("the delivery", &git_ref, &delivered_raw);
    let ids: Vec<&str> = delivered["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|message| message["id"].as_str())
        .collect();
    if !ids.contains(&SECOND_DATE) || !delivered_raw.contains("on the fourth") {
        fail("the correction delivered", &delivered_raw);
    }
    if ids.contains(&FIRST_DATE) || delivered_raw.contains("on the third") {
        fail(
            "the replaced message left out of the delivery",
            &delivered_raw,
        );
    }

    let read = parsed(
        "reading the replaced message by its id",
        &git_ref,
        &surface
            .call(
                "read_message",
                json!({"message_id": FIRST_DATE, "sid": sid}),
            )
            .await,
    );
    if !read.to_string().contains("\"state\":\"superseded\"")
        || !read.to_string().contains(SECOND_DATE)
    {
        fail(
            "the replaced message naming its state and its correction",
            &read.to_string(),
        );
    }

    let closed = parsed(
        "closing the pair",
        &git_ref,
        &surface
            .call(
                "mark_processed",
                json!({"message_ids": [FIRST_DATE, SECOND_DATE], "sid": sid}),
            )
            .await,
    );
    let named = closed["superseded"]
        .as_array()
        .is_some_and(|ids| ids.iter().any(|id| id == FIRST_DATE));
    let processed = closed["ids"].to_string();
    if !named || processed.contains(FIRST_DATE) || !processed.contains(SECOND_DATE) {
        fail(
            "closing naming the replaced message apart and processing the correction",
            &closed.to_string(),
        );
    }
    let after = parsed(
        "counting the box after the close",
        &git_ref,
        &surface
            .call("read_mailbox", json!({"counts_only": true, "sid": sid}))
            .await,
    );
    if after["counts"]["superseded"] != 1 {
        fail(
            "the replaced message still counted superseded",
            &after.to_string(),
        );
    }
    surface.finish().await;
    booted.stop().await;
    drop(scratch);
}

/// ── (g) A numbered address in an old field value ────────────────────────────
const ORDINANCES: &str = "topic:gate-ordinances";
const FINE: &str = "thing:gate-fine";
const POINTER: &str = "thing:gate-pointer";
const NUMBERED: &str = "topic:gate-ordinances/2";

/// A collection that numbers its members by `number`, one member holding 2, and
/// a thing whose field names that member by its numbered address, as text. The
/// pass that marks field values had not finished on this store.
async fn plant_a_numbered_address_in_a_field_value(pool: &sqlx::MySqlPool) {
    entity(pool, ORDINANCES, "gordin1", None).await;
    record(pool, "gordin1", "the ordinances of the town").await;
    field(pool, "gordin1", "numbers_by", "number", "sets").await;
    entity(pool, FINE, "gfine01", Some("gordin1")).await;
    record(pool, "gfine01", "a fine for parking").await;
    field(pool, "gfine01", "number", "2", "sets").await;
    entity(pool, POINTER, "gpoint1", None).await;
    record(pool, "gpoint1", "points at an ordinance by its number").await;
    field(
        pool,
        "gpoint1",
        "ruled_by",
        &format!("{ORDINANCES}/2"),
        "sets",
    )
    .await;
    sqlx::query("DELETE FROM store_pass WHERE name = 'field value links marked'")
        .execute(pool)
        .await
        .expect("the marking has not finished on this store");
    sqlx::query(
        "INSERT IGNORE INTO store_pass (name, finished_at)
         VALUES ('field value lowering', '2026-10-08T14:02:10Z')",
    )
    .execute(pool)
    .await
    .expect("the preceding pass had finished");
}

/// **A field value holding `<collection>/<N>` as text, on a store where the
/// marking pass had not finished, is named once and left as text, and the second
/// boot changes nothing.** The marking pass keeps its name and reads a numbered
/// address as the head's resolver does. The older lowering pass had decided plain
/// handle text, so the address stays text and the boot says so once; the earlier
/// body of the pass did not know the address names a member and said nothing.
#[tokio::test]
async fn a_numbered_address_in_an_old_field_value_is_named_once_and_left_as_text() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_the_fixture("numbered-field-value").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    jojobot_adapters::dolt::migrate::run(store.pool())
        .await
        .expect("the planting schema");
    plant_a_numbered_address_in_a_field_value(store.pool()).await;
    store.stop().await;

    // **The older lowering pass had finished**, so the marking pass leaves a plain
    // handle as the text the older pass decided on, and says so once. The head
    // reads `<collection>/<N>` as a handle that names one member: the line below
    // names the item, which the earlier body of the pass did not know to name.
    let reports = usize::from(recording_holds_the_plain_ambiguous_handle(&git_ref));
    let first = boot_current_expecting(
        &state_dir,
        store_port,
        &git_ref,
        Some(reports),
        &["FIELD VALUE LEFT AS TEXT"],
    )
    .await;
    let named = first
        .log
        .iter()
        .filter(|line| line.contains("FIELD VALUE LEFT AS TEXT") && line.contains(NUMBERED))
        .count();
    first.stop().await;
    assert_eq!(
        named, 1,
        "recorded at {git_ref}: the boot did not name the numbered address once"
    );
    let mut store = open_the_store(&state_dir, store_port).await;
    let linked: Vec<(String,)> = sqlx::query_as(
        "SELECT target FROM field_link WHERE entity = 'gpoint1' AND `key` = 'ruled_by'",
    )
    .fetch_all(store.pool())
    .await
    .expect("the links of the pointer");
    store.stop().await;
    assert!(
        linked.is_empty(),
        "recorded at {git_ref}: the numbered address was linked against the older pass: {linked:?}"
    );
    let after_first = dump_store(&state_dir).await;

    // **A bare second boot, as every other dump comparison in this file is**: a
    // read through the surface is attributed and writes, so the read comes in a
    // third boot after the dumps are taken.
    let second = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
    second.stop().await;
    assert_eq!(
        after_first,
        dump_store(&state_dir).await,
        "recorded at {git_ref}: the second boot changed the store the first boot marked"
    );
    let second = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", second.http_port))
        .await
        .expect("connecting to the current binary");
    let read = surface.call("recall", json!({"subject": POINTER})).await;
    assert!(
        read.contains(NUMBERED),
        "recorded at {git_ref}: the pointer did not keep the numbered address as written: {read}"
    );
    surface.finish().await;
    second.stop().await;
    drop(scratch);
}

/// ── (h) A role live at the upgrade ──────────────────────────────────────────
/// **A role live when the head arrives has no heartbeat row, keeps its lease, and
/// gets its row at its next renewal.** The table is new in 9.3.3 and nothing
/// backfills it; the lease is read from the role's own moment until then.
#[tokio::test]
async fn a_role_live_at_the_upgrade_keeps_its_lease_and_gets_its_heartbeat_row_at_the_next_renewal()
{
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        holder_key,
        ..
    } = restore_the_fixture("heartbeat-wait").await;
    let role_holder = std::fs::read_to_string(format!("{}/role_holder.txt", fixture_dir()))
        .expect("the recorded holder")
        .trim()
        .to_string();
    let role_key = holder_key.expect("the recording is of a build that keys its runs");

    // The head's first boot makes no call as the holder, so nothing renews the lease.
    let first = boot_current(&state_dir, store_port, &git_ref).await;
    first.stop().await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let role = badge(store.pool(), "role:upgrade-fixture-holder").await;
    let waiting: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM role_lease_heartbeat WHERE role = ?")
            .bind(&role)
            .fetch_one(store.pool())
            .await
            .expect("the heartbeat rows of the role");
    assert_eq!(
        waiting, 0,
        "recorded at {git_ref}: the head made a heartbeat row for a role nobody renewed"
    );
    let aged = (jiff::Timestamp::now() - jiff::SignedDuration::from_mins(10)).to_string();
    sqlx::query("UPDATE field_write SET value = ? WHERE `key` = 'claimed_at' AND entity = ?")
        .bind(&aged)
        .bind(&role)
        .execute(store.pool())
        .await
        .expect("the lease moment is aged");
    store.stop().await;

    let second = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", second.http_port))
        .await
        .expect("connecting to the current binary");
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not hold: {body}")
    };
    // The lease is held with no row to say so: a rival is refused and told who holds it.
    let rival = parsed(
        "the rival's boot",
        &git_ref,
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "claim": "upgrade-fixture-holder",
                       "resume": "new", "brief": true}),
            )
            .await,
    );
    if rival["session"]["claim"]["status"] != "refused"
        || rival["session"]["claim"]["holder"] != role_holder
    {
        fail(
            "a rival refused a lease with no heartbeat row",
            &rival.to_string(),
        );
    }
    let beat = surface
        .call(
            "journal",
            json!({"entry": "renewed after the upgrade", "sid": role_key}),
        )
        .await;
    if beat.contains("\"status\":\"blocked\"") {
        fail("the holder's renewal", &beat);
    }
    surface.finish().await;
    second.stop().await;

    let mut store = open_the_store(&state_dir, store_port).await;
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT moment FROM role_lease_heartbeat WHERE role = ?")
            .bind(&role)
            .fetch_all(store.pool())
            .await
            .expect("the heartbeat rows after the renewal");
    let moment: String = sqlx::query_scalar(
        "SELECT value FROM field_write WHERE `key` = 'claimed_at' AND entity = ?
         ORDER BY ordinal DESC LIMIT 1",
    )
    .bind(&role)
    .fetch_one(store.pool())
    .await
    .expect("the lease moment after the renewal");
    store.stop().await;
    assert_eq!(
        rows,
        vec![(moment.clone(),)],
        "recorded at {git_ref}: the renewal left one heartbeat row holding the lease moment"
    );
    assert_ne!(
        moment, aged,
        "recorded at {git_ref}: the renewal moved the moment"
    );
    drop(scratch);
}

/// ── (j) A write of the boot's own passes before the teaching ledger opens ───
/// **A boot whose due-days pass has work writes through the writer before the
/// teaching ledger opens, and both come out whole.** The pass stamps a thing that
/// cannot say when it falls due; the ledger opens after it and still serves the
/// marks the restore seeded. The boot's lines show the stamp done before the
/// serving line, and the log, the migrations and the marks all end whole.
#[tokio::test]
async fn a_boot_that_stamps_a_due_day_before_the_teaching_ledger_opens_leaves_both_whole() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_the_fixture("due-days-before-teaching").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    leave_the_claims_import_to_the_head_boot(store.pool()).await;
    jojobot_adapters::dolt::migrate::run(store.pool())
        .await
        .expect("the planting schema");
    entity(store.pool(), "thing:gate-fern", "gfern01", None).await;
    record(
        store.pool(),
        "gfern01",
        "watered on a schedule nobody wrote down",
    )
    .await;
    for (key, value) in [
        ("cadence_days", "often"),
        ("advances_from", "due_date"),
        ("counts_from", "2026-06-01"),
    ] {
        field(store.pool(), "gfern01", key, value, "legacy").await;
    }
    sqlx::query("DELETE FROM store_pass WHERE name = 'unreadable due day'")
        .execute(store.pool())
        .await
        .expect("the unreadable loop has not been stamped");
    store.stop().await;

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let stamped = booted
        .log
        .iter()
        .position(|line| line.contains("due days: stamped what could not be read"));
    let serving = booted
        .log
        .iter()
        .position(|line| line.contains("serving http://"));
    assert!(
        stamped.is_some() && stamped < serving,
        "recorded at {git_ref}: the due-days pass did not write before the serving line: {:?}",
        booted.log
    );
    // **Before the teaching ledger opens**: the boot logs the open on the line
    // below, so a pass moved to any point behind it stamps after that line.
    let opened = booted
        .log
        .iter()
        .position(|line| line.contains("teaching ledger: open over the event log"));
    assert!(
        opened.is_some() && stamped < opened,
        "recorded at {git_ref}: the due-days pass did not write before the teaching ledger \
         opened: {:?}",
        booted.log
    );
    booted.stop().await;
    assert_the_ledger_ends_whole(&state_dir, &git_ref).await;
    assert_the_teaching_marks_equal_the_rows(&state_dir, store_port, &git_ref).await;
    drop(scratch);
}

/// ── (f) A run closed twice ──────────────────────────────────────────────────
/// **A run offers its wrap code once.** The first wrap hands back a code, the
/// code reopens the run for one last change, and the second wrap, which carries
/// the same story, hands back none and ends the window: the spent code reopens
/// nothing. The head's check of a retry reads the run's own record of closes, and
/// this holds the surface half of it on a store the deployed build filled, beside
/// that build's runs. The retry after a close whose key could not be ended needs
/// a store that fails on cue, and the unit cases of `wrap_session` hold it.
#[tokio::test]
async fn a_run_closed_twice_offers_its_wrap_code_once_on_the_head() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_the_fixture("closed-twice").await;
    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let (surface, sid) = as_the_assistant(&booted, &git_ref).await;
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not hold: {body}")
    };
    let started = surface
        .call(
            "journal",
            json!({"entry": "a run the gate closes twice", "sid": sid}),
        )
        .await;
    if started.contains("\"status\":\"blocked\"") {
        fail("the first beat", &started);
    }
    let story = "the gate's run is done";
    let first = parsed(
        "the first wrap",
        &git_ref,
        &surface
            .call("wrap_session", json!({"story": story, "sid": sid}))
            .await,
    );
    let Some(code) = first["wrap_code"].as_str() else {
        fail("the first wrap handing back a code", &first.to_string());
    };
    let reopened = parsed(
        "reopening the run with its code",
        &git_ref,
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "resume": code, "brief": true}),
            )
            .await,
    );
    let Some(reopened_sid) = reopened["session"]["sid"].as_str() else {
        fail("the code reopening the run", &reopened.to_string());
    };
    // An open window still holds the spent code. The second wrap clears it, so
    // only a retry here can expose a guard that accepts the open window again.
    let spent_open = parsed(
        "the spent code while the window is open",
        &git_ref,
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "resume": code, "brief": true}),
            )
            .await,
    );
    if spent_open["status"] != "blocked" || !spent_open["session"]["sid"].is_null() {
        fail(
            "the spent code refused while its window is open",
            &spent_open.to_string(),
        );
    }
    let second = parsed(
        "the second wrap",
        &git_ref,
        &surface
            .call("wrap_session", json!({"story": story, "sid": reopened_sid}))
            .await,
    );
    if second["status"] == "blocked" || !second["wrap_code"].is_null() {
        fail(
            "the second wrap landing without a new code",
            &second.to_string(),
        );
    }
    // **No key comes back.** Each use of a code issues a key of its own, so the
    // key of the first reopening cannot be the thing compared: a spent code that
    // reopened the run would hand back another.
    let spent = surface
        .call(
            "start_here",
            json!({"bot": "assistant", "resume": code, "brief": true}),
        )
        .await;
    let spent_answer = parsed("the spent code", &git_ref, &spent);
    if spent_answer["status"] != "blocked" || !spent_answer["session"]["sid"].is_null() {
        fail("the spent code reopening nothing", &spent);
    }
    surface.finish().await;
    booted.stop().await;
    drop(scratch);
}
