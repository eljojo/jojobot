//! **A server acting out a day continues after the stamps its store already
//! holds.**
//!
//! A stated clock counts from its day's midnight in every process. A process
//! started over a store that an earlier process filled would stamp its first
//! writes before that process's last ones, and a reader that orders by stamp
//! would put the new mail ahead of the old. These cases start the real binary on
//! the stated day, over a store seeded by another process, and read the stamp of
//! a write it makes.

use std::process::Stdio;

use jiff::Timestamp;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::mailboxes::DoltMailboxes;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::sessions::DoltSessions;
use jojobot_adapters::testing::{free_port, start_unhurried};
use jojobot_domain::mailbox::{
    MailboxError, MailboxName, Mailboxes, NewMessage, OwnerIndex, OwnerLookup,
};
use jojobot_domain::memory::EntityId;
use jojobot_domain::session::{NewSession, Sessions, Sid};

const DAY: &str = "2026-06-01";

/// An owner index that answers for anybody: the case needs a box a person owns,
/// not the roster that decides who may.
struct AnyOwner;

#[async_trait::async_trait]
impl OwnerIndex for AnyOwner {
    async fn look_up(&self, _: &EntityId) -> Result<OwnerLookup, MailboxError> {
        Ok(OwnerLookup::Known)
    }
}

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// **The stamp of a journal beat written by the real binary acting out
/// [`DAY`]**, over a store whose only row, when `seeded_at` is given, is a run
/// begun at that moment by an earlier process, after `earlier_boots` boots of
/// the binary over the same store.
async fn stamp_of_a_beat_after(seeded_at: Option<Timestamp>, earlier_boots: usize) -> Timestamp {
    stamp_of_a_beat_over(seeded_at, earlier_boots, false).await
}

/// [`stamp_of_a_beat_after`], with the earlier process also having left a push
/// queued for the operator when `queued_push` is set. The outbox stamps that row
/// on the wall clock, whatever day the run is acting out.
async fn stamp_of_a_beat_over(
    seeded_at: Option<Timestamp>,
    earlier_boots: usize,
    queued_push: bool,
) -> Timestamp {
    // One directory per call: two seeded cases run side by side in this process.
    let state_dir = jojobot_adapters::testing::scratch_path("jojobot-continued-clock");
    let _ = std::fs::remove_dir_all(&state_dir);
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let _scratch = Scratch(state_dir.clone());
    let mut store_port = free_port();

    if queued_push {
        let mut seeding = seeding_store(&state_dir, &mut store_port).await;
        migrate::run(seeding.pool())
            .await
            .expect("the schema applies");
        migrate::seed_kinds(seeding.pool())
            .await
            .expect("the kinds are seeded");
        let mail =
            DoltMailboxes::open(seeding.pool().clone(), std::sync::Arc::new(AnyOwner)).pushing();
        let person = EntityId("person:ned-flanders".into());
        mail.create_mailbox(&MailboxName::named_for(&person), &person, None)
            .await
            .expect("the operator's box opens")
            .written()
            .expect("not blocked");
        // Stamped at the day's first second, so the beat the binary writes is
        // still within minutes of midnight when nothing else moves the clock.
        mail.post_message(NewMessage {
            mailbox: MailboxName::named_for(&person),
            body: "a word for the person".into(),
            subject: None,
            sender: "gamma".into(),
            sent_at: format!("{DAY}T00:00:01Z").parse().expect("a moment"),
            in_reply_to: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
            supersedes: None,
        })
        .await
        .expect("the post lands")
        .written()
        .expect("not blocked");
        seeding.stop().await;
    }
    if let Some(started_at) = seeded_at {
        // The exact path `wiring::resolve_store_dir` derives from `STATE_DIRECTORY`.
        let mut seeding = seeding_store(&state_dir, &mut store_port).await;
        migrate::run(seeding.pool())
            .await
            .expect("the schema applies");
        DoltSessions::open(seeding.pool().clone())
            .begin(NewSession {
                bot: EntityId("bot:marge".into()),
                sid: Sid("hmr1".into()),
                focus: "working".into(),
                started_at,
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("the earlier process's run lands");
        seeding.stop().await;
    }

    let binary = jojobot_exercise::room::server_binary().expect("a jojobot binary to run");
    for _ in 0..earlier_boots {
        beat_written_by_the_binary(&binary, &state_dir, &mut store_port).await;
    }
    beat_written_by_the_binary(&binary, &state_dir, &mut store_port).await
}

/// **How many times a store or a boot is tried before its failure is reported.**
/// The port a case claims is let go before the store or the binary binds it, and
/// a process outside the claim files can take it in that time, so one failure
/// says nothing about the build. The same count a room tries (see
/// `jojobot_exercise::room`), with the same short, uneven pause between tries.
const ATTEMPTS: usize = 8;

/// **Start the store that seeds a case**, on `port`, and on a fresh port when
/// that one fails. `port` holds the port that answered, which the binary then
/// starts on over the same data. The readiness wait is the one a store is given
/// to answer in (thirty seconds), not the probe's fifth of a second, so a loaded
/// host is not a failed case. A store that never comes up in [`ATTEMPTS`] tries
/// is reported with each try's words.
async fn seeding_store(state_dir: &std::path::Path, port: &mut u16) -> Dolt {
    seeding_store_claiming(state_dir, port, free_port).await
}

/// [`seeding_store`] with the way a fresh port is claimed given, so a case can
/// hand out ports that something else holds.
async fn seeding_store_claiming(
    state_dir: &std::path::Path,
    port: &mut u16,
    mut claim: impl FnMut() -> u16,
) -> Dolt {
    let mut said = Vec::new();
    for attempt in 0..ATTEMPTS {
        if attempt > 0 {
            jojobot_exercise::room::pause_before_retry().await;
            *port = claim();
        }
        match start_unhurried(&state_dir.join("db"), *port).await {
            Ok(store) => return store,
            Err(e) => said.push(format!("try {}: {e}", attempt + 1)),
        }
    }
    panic!(
        "the seeding store did not come up in {ATTEMPTS} tries: {}",
        said.join("; ")
    );
}

/// **One run of the real binary acting out [`DAY`]** over the store in
/// `state_dir`: it boots, writes a journal beat, stops, and the beat's stamp is
/// returned.
async fn beat_written_by_the_binary(
    binary: &std::path::Path,
    state_dir: &std::path::Path,
    store_port: &mut u16,
) -> Timestamp {
    let mut said = Vec::new();
    for attempt in 0..ATTEMPTS {
        if attempt > 0 {
            jojobot_exercise::room::pause_before_retry().await;
            *store_port = free_port();
        }
        match one_boot_of_the_binary(binary, state_dir, *store_port).await {
            Ok(stamp) => return stamp,
            Err(words) => said.push(format!("try {}: {words}", attempt + 1)),
        }
    }
    panic!(
        "the binary never served in {ATTEMPTS} tries: {}",
        said.join("; ")
    );
}

/// **One boot of the binary and the beat it writes**, or what the boot said when
/// it never reached its serving line: how long it waited and the last lines of the
/// server's stderr, so a store that will not start is read from its own words.
async fn one_boot_of_the_binary(
    binary: &std::path::Path,
    state_dir: &std::path::Path,
    store_port: u16,
) -> Result<Timestamp, String> {
    let http_port = free_port();
    let mut child = tokio::process::Command::new(binary)
        .env("STATE_DIRECTORY", state_dir)
        .env("JOJOBOT_STORE_PORT", store_port.to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{http_port}"))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env("JOJOBOT_TODAY", DAY)
        .env_remove("JOJOBOT_ISSUER")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("the binary runs");
    // The shared wait: on a boot that never serves, it says how the process ended
    // and quotes the last lines of its stderr.
    let serving = jojobot_adapters::testing::wait_for_serving_line(
        &mut child,
        jojobot_exercise::room::READY_TIMEOUT,
        "serving http://",
    )
    .await
    .map_err(|failure| {
        format!(
            "the binary {failure} (a boot is given {:?})",
            jojobot_exercise::room::READY_TIMEOUT
        )
    })?;
    // Held to the end of the boot, so the pipe stays open while the case reads.
    let _stdout = serving.stdout;

    let surface =
        jojobot_exercise::surface::Surface::connect(&format!("http://127.0.0.1:{http_port}/mcp"))
            .await
            .expect("connecting to the real binary");
    let mut booted: serde_json::Value = serde_json::from_str(
        &surface
            .call("start_here", serde_json::json!({"bot": "assistant"}))
            .await,
    )
    .expect("the boot answers in JSON");
    if booted
        .pointer("/session/sid")
        .and_then(|sid| sid.as_str())
        .is_none()
    {
        // An earlier boot of the binary left a run to pick up; this one starts its own.
        booted = serde_json::from_str(
            &surface
                .call(
                    "start_here",
                    serde_json::json!({"bot": "assistant", "resume": "new"}),
                )
                .await,
        )
        .expect("the boot answers in JSON");
    }
    let sid = booted
        .pointer("/session/sid")
        .and_then(|sid| sid.as_str())
        .expect("a boot hands back a sid once it has answered the resume choice")
        .to_string();
    let beat: serde_json::Value = serde_json::from_str(
        &surface
            .call(
                "journal",
                serde_json::json!({"sid": sid, "entry": "found the door"}),
            )
            .await,
    )
    .expect("the journal answers in JSON");
    surface.finish().await;
    let _ = child.start_kill();
    let _ = child.wait().await;

    Ok(beat
        .pointer("/entry/at")
        .and_then(|at| at.as_str())
        .unwrap_or_else(|| panic!("the receipt carries the beat's stamp: {beat}"))
        .parse()
        .expect("the stamp is an instant"))
}

/// **The positive.** The earlier process left a run begun at eleven at night on
/// the stated day. A beat the new process writes is stamped after it, though the
/// new process has run for a minute and not for twenty-three hours.
#[tokio::test]
async fn a_beat_written_over_a_store_another_process_filled_is_stamped_after_what_it_holds() {
    let seeded: Timestamp = format!("{DAY}T23:00:00.5Z").parse().expect("a moment");

    let stamp = stamp_of_a_beat_after(Some(seeded), 0).await;

    assert!(
        stamp > seeded,
        "the beat is stamped {stamp}, which is not after the run the store already held, {seeded}"
    );
}

/// **A stamp after the stated day counts as well.** The earlier process left a
/// run begun at one in the morning on the day after [`DAY`]. A beat the new
/// process writes is stamped after it, though the stated day is over a day behind
/// it.
#[tokio::test]
async fn a_beat_written_over_a_store_holding_a_stamp_on_the_next_day_is_stamped_after_it() {
    let seeded: Timestamp = "2026-06-02T01:00:00.5Z".parse().expect("a moment");

    let stamp = stamp_of_a_beat_after(Some(seeded), 0).await;

    assert!(
        stamp > seeded,
        "the beat is stamped {stamp}, which is not after the run the store already held, {seeded}"
    );
}

/// **The control.** With nothing stamped in the store, the clock starts at its
/// day's midnight as it always did. A clock that began late for every store
/// would pass the case above for the wrong reason.
#[tokio::test]
async fn a_beat_written_over_a_store_with_no_stamp_on_the_day_is_stamped_at_midnight() {
    let midnight: Timestamp = format!("{DAY}T00:00:00Z").parse().expect("a moment");

    let stamp = stamp_of_a_beat_after(None, 0).await;

    assert!(
        stamp >= midnight && stamp.duration_since(midnight).as_secs() < 600,
        "the beat is stamped {stamp}, which is not within ten minutes of {midnight}"
    );
}

/// **A boot that finishes a store pass leaves the stated day where it was.** A
/// pass records its end on the real clock, so a later boot that read that moment
/// as a stamp of the run being acted out would continue from today. The binary
/// boots twice over one store: the first boot finishes the passes a new store
/// owes, and a beat written by the second is still stamped on [`DAY`].
#[tokio::test]
async fn a_stated_day_survives_a_boot_that_finished_a_store_pass() {
    let midnight: Timestamp = format!("{DAY}T00:00:00Z").parse().expect("a moment");

    let stamp = stamp_of_a_beat_after(None, 1).await;

    assert!(
        stamp >= midnight && stamp.duration_since(midnight).as_secs() < 3600,
        "the beat is stamped {stamp}, which is not within an hour of {midnight}"
    );
    assert_ne!(
        stamp.to_zoned(jiff::tz::TimeZone::UTC).date(),
        Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC).date(),
        "the beat is stamped on the real day, so the stated clock moved to today"
    );
}

/// **A push queued for the operator leaves the stated day where it was.** The
/// earlier process queued a push, which the outbox stamps on the real clock. A
/// beat the binary writes over that store is still stamped within minutes of the
/// stated day's start, on [`DAY`] and not on the real day. The same store without
/// the push is the control in the case above it.
#[tokio::test]
async fn a_stated_day_survives_a_push_queued_by_an_earlier_process() {
    let midnight: Timestamp = format!("{DAY}T00:00:00Z").parse().expect("a moment");

    let stamp = stamp_of_a_beat_over(None, 0, true).await;

    assert!(
        stamp >= midnight && stamp.duration_since(midnight).as_secs() < 600,
        "the beat is stamped {stamp}, which is not within ten minutes of {midnight}"
    );
}

/// **A shim in place of the binary: it fails the way a boot fails when its store
/// port is taken, for its first `refusals` starts, then becomes the real binary.**
/// The words are the ones a kept CI log shows. The counter file says how many
/// starts it refused.
fn binary_that_is_refused(
    refusals: usize,
    then: &std::path::Path,
    dir: &std::path::Path,
) -> std::path::PathBuf {
    binary_that_is_refused_noisily(refusals, 0, then, dir)
}

/// [`binary_that_is_refused`], printing `noise` numbered lines on stderr before
/// the words of the refusal, so a case can read how much of stderr is quoted.
fn binary_that_is_refused_noisily(
    refusals: usize,
    noise: usize,
    then: &std::path::Path,
    dir: &std::path::Path,
) -> std::path::PathBuf {
    let noise: String = (1..=noise)
        .map(|n| format!("echo 'noisy-{n:02}' >&2\n"))
        .collect();
    let counter = dir.join("refused-starts");
    let shim = dir.join("refuses-then-serves");
    jojobot_exercise::spawn_gate::write_script(
        &shim,
        &format!(
            "#!/bin/sh\n\
             n=$(cat '{counter}' 2>/dev/null || echo 0)\n\
             if [ \"$n\" -lt {refusals} ]; then\n\
             echo $((n + 1)) > '{counter}'\n\
             echo \"$JOJOBOT_STORE_PORT\" >> '{ports}'\n\
             {noise}\
             echo 'STORE UNAVAILABLE: the store exited before it answered' >&2\n\
             exit 1\n\
             fi\n\
             exec '{then}'\n",
            counter = counter.display(),
            ports = dir.join("refused-ports").display(),
            then = then.display(),
            noise = noise,
        ),
    )
    .expect("the throwaway script");
    shim
}

/// 🚨 **A boot whose store port is refused four times still writes its beat.**
/// The store port is let go before the binary binds it, and a process outside the
/// claim files can take it, so the first boots of one case can fail together. The
/// shim fails four starts as the real binary fails, then serves; the case needs a
/// fresh port and a pause between tries to get through, and the beat's stamp is
/// the day's own.
#[tokio::test]
async fn a_boot_whose_store_port_is_refused_four_times_still_writes_its_beat() {
    const REFUSALS: usize = 4;
    let dir = jojobot_adapters::testing::scratch_path("jojobot-refused-boot");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let real = jojobot_exercise::room::server_binary().expect("a jojobot binary");
    let shim = binary_that_is_refused(REFUSALS, &real, &dir);
    let state_dir = dir.join("state");
    std::fs::create_dir_all(&state_dir).expect("a state directory");
    let mut port = free_port();

    let stamp = beat_written_by_the_binary(&shim, &state_dir, &mut port).await;

    let refused = std::fs::read_to_string(dir.join("refused-starts")).unwrap_or_default();
    assert_eq!(
        refused.trim(),
        REFUSALS.to_string(),
        "the shim refused another count"
    );
    // The shim ignores the port, so the case must show the port changed itself:
    // every refused start was given a port no earlier start had.
    let ports = std::fs::read_to_string(dir.join("refused-ports")).unwrap_or_default();
    let ports: Vec<&str> = ports.lines().collect();
    let distinct: std::collections::HashSet<&str> = ports.iter().copied().collect();
    assert_eq!(
        ports.len(),
        REFUSALS,
        "a port was recorded per refused start: {ports:?}"
    );
    assert_eq!(
        distinct.len(),
        REFUSALS,
        "a refused start was tried again on the port that was refused: {ports:?}"
    );
    let midnight: Timestamp = format!("{DAY}T00:00:00Z").parse().expect("a moment");
    assert!(
        stamp >= midnight,
        "the beat is stamped {stamp}, before the stated day began"
    );
}

/// 🚨 **A boot that never serves reports the server's own words.** The stand-in
/// always fails and says why on stderr; after every try the case panics, and the
/// panic quotes what the server said, not a bare count.
#[tokio::test]
async fn a_boot_that_never_serves_reports_what_the_server_said() {
    let dir = jojobot_adapters::testing::scratch_path("jojobot-dead-boot");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let real = jojobot_exercise::room::server_binary().expect("a jojobot binary");
    let shim = binary_that_is_refused(1000, &real, &dir);
    let state_dir = dir.join("state");
    std::fs::create_dir_all(&state_dir).expect("a state directory");

    let tried = tokio::spawn(async move {
        let mut port = free_port();
        beat_written_by_the_binary(&shim, &state_dir, &mut port).await
    })
    .await
    .expect_err("a binary that never serves fails the case");

    let said = tried
        .into_panic()
        .downcast::<String>()
        .map(|said| *said)
        .unwrap_or_default();
    assert!(
        said.contains("STORE UNAVAILABLE: the store exited before it answered"),
        "the panic does not quote the server's stderr: {said}"
    );
}

/// 🚨 **A seeding store whose port something else holds is started on a fresh
/// port.** Three ports are held by listeners that never answer; the store fails on
/// each and comes up on the fourth, a port the case claimed afresh.
#[tokio::test]
async fn a_seeding_store_whose_port_is_held_comes_up_on_a_fresh_port() {
    let dir = jojobot_adapters::testing::scratch_path("jojobot-held-seed");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let held: Vec<std::net::TcpListener> = (0..3)
        .map(|_| std::net::TcpListener::bind(("127.0.0.1", 0)).expect("a port to hold"))
        .collect();
    let mut ports: Vec<u16> = held
        .iter()
        .map(|listener| listener.local_addr().expect("an address").port())
        .collect();
    let mut port = ports.remove(0);
    let first = port;
    let mut claims = 0usize;

    let mut store = seeding_store_claiming(&dir, &mut port, || {
        claims += 1;
        ports.pop().unwrap_or_else(free_port)
    })
    .await;

    assert_ne!(port, first, "the store came up on the port that was held");
    assert_eq!(
        claims, 3,
        "two held ports were handed out, then a fresh one"
    );
    store.stop().await;
    drop(held);
}

/// 🚨 **A seeding store that never comes up is reported after every try.** Every
/// port the claim function hands out is held by a listener that never answers, so
/// each of the tries fails; the panic carries each try's words, the first and the
/// last.
#[tokio::test]
async fn a_seeding_store_that_never_comes_up_is_reported_after_every_try() {
    let dir = jojobot_adapters::testing::scratch_path("jojobot-never-seed");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let held = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("a port to hold");
    let taken = held.local_addr().expect("an address").port();

    let tried = tokio::spawn(async move {
        let mut port = taken;
        seeding_store_claiming(&dir, &mut port, || taken).await;
    })
    .await
    .expect_err("a store that cannot take any port fails the case");

    let said = tried
        .into_panic()
        .downcast::<String>()
        .map(|said| *said)
        .unwrap_or_default();
    assert!(
        said.contains("try 1:") && said.contains(&format!("try {ATTEMPTS}:")),
        "the panic does not carry the first and the last try: {said}"
    );
    drop(held);
}

/// 🚨 **Only the last twenty lines of a server's stderr are quoted.** The shim
/// prints thirty numbered lines and then its refusal, in every try; the panic has
/// the end of them and not the start.
#[tokio::test]
async fn only_the_last_twenty_lines_of_the_servers_stderr_are_quoted() {
    let dir = jojobot_adapters::testing::scratch_path("jojobot-noisy-boot");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let real = jojobot_exercise::room::server_binary().expect("a jojobot binary");
    let shim = binary_that_is_refused_noisily(1000, 30, &real, &dir);
    let state_dir = dir.join("state");
    std::fs::create_dir_all(&state_dir).expect("a state directory");

    let tried = tokio::spawn(async move {
        let mut port = free_port();
        beat_written_by_the_binary(&shim, &state_dir, &mut port).await
    })
    .await
    .expect_err("a binary that never serves fails the case");

    let said = tried
        .into_panic()
        .downcast::<String>()
        .map(|said| *said)
        .unwrap_or_default();
    // Thirty noise lines and the refusal are thirty-one; twenty are kept, the
    // refusal and the last nineteen of the noise.
    assert!(
        said.contains("noisy-12")
            && said.contains("noisy-30")
            && said.contains("STORE UNAVAILABLE"),
        "the end of stderr is quoted: {said}"
    );
    assert!(
        !said.contains("noisy-01") && !said.contains("noisy-11"),
        "the start of stderr is not: {said}"
    );
}
