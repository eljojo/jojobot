//! **The year, held for free.**
//!
//! Seventeen cold sittings over a fictional year. The locks that matter are late
//! and the work that earns them is early, so the case that says the year
//! measures anything is the one that works only its second half: a store that
//! nobody wrote in until June cannot answer the questions September and October
//! ask.
//!
//! Both kinds of case, named apart: the ones that prove the locks discriminate,
//! and the one that proves the year is solvable at all. A year whose own
//! arithmetic is wrong is unreachable by every session that will ever enter it,
//! every discriminating case still passes, and the failure surfaces on a paid
//! run reading as a defect in the product.
//!
//! The file reads in this order: the harness, the player (what a sitting does,
//! right and wrong), the year (which sittings run and which plays replace
//! them), the locks by name, then the cases.

use jojobot_exercise::expectations;
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{
    Boundary, Observed, Outcome, boundary, boundary_names, days_claimed, uncovered_phases,
};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::LazyLock;

// ───────────────────────────────── the harness ─────────────────────────────────

/// **The day this room's server runs on**, read from the room's own document
/// (the one the paid path reads) and carried to the child as its own clock. Every moment the server stamps is this day, whatever the real date is,
/// so no needle can be met by a stamp on one real day. It is a day no needle
/// of this room holds, and `no_needle_here_is_the_servers_own_day` asks that.
static SERVER_DAY: LazyLock<String> = LazyLock::new(|| {
    room_document()
        .stated_server_day()
        .unwrap_or_else(|e| panic!("{e:#}"))
        .to_string()
});

/// The document a run is driven by.
fn room_document() -> Playbook {
    let path = expectations::room_document(expectations::YEAR_ROOM);
    Playbook::read(&path).unwrap_or_else(|e| panic!("the shipped room must read: {e:#}"))
}

/// **How a case's room is furnished.** `Live` furnishes through the served
/// surface, exactly as a run does. `Cached` is a private copy of the directory a
/// live furnish left behind, built once per process.
///
/// ⛔️ **Neither boots anything.** A sitting is a run of its own and brings its own
/// session, in its own day. A session minted here would be a run in the day the
/// server is actually having — months after the year's first sitting — and the
/// sweep answering in January's frame would see a beat from its own future and
/// keep offering it.
#[derive(Clone, Copy)]
enum Furnish {
    Cached,
    Live,
}

/// A room furnished the way a run furnishes it, and a handle to write into it
/// as an occupant would. `Cached` reads the same as `Live` on what a sitting can
/// find, which `the_cached_year_reads_the_same_as_a_live_furnish` proves.
async fn furnished(how: Furnish) -> (Room, Surface) {
    let binary = server_binary().expect("a jojobot binary");
    match how {
        Furnish::Live => {
            let (room, surface) = Room::open_with_client_stating(&binary, SERVER_DAY.as_str())
                .await
                .expect("a room");
            expectations::seed_for(expectations::YEAR_ROOM)
                .expect("the room has furniture")
                .furnish(&surface)
                .await
                .expect("the room is furnished");
            (room, surface)
        }
        // `Room::open_with_client_from_stating` copies the seed before spawning, so
        // a caller here mutates its own directory only and never another test's.
        Furnish::Cached => Room::open_with_client_from_stating(
            &binary,
            &filed_year_seed().await,
            SERVER_DAY.as_str(),
        )
        .await
        .expect("a room from the seed"),
    }
}

/// **The directory a live furnish of the year leaves behind, built once per
/// process and copied for every caller after that** — the same mechanism
/// `decisions_room.rs` already has (`filed_register_seed`), reused rather
/// than duplicated. `OnceCell::get_or_try_init` is what makes the first
/// builder finish before any concurrent caller copies from it: a caller
/// that arrives while the first build is still running awaits the same
/// build rather than starting a second one.
static YEAR_SEED: tokio::sync::OnceCell<PathBuf> = tokio::sync::OnceCell::const_new();

async fn filed_year_seed() -> PathBuf {
    YEAR_SEED
        .get_or_try_init(|| async {
            let binary = server_binary().expect("a jojobot binary");
            Room::snapshot_after_stating(&binary, SERVER_DAY.as_str(), |surface| async move {
                expectations::seed_for(expectations::YEAR_ROOM)?
                    .furnish(&surface)
                    .await?;
                Ok(())
            })
            .await
        })
        .await
        .expect("the filed-year seed builds")
        .clone()
}

/// **One sitting: a session minted for it, in the day that sitting is in.**
///
/// 🚨 **Each sitting is a RUN OF ITS OWN.** The year used to thread one session
/// through fifteen phases and name the day on every write; a sitting is a run
/// on its own day, and the day belongs to the run rather than to each call.
struct Script<'a> {
    surface: &'a Surface,
    sid: String,
    /// The day the session was booted in, so a refusal can name its phase.
    day: String,
    /// **Whether the run leaves sittings out**, which is a run that meets
    /// refusals of one kind by design: a write about a thing the left-out
    /// sittings would have made.
    skipping: bool,
}

/// A session for one sitting, in the day that sitting is in.
///
/// ⭐ **No `resume` is sent, and that is an assertion rather than an omission.**
/// A boot meeting a run still in flight hands back the resume-or-new choice and
/// no handle. Each sitting here states a day weeks after the last one, so every
/// earlier run has gone quiet in the frame the sweep answers in — **and a
/// handle coming straight back is the sweep having worked.** A choice arriving
/// instead means it did not, and the panic says which runs were offered.
async fn sitting<'a>(room: &'a Surface, day: &str) -> Script<'a> {
    Script {
        surface: room,
        sid: boot_sitting(room, day).await,
        day: day.to_string(),
        skipping: false,
    }
}

async fn boot_sitting(room: &Surface, day: &str) -> String {
    let booted = room
        .must(
            "start_here",
            json!({"bot": "assistant", "brief": true, "today": day}),
        )
        .await
        .expect("the shipped identity boots");
    if let Some(sid) = booted["session"]["sid"].as_str() {
        return sid.to_string();
    }

    // 🚨 **A choice here is the SWEEP HAVING WORKED, and it is asserted rather
    // than answered blind.** The sitting before this one stopped without
    // wrapping — which is what a sitting does — so booting a day later finds it
    // gone quiet in this run's frame and offers it back.
    //
    // ⛔️ **An `active` run in that list is the sweep NOT having worked**: it
    // means the earlier run still reads as working, which is what a year acted
    // out in minutes looks like when nobody states a day.
    let offered = booted["session"]["choices"]
        .as_array()
        .unwrap_or_else(|| panic!("the boot on {day} handed back neither a handle nor a choice"));
    let working: Vec<&Value> = offered
        .iter()
        .filter(|run| run["state"] == "active")
        .collect();
    assert!(
        working.is_empty(),
        "the boot on {day} was offered a run still ACTIVE, so an earlier sitting did not go \
         quiet in this run's frame and the sweep answered on the clock instead: {working:?}",
    );

    // **A new run, deliberately.** Each sitting is its own; resuming the one
    // before it would make the year one long session again by another route.
    let answered = room
        .must(
            "start_here",
            json!({"bot": "assistant", "brief": true, "today": day, "resume": "new"}),
        )
        .await
        .expect("the boot answering the choice is ok");
    answered["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("answering `new` on {day} handed back no handle: {answered}"))
        .to_string()
}

/// Whether an answer is a refusal, or a transport failure, which ends a script
/// the same way.
fn is_refused(answer: &str) -> bool {
    serde_json::from_str::<Value>(answer)
        .is_ok_and(|body| body["status"] == "blocked" || !body["transport_error"].is_null())
}

/// **Whether a refusal is for a thing the room does not hold.** Read off the
/// room rather than off the refusal's words: the refusal names what it attempted,
/// and a read of that name is itself blocked when nothing answers to it.
async fn names_an_absent_entity(room: &Surface, answer: &str) -> bool {
    let Some(attempted) = serde_json::from_str::<Value>(answer)
        .ok()
        .and_then(|body| body["attempted"].as_str().map(str::to_string))
    else {
        return false;
    };
    if attempted.contains('#') {
        return false;
    }
    is_refused(&room.call("recall", json!({"subject": attempted})).await)
}

impl Script<'_> {
    /// **A call an occupant would make, and a call that must land.** The worked
    /// year and its variants are scripts meant to succeed, so a refused call ends
    /// the run at the call, naming the phase and the refusal; swallowed, it shows
    /// up phases later as locks that fail far from the cause.
    ///
    /// **One refusal is expected, and only where the run says so.** A run that
    /// leaves sittings out meets writes about things those sittings would have
    /// made, and the room refuses them as unknown. Such a run declares it by its
    /// [`Year`]; every other refusal still ends it, and so does that one in any run
    /// that left nothing out.
    async fn did(&self, verb: &str, mut args: Value) -> String {
        args["sid"] = json!(self.sid);
        let sent = args.clone();
        let answer = self.surface.call(verb, args).await;
        if !is_refused(&answer) {
            return answer;
        }
        if self.skipping && names_an_absent_entity(self.surface, &answer).await {
            return answer;
        }
        let phase = room_document()
            .phases
            .iter()
            .find(|phase| phase.day.as_deref() == Some(self.day.as_str()))
            .map(|phase| phase.name.clone())
            .unwrap_or_else(|| "a sitting with no recorded day".to_string());
        panic!(
            "{phase}: {verb} was refused, so the script did not land.\ncall: {sent}\nanswer: {answer}"
        );
    }
}

/// What a script's panic said, read off the task it ran in.
async fn what_the_script_said(task: tokio::task::JoinHandle<()>, ending: &str) -> String {
    let panic = task.await.expect_err(ending).into_panic();
    panic
        .downcast_ref::<String>()
        .cloned()
        .expect("the run ends with a message")
}

/// **A call that must end the run, run on a task of its own, and what it said.**
/// The sitting is booted on the task, so the script it carries is the one that
/// refuses.
async fn refused_run(
    surface: &std::sync::Arc<Surface>,
    day: &str,
    skipping: bool,
    verb: &'static str,
    args: Value,
) -> String {
    let task = tokio::spawn({
        let (surface, day) = (surface.clone(), day.to_string());
        async move {
            let mut script = sitting(&surface, &day).await;
            script.skipping = skipping;
            script.did(verb, args).await;
        }
    });
    what_the_script_said(task, "the refused call must end the run").await
}

/// **Correct a testimony claim an EARLIER sitting wrote, the way the product
/// asks for it.** A session rewrites testimony in place only if it wrote the
/// claim, so a later sitting archives the claim, with the reason in `details`,
/// and captures the corrected claim with `derived_from` naming it. The
/// original stays readable at its address. One helper, so every play that
/// corrects an earlier sitting's words does it the same way.
async fn corrects_an_earlier_claim(
    s: &Script<'_>,
    address: &str,
    reason: &str,
    mut correction: Value,
) {
    s.did(
        "update_fact",
        json!({"address": address, "status": "archived", "details": reason}),
    )
    .await;
    correction["derived_from"] = json!(address);
    s.did("capture", correction).await;
}

/// Every lock the year registers, run against the room as it stands and
/// against the readings the run took as it went.
///
/// ⚠️ **The boundaries are an argument rather than an empty list.** A check
/// scoped to one sitting's window has nothing to read without them, and a case
/// that judged a worked year against no readings would report that check
/// failing for a reason that has nothing to do with the year.
async fn judge_all(room: &Surface, boundaries: &[Boundary]) -> Vec<Outcome> {
    let seen = Observed { room, boundaries };
    let checks = expectations::for_playbook(expectations::YEAR_ROOM).expect("the year asserts");
    let mut outcomes = Vec::new();
    for check in checks {
        outcomes.push(check.check(&seen).await);
    }
    assert!(
        !outcomes.is_empty(),
        "the year registered no locks, so a run would report a pass over an empty list",
    );
    outcomes
}

/// What the run would say, so a failure names the lock that failed.
fn saying(outcomes: &[Outcome]) -> String {
    outcomes
        .iter()
        .enumerate()
        .map(|(at, o)| format!("\n  {at:>2} [{}] {}", o.held, o.saying))
        .collect()
}

/// The facts `recall` reports for `subject`, as served, or none when the read is
/// not a recall of it.
async fn facts_of(room: &Surface, subject: &str) -> Option<Vec<Value>> {
    let read = room
        .call("recall", json!({"subject": subject, "facts": true}))
        .await;
    serde_json::from_str::<Value>(&read)
        .ok()?
        .get("objects")?
        .get(0)?
        .get("facts")?
        .as_array()
        .cloned()
}

/// The address of the record on `subject` whose content carries `needle`.
async fn address_of(room: &Surface, subject: &str, needle: &str) -> String {
    try_address_of(room, subject, needle)
        .await
        .unwrap_or_else(|| panic!("no record on {subject} says {needle:?}"))
}

/// **[`address_of`], without the panic.** For a sitting whose own business
/// is optional on a partial year — some cases drive one on purpose, for
/// reasons that have nothing to do with what this call is finding.
async fn try_address_of(room: &Surface, subject: &str, needle: &str) -> Option<String> {
    facts_of(room, subject)
        .await?
        .iter()
        .find(|fact| {
            fact["content"]
                .as_str()
                .is_some_and(|said| said.contains(needle))
        })
        .and_then(|fact| fact["address"].as_str())
        .map(str::to_string)
}

/// The address a write's answer carries.
fn captured_address(answer: &str) -> String {
    serde_json::from_str::<Value>(answer)
        .ok()
        .and_then(|v| v["address"].as_str().map(str::to_string))
        .unwrap_or_else(|| panic!("no address in the capture answer: {answer}"))
}

/// **The one entity of `kind`'s current handle.** Looked up rather than
/// hardcoded: October may have renamed it by the time this is asked, and a
/// literal written here would go stale under exactly the rename this room
/// now weaves in.
async fn handle_of_kind(room: &Surface, kind: &str) -> String {
    let read = room.call("recall", json!({"kind": kind})).await;
    let parsed: Value = serde_json::from_str(&read).expect("the read is json");
    parsed["objects"][0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no {kind} is on the board: {read}"))
        .to_string()
}

// ──────────────────────────────── the player ────────────────────────────────
//
// What a sitting does, done the way a session that read the surface would do it
// and each carrying the day the document says it happens on; then, beside each,
// the wrong plays a lock exists to catch.

/// **What June's inspection claim says, in the shapes the year can be driven
/// with.**
///
/// They differ in one thing: what the claim's own words POINT AT. Everything
/// else the sitting does is the same under all of them, so a case that moves
/// the lock about handles is measuring the sentence rather than a sitting that
/// did less.
///
/// ⚠️ **One line each, and that is the point of them being constants.** A
/// wrapped literal whose continuation is lost stores runs of spaces
/// mid-sentence, every check on it still passes, and the fault reaches a
/// reader — five instances of it across three crates so far.
const WHAT_JUNE_SAW: &str = "@event:gorge-safety-inspection ran on @place:daredevil-gorge with @person:milhouse and @person:nelson";

/// The three nouns as words, which is what a sitting that never reached for a
/// mention leaves behind.
const IN_WORDS: &str =
    "the gorge safety inspection ran on the Daredevil Gorge with Milhouse and Nelson";

/// **Two kinds on one record and not the three the lock used to name.** The
/// event is the claim's own topic and is spelled rather than pointed at; the
/// place and the people are pointers, so the claim leads somewhere in two
/// directions.
const A_PLACE_AND_A_PERSON: &str =
    "the inspection ran on @place:daredevil-gorge with @person:milhouse and @person:nelson";

/// **Pointers, all of one kind.** Every mention leads to a person, so no claim
/// links a thing of one kind to a thing of another.
const PEOPLE_ONLY: &str = "@person:milhouse and @person:nelson were both at the inspection";

/// **The five days the longboard gains a small repair**, and what each is — read
/// off `recorded_at` by `checks.rs`'s longboard hatches, so the wording here is
/// free to change and the day is not.
const LONGBOARD_DAYS: [(&str, &str); 5] = [
    (
        "2026-02-08",
        "there is a soft spot in the longboard's deck near the nose",
    ),
    (
        "2026-03-15",
        "patched that soft spot in the longboard's deck",
    ),
    (
        "2026-05-10",
        "re-gripped the longboard since the grip tape was getting worn",
    ),
    (
        "2026-07-05",
        "replaced the longboard's rear truck, the old one had cracked",
    ),
    (
        "2026-09-13",
        "patched a new crack near the longboard's nose before it gets worse",
    ),
];

/// What a fold of the longboard's pile says, when it says nothing about a day.
const FOLD_SAYS: &str = "the longboard needed a handful of small repairs this year — the details are on the records this stands for";

/// The three words the operator uses for where a payment stands, as January
/// declares them.
const OPERATORS_WORDS: [&str; 3] = ["invoiced", "paid", "waived"];

/// The things the year stands up, as `(kind, handle, name)`.
const SKATE_TOOL: (&str, &str, &str) = ("thing", "skate-tool", "The Skate Tool");
const LONGBOARD: (&str, &str, &str) = ("thing", "longboard", "The Longboard");
const BOARD_LOCK: (&str, &str, &str) = ("thing", "board-lock", "The Board Lock");

/// Stand things up, each as the operator's own record.
async fn stand_up(s: &Script<'_>, things: &[(&str, &str, &str)]) {
    for (kind, handle, name) in things {
        s.did(
            "add_entity",
            json!({"kind": kind, "handle": handle, "name": name, "source": "the operator"}),
        )
        .await;
    }
}

/// One of the longboard's five small repairs, on the day it is recorded.
async fn repair(s: &Script<'_>, nth: usize) {
    s.did(
        "capture",
        json!({"subject": "thing:longboard", "content": LONGBOARD_DAYS[nth].1,
               "provenance": "testimony", "recorded_at": LONGBOARD_DAYS[nth].0}),
    )
    .await;
}

/// A claim filed on the council.
async fn council_says(s: &Script<'_>, content: &str) {
    s.did(
        "capture",
        json!({"subject": "org:springfield-town-council", "content": content,
               "provenance": "testimony"}),
    )
    .await;
}

/// The loop January opens: declared, then given its cadence and policy.
///
/// **The loop is declared, then opened by a back-dated check-in.** The
/// cadence and the policy are the operator's word and no check-in can state
/// them. The basis is not sent: the bearings were last done before this sitting
/// opened the loop, so the opening turn is a check-in dated that day and
/// jojobot derives `counts_from` from it. Sending the basis by hand is what
/// this room's own lock on derivations exists to catch.
async fn january_loop(s: &Script<'_>) {
    s.did(
        "add_entity",
        json!({"kind": "rhythm", "handle": "bearings-check", "name": "Bearings check",
               "source": "the operator", "parent": "thing:skateboard"}),
    )
    .await;
    s.did(
        "capture",
        json!({"subject": "rhythm:bearings-check", "content": "look at the skateboard bearings every ninety days",
               "provenance": "testimony",
               "sets": {"name": "Bearings check", "cadence_days": "90",
                          "advances_from": "check_in_date"}}),
    )
    .await;
}

async fn january(s: &Script<'_>) {
    s.did("read_mailbox", json!({})).await;
    january_loop(s).await;
    s.did(
        "capture",
        json!({"subject": "rhythm:bearings-check", "content": "last did the bearings just before christmas",
               "provenance": "testimony",
               "check_in": "ran", "recorded_at": "2025-12-20"}),
    )
    .await;
    s.did(
        "capture",
        json!({"subject": "person:milhouse", "content": "lives in Springfield",
               "provenance": "testimony",
               "shape": "location", "object": "place:springfield"}),
    )
    .await;
    s.did(
        "capture",
        json!({"subject": "person:milhouse", "content": "sits on the council",
               "provenance": "testimony",
               "shape": "membership", "object": "org:springfield-town-council"}),
    )
    .await;
    stand_up(
        s,
        &[(
            "event",
            "gorge-safety-inspection",
            "The gorge safety inspection",
        )],
    )
    .await;
    // **The brief's own hedge, carried as one.** "I mean to be there" is the
    // operator's word about something that has not happened yet — testimony,
    // and open, because only June's own outcome could settle it. Nothing
    // later in the year promotes it: this weave is that the hedge is
    // recorded as one, not that it is later confirmed.
    s.did(
        "capture",
        json!({"subject": "event:gorge-safety-inspection", "content": "planning to attend",
               "provenance": "testimony", "standing": "open"}),
    )
    .await;
    // **The brief's own vocabulary, declared once, early — the only place a
    // cold sitting months from now can learn which word the operator
    // actually uses.** Without this, "sent" and "invoiced" are just two
    // words; with it, one of them is wrong.
    s.did(
        "declare_type",
        json!({"name": "paid-job",
               "fields": [{"key": "settled", "holds": "text",
                           "one_of": ["invoiced", "paid", "waived"]}]}),
    )
    .await;
    // **The brief asked for this in the operator's own words** — leave it so
    // whoever picks it up in a month has what they need — so a sitting that
    // writes a note for the next one is doing what it was told.
    s.did(
        "post_message",
        json!({"to": "assistant", "subject": "where the year stands",
               "body": "The skateboard bearings loop is on file and the council is on the roster."}),
    )
    .await;
}

/// February, with or without the one link at the person holding the skate tool.
///
/// **A February that stands the things up and never says who has the skate tool**
/// is the fault the lock names, played: the entities exist, the prose of the
/// sitting says it was lent out, and nothing on the record points at the person
/// holding it. **September still records that it came back**, which is what used
/// to satisfy February's lock seven months early. Everything else February does
/// is done, because a year missing February's other work cannot run at all and
/// would fail on a play rather than on this lock.
async fn february(s: &Script<'_>, records_the_holder: bool) {
    stand_up(
        s,
        &[
            ("person", "ralph", "Ralph"),
            ("person", "nelson", "Nelson"),
            SKATE_TOOL,
            LONGBOARD,
        ],
    )
    .await;
    let mut lent = json!({"subject": "thing:skate-tool",
                          "content": "lent out, wanted back before the inspection",
                          "provenance": "testimony"});
    if records_the_holder {
        lent["shape"] = json!("connection");
        lent["object"] = json!("person:ralph");
    } else {
        lent["recorded_at"] = json!("2026-02-08");
    }
    s.did("capture", lent).await;
    s.did(
        "capture",
        json!({"subject": "person:nelson", "content": "joined the council",
               "provenance": "testimony",
               "shape": "membership", "object": "org:springfield-town-council"}),
    )
    .await;
    repair(s, 0).await;
}

async fn march(s: &Script<'_>) {
    s.did(
        "capture",
        json!({"subject": "org:springfield-town-council", "content": "meets on Tuesdays",
               "provenance": "testimony", "recorded_at": "2026-03-15"}),
    )
    .await;
    // **The correctly-worded job — the control this weave rests on.** It
    // already says what the operator's own word says, so the cold sitting
    // that repaints the wrong one must leave this alone.
    s.did(
        "capture",
        json!({"subject": "thing:longboard", "content": LONGBOARD_DAYS[1].1,
               "provenance": "testimony", "recorded_at": LONGBOARD_DAYS[1].0,
               "sets": {"cost": "30", "settled": "paid"}}),
    )
    .await;
}

async fn april(s: &Script<'_>) {
    let was = address_of(s.surface, "person:milhouse", "Springfield").await;
    s.did("update_fact", json!({"address": was, "status": "archived"}))
        .await;
    s.did(
        "capture",
        json!({"subject": "person:milhouse", "content": "moved to Shelbyville",
               "provenance": "testimony",
               "shape": "location", "object": "place:shelbyville"}),
    )
    .await;
}

async fn may(s: &Script<'_>) {
    s.did(
        "capture",
        json!({"subject": "place:daredevil-gorge", "content": "closed by a rockslide at the top end this spring",
               "provenance": "testimony", "recorded_at": "2026-05-10"}),
    )
    .await;
    repair(s, 2).await;
}

/// Each person's attendance at the inspection, with the claim saying `content`
/// and drawn at `object`.
async fn june_attendance(s: &Script<'_>, content: &str, object: &str) {
    for who in ["person:milhouse", "person:nelson"] {
        s.did(
            "capture",
            json!({"subject": who, "content": content,
                   "provenance": "testimony",
                   "shape": "attendance", "object": object}),
        )
        .await;
    }
}

/// The loop's turn, checked in rather than written by hand.
async fn june_checks_in(s: &Script<'_>) {
    s.did(
        "capture",
        json!({"subject": "rhythm:bearings-check", "content": "did the skateboard bearings this morning",
               "provenance": "testimony",
               "check_in": "ran"}),
    )
    .await;
}

/// **June, with the inspection claim saying `said`.**
///
/// One body for every June the suite drives, because the sittings differ in one
/// sentence and separate bodies for them are copies of the same twenty lines.
/// The attendance
/// edges are drawn and the loop is checked in whatever the claim says, so every
/// other June lock holds under all of them — which is what makes a case over
/// the lock about handles a case about the sentence rather than about a sitting
/// that did less.
///
/// The claim is filed on the council because the good version of it names the
/// event, and a claim cannot mention the thing it is already filed against.
async fn june_saying(s: &Script<'_>, said: &str) {
    council_says(s, said).await;
    june_attendance(
        s,
        "was at the gorge safety inspection",
        "event:gorge-safety-inspection",
    )
    .await;
    june_checks_in(s).await;
    // **The first figure, under a key declared to sum rather than replace.**
    // Declared here, once: the fold behaviour belongs to the key, and late
    // November's own figure only joins this one rather than overwriting it
    // because this declaration ran before either write landed.
    s.did(
        "declare_type",
        json!({"name": "cycling",
               "fields": [{"key": "km", "holds": "number", "folds": "sum"}]}),
    )
    .await;
    s.did(
        "capture",
        json!({"subject": "thing:skateboard", "content": "around 800 kilometres since the spring",
               "provenance": "testimony", "sets": {"km": "800"}}),
    )
    .await;
}

/// **June, with attendance filed as a mention on each person too** — the
/// shape a live paid run actually wrote, rather than the edge this room's
/// default June uses for the same claim. Three records point at the inspection
/// afterward: the council's own and one per person, all by the same entity.
async fn june_with_attendance_as_mentions(s: &Script<'_>) {
    council_says(s, WHAT_JUNE_SAW).await;
    june_attendance(
        s,
        "was at the gorge safety inspection, @event:gorge-safety-inspection",
        "event:gorge-safety-inspection",
    )
    .await;
    june_checks_in(s).await;
}

/// **June, plus a second event nothing in October ever touches.**
///
/// The partial-rename shape: October renames the inspection and this second
/// event is never named in its reason, so a rename that reached every one
/// of June's addresses would still leave this one exactly as written.
async fn june_also_points_at_a_second_event(s: &Script<'_>) {
    june_saying(s, WHAT_JUNE_SAW).await;
    stand_up(s, &[("event", "krusty-fest", "Krusty Fest")]).await;
    council_says(s, "already looking ahead to @event:krusty-fest").await;
}

/// **June, drawing its attendee edges at an event under a differently
/// spelled handle than the one January happened to invent.**
///
/// Nothing tells a model what to call the inspection — January's own handle is
/// that sitting's invention, not a word the operator used — so a June that
/// lands on a different spelling has done the same work as one that landed
/// on `event:gorge-safety-inspection` by chance. `event:gorge-safety-inspection` itself is left
/// standing, unused: the point is that June's own lock must not care which
/// event carries the edges, only that one does.
async fn june_points_at_a_differently_spelled_inspection(s: &Script<'_>) {
    stand_up(s, &[("event", "the-june-gorge-day", "The June gorge day")]).await;
    council_says(s, WHAT_JUNE_SAW).await;
    june_attendance(
        s,
        "was at the gorge safety inspection",
        "event:the-june-gorge-day",
    )
    .await;
    june_checks_in(s).await;
}

/// **A June that sets the loop's key instead of checking in.**
///
/// The play this room used to have, kept as the thing that must fail. Writing
/// `last_check_in` by hand puts the right day on the record and routes around
/// the verb the capability is made of: no schedule is computed, nothing is
/// stored beside the claim, and the record is not a derivation. **Every lock
/// that reads the day holds either way, which is why nothing noticed.**
async fn june_writes_the_turn_by_hand(s: &Script<'_>) {
    june_attendance(
        s,
        "was at the gorge safety inspection",
        "event:gorge-safety-inspection",
    )
    .await;
    s.did(
        "capture",
        json!({"subject": "rhythm:bearings-check", "content": "did the skateboard bearings this morning",
               "sets": {"last_check_in": "2026-06-14"}}),
    )
    .await;
}

/// **A late November that sets the loop's key instead of checking in**, for the
/// reason its June counterpart gives.
async fn late_november_writes_the_turn_by_hand(s: &Script<'_>) {
    s.did(
        "capture",
        json!({"subject": "rhythm:bearings-check", "content": "did the bearings again today",
               "sets": {"last_check_in": "2026-11-22"}}),
    )
    .await;
}

/// **A late November that reaches for the operator's own words, finds
/// nothing under January's handle, and stands up a second loop instead of
/// the first.**
///
/// The room's own document names this failure at Phase 13: the store then
/// holds two loops, each with half the history, and neither can say when the
/// bearings were last done. This one is also written by hand rather than checked
/// in, so the second loop carries the same defect June and this sitting's
/// other guilty variant do — a fixture giving the derivations lock's own
/// scoping something to be wrong about, since January's loop is left
/// untouched and entirely clean.
async fn late_november_stands_up_a_second_loop(s: &Script<'_>) {
    s.did(
        "add_entity",
        json!({"kind": "rhythm", "handle": "orphan", "name": "Wheels service",
               "source": "the operator", "parent": "thing:skateboard"}),
    )
    .await;
    s.did(
        "capture",
        json!({"subject": "rhythm:orphan", "content": "did the wheels again today",
               "sets": {"last_check_in": "2026-11-22"}}),
    )
    .await;
}

/// **A July that corrects March's claim instead of taking it back.**
///
/// The wrong move under the operator's own rule: March and July are different
/// sittings, so this is not a sitting correcting its own mistake in the same
/// breath — it is one sitting editing what an EARLIER one said and calling it
/// the same claim. The product no longer lets it rewrite the words in place, so
/// the defensible-looking move left is to archive March's claim and capture a
/// corrected one derived from it. The words end up right either way, which is
/// exactly why the room needs a lock that can tell it from a retraction.
async fn july_rewrites_in_place(s: &Script<'_>) {
    let wrong = address_of(s.surface, "org:springfield-town-council", "Tuesdays").await;
    corrects_an_earlier_claim(
        s,
        &wrong,
        "the operator was mistaken in March",
        json!({"subject": "org:springfield-town-council",
               "content": "The Springfield Town Council does not meet on Tuesdays — the operator was mistaken in March; that never stood.",
               "provenance": "testimony", "recorded_at": "2026-07-05"}),
    )
    .await;
    repair(s, 3).await;
}

/// **A July that writes only the longboard record and does nothing to the
/// council.** The case a review found this room could not yet fail: once the
/// longboard thread gave July a second subject, a whole-world count of "a record
/// gained July's day" was satisfied by the longboard alone, and "no retraction
/// appeared" holds trivially over a subject nobody touched.
async fn july_writes_only_the_longboard(s: &Script<'_>) {
    repair(s, 3).await;
}

/// **A July that takes the claim back rather than rewriting it in place.**
///
/// The right move under the operator's own rule: March wrote the claim and
/// July is a different sitting correcting it, so the correction stays
/// visible instead of replacing March's own words. Withdrawal alone is
/// complete here — there is nothing else the record needs to say, exactly as
/// Nelson's own retracted attendance needs no replacement claim either.
async fn july(s: &Script<'_>) {
    let wrong = address_of(s.surface, "org:springfield-town-council", "Tuesdays").await;
    s.did(
        "retract",
        json!({"address": wrong,
               "reason": "the council does not meet on Tuesdays and the operator was mistaken",
               "recorded_at": "2026-07-05"}),
    )
    .await;
    repair(s, 3).await;
}

/// **An August that cannot find the two and writes a third.**
///
/// The failure this sitting exists to catch: asked who was at the inspection, a
/// session that does not reach the record fills the gap rather than saying it
/// does not know.
async fn august_puts_a_third_person_there(s: &Script<'_>) {
    august(s).await;
    s.did(
        "capture",
        json!({"subject": "person:ralph", "content": "was at the gorge safety inspection",
               "provenance": "inference",
               "shape": "attendance", "object": "event:gorge-safety-inspection"}),
    )
    .await;
}

async fn august(s: &Script<'_>) {
    s.did(
        "capture",
        json!({"subject": "org:springfield-town-council", "content": "the operator is standing for election to the council board",
               "provenance": "testimony", "recorded_at": "2026-08-16"}),
    )
    .await;
}

async fn september(s: &Script<'_>) {
    s.did(
        "capture",
        // ⚠️ **The one write in the year about a day that is not the
        // sitting's.** Every other claim here is recorded on the day it is
        // made, which the run's own frame now supplies. This one says WHEN
        // THE THING HAPPENED — the skate tool came back at the June inspection — and
        // that is a different field from when the record was made.
        json!({"subject": "thing:skate-tool", "content": "came back at the inspection",
                   "provenance": "testimony", "happened_at": "2026-06-14",
                   "shape": "connection", "object": "person:ralph"}),
    )
    .await;
    repair(s, 4).await;
}

/// **A different, equally honest September, naming Ralph in prose rather than
/// by edge.** Run 20's own model wrote the account this way: an `attendance`
/// edge at the event, with Ralph named inside the sentence — a reasonable
/// account of who returned the skate tool that the skate tool lock's history check must
/// recognise exactly as it recognises the edge-shaped one.
async fn september_names_ralph_in_prose(s: &Script<'_>) {
    s.did(
        "capture",
        json!({"subject": "thing:skate-tool",
               "content": "@person:ralph gave the skate tool back at @event:gorge-safety-inspection",
               "provenance": "testimony", "happened_at": "2026-06-14",
               "shape": "attendance", "object": "event:gorge-safety-inspection"}),
    )
    .await;
}

/// **A guilty September: the skate tool came back and nobody is named at all.** No
/// edge, no mention — so the account behind October's later correction
/// genuinely carries nothing about who returned it.
async fn september_names_nobody(s: &Script<'_>) {
    s.did(
        "capture",
        json!({"subject": "thing:skate-tool", "content": "came back at the inspection",
               "provenance": "testimony", "happened_at": "2026-06-14"}),
    )
    .await;
}

/// **A September that thinks it is June.**
///
/// The fault the day assertion exists for, and the one the exemption used to
/// hide: a sitting told in prose that it is September, whose writes land under
/// some other day. **Nothing else in the run fails** — the prose is in period,
/// the claim is right, and the year quietly collapses.
///
/// ⚠️ **It is played by booting in the wrong day rather than by naming one on
/// the write.** A run's day is where a claim's day now comes from, so sitting
/// in the wrong day IS the fault; naming a day on the call is a different
/// thing and one the surface is about to refuse.
async fn september_sits_in_the_wrong_day(s: &Script<'_>) {
    let confused = sitting(s.surface, "2026-06-14").await;
    confused
        .did(
            "capture",
            json!({"subject": "thing:skate-tool", "content": "came back at the inspection",
                   "provenance": "testimony", "happened_at": "2026-06-14",
                   "shape": "connection", "object": "person:ralph"}),
        )
        .await;
}

async fn october(s: &Script<'_>) {
    october_writes_the_skate_tool_and_the_place(s).await;
    // **The reason is given in words; the rename is the move it was given
    // for.** The operator never names the verb, and finding it is the
    // measurement — the same posture this room takes with every walk it
    // asks for elsewhere.
    s.did(
        "rename_entity",
        json!({"handle": "event:gorge-safety-inspection", "to": "event:rockslide-review"}),
    )
    .await;
}

/// **October, minus the one move its reason was given for.**
///
/// Everything else this sitting is asked to do is done — the skate tool's account
/// and the inspection's place are both on the record — so a run built on this
/// fails only the lock that watches for the rename, never a lock that would
/// fail for an unrelated reason.
async fn october_without_renaming_the_inspection(s: &Script<'_>) {
    october_writes_the_skate_tool_and_the_place(s).await;
}

/// **October retracts June's claim**, the way two of its wrong plays do, naming
/// the reason they share.
async fn october_retracts_junes_claim(s: &Script<'_>) {
    let junes = address_of(s.surface, "org:springfield-town-council", "ran on").await;
    s.did(
        "retract",
        json!({"address": junes,
               "reason": "filing the inspection under its proper record instead"}),
    )
    .await;
}

/// **October reaches the rename's END STATE without ever calling the verb.**
///
/// June's own claim is retracted and a fresh one is written on the council
/// naming a different event by a mention — the same shape a rename leaves,
/// arrived at a different way. A lock anchored to whichever claim currently
/// points at an event cannot tell this from a rename; one anchored to June's
/// own address can, because that address is retracted rather than renamed.
async fn october_retracts_junes_claim_instead_of_renaming(s: &Script<'_>) {
    october_writes_the_skate_tool_and_the_place(s).await;
    october_retracts_junes_claim(s).await;
    stand_up(s, &[("event", "the-jotting", "The Jotting")]).await;
    council_says(
        s,
        "the inspection ran on @place:daredevil-gorge, filed now as @event:the-jotting",
    )
    .await;
}

/// **October retracts June's claim, and ALSO makes the rename it never
/// called happen underneath it.** Neither existing variant tests this
/// combination: the honest October renames and touches nothing retracted;
/// `october_retracts_junes_claim_instead_of_renaming` retracts and never
/// renames. Here the inspection is renamed for real, so a live read of June's
/// now-retracted address renders the NEW handle — the same content a
/// genuine rename would leave — even though this address is not standing.
/// Only a check that reads the address's own status, ahead of comparing
/// what it renders, can tell this dead address from a live one that was
/// actually carried across the rename.
async fn october_retracts_junes_claim_and_renames_the_inspection_anyway(s: &Script<'_>) {
    october_writes_the_skate_tool_and_the_place(s).await;
    october_retracts_junes_claim(s).await;
    s.did(
        "rename_entity",
        json!({"handle": "event:gorge-safety-inspection", "to": "event:rockslide-review"}),
    )
    .await;
}

/// **September's own address on the skate tool, whatever it said.** Excludes
/// February's fixed wording rather than matching September's, because
/// September's own wording is what varies between plays here — a prose
/// account and an edge-shaped one say nothing alike, and finding "the record
/// to correct" by a phrase only one of them uses is how a play meant to work
/// for either ends up silently capturing a second claim instead.
async fn septembers_skate_tool_address(room: &Surface) -> Option<String> {
    facts_of(room, "thing:skate-tool")
        .await?
        .iter()
        .find(|fact| {
            fact["content"].as_str().is_some_and(|content| {
                !content.contains("lent out, wanted back before the inspection")
            })
        })
        .and_then(|fact| fact["address"].as_str())
        .map(str::to_string)
}

/// **Correct September's account when there is one; capture Nelson's fresh
/// when there is not.** The taught distinction, played: archive the earlier
/// claim and capture the correction derived from it when something came before
/// to correct, and a plain `capture` only when nothing did — the shape a year
/// that skipped September actually meets.
///
/// **The correction restates the day the skate tool came back.** The old claim keeps
/// its own, but it is archived, and a reader looking for the day among the
/// standing claims finds it only if the correction says it.
async fn corrects_or_captures_the_skate_tool(s: &Script<'_>) {
    match septembers_skate_tool_address(s.surface).await {
        Some(ralphs) => {
            corrects_an_earlier_claim(
                s,
                &ralphs,
                "Nelson brought the skate tool back, not Ralph",
                json!({"subject": "thing:skate-tool", "content": "brought round over the summer",
                       "provenance": "testimony", "happened_at": "2026-06-14",
                       "shape": "connection", "object": "person:nelson"}),
            )
            .await;
        }
        None => nelson_brought_it_back(s).await,
    }
}

/// Nelson's account of who brought the skate tool back, with no source named.
async fn nelson_brought_it_back(s: &Script<'_>) {
    s.did(
        "capture",
        json!({"subject": "thing:skate-tool", "content": "brought round over the summer",
               "provenance": "testimony",
               "shape": "connection", "object": "person:nelson"}),
    )
    .await;
}

/// **October corrects the skate tool exactly as usual, and ALSO clears the day it
/// came back** — the shape run 20's own model actually wrote. "Over the
/// summer" borrows no precision from "at the inspection", so the exact day
/// Ralph's account carried is no longer accurate and a correction is
/// entitled to remove it. Kept apart from [`corrects_or_captures_the_skate_tool`]:
/// clearing the day is what exposes September's own lock to a legitimate
/// edit from a later sitting, and the canonical play every other case in
/// this file builds on does not need to carry that.
async fn october_corrects_the_skate_tool_and_clears_its_day(s: &Script<'_>) {
    let Some(ralphs) = septembers_skate_tool_address(s.surface).await else {
        panic!("september wrote nothing on the skate tool for october to correct");
    };
    corrects_an_earlier_claim(
        s,
        &ralphs,
        "Nelson brought the skate tool back, not Ralph",
        json!({"subject": "thing:skate-tool", "content": "brought round over the summer",
               "provenance": "testimony",
               "shape": "connection", "object": "person:nelson"}),
    )
    .await;
}

/// **The note lands on the place, which this sitting had to find.** The
/// operator says where we held the inspection and never says where that was;
/// June's claim is the one record that answers it. Without `with_place` the
/// note is filed and never says where.
async fn october_note(s: &Script<'_>, with_place: bool) {
    let mut note = json!({"subject": "event:gorge-safety-inspection",
                          "content": "the ground needs a look before next year",
                          "provenance": "testimony"});
    if with_place {
        note["shape"] = json!("location");
        note["object"] = json!("place:daredevil-gorge");
    }
    s.did("capture", note).await;
}

async fn october_writes_the_skate_tool_and_the_place(s: &Script<'_>) {
    // **A correction of September's claim.** The operator's ruling makes
    // October's account a correction of September's, not a second thing that
    // happened, so September's claim is archived and the correction is derived
    // from it, rather than a fresh claim standing beside an unrevised one.
    corrects_or_captures_the_skate_tool(s).await;
    october_note(s, true).await;
}

/// **An October that picks a winner between the two accounts of how the skate tool
/// came back, instead of leaving both to stand.**
///
/// The skate tool lock's own comment says what is locked is the WRITE: neither
/// account quietly replacing the other. This is that replacement, played —
/// September's account is retracted rather than left standing, in the same
/// sitting that records Nelson's. The location note is still filed exactly as
/// the honest October files it, so this targets one lock and not two.
async fn october_removes_one_of_the_two_accounts(s: &Script<'_>) {
    let ralphs = address_of(s.surface, "thing:skate-tool", "came back at the inspection").await;
    s.did(
        "retract",
        json!({"address": ralphs,
               "reason": "Nelson brought it back, not Ralph",
               "recorded_at": "2026-10-11"}),
    )
    .await;
    nelson_brought_it_back(s).await;
    october_note(s, true).await;
}

/// **An October that writes the note and never says where.**
///
/// The sitting does everything else it is asked and files the note on the event
/// itself, so the skate tool lock still holds and the note is really there. **What it
/// never did is work out WHERE the inspection was held** — the one thing only
/// June's claim says, and the one thing the walk asks for.
async fn october_files_the_note_on_the_event(s: &Script<'_>) {
    corrects_or_captures_the_skate_tool(s).await;
    october_note(s, false).await;
}

/// **October records Nelson's account as a SECOND claim instead of
/// correcting September's.** The operator's ruling makes this a correction;
/// filing a fresh claim beside September's own leaves that address exactly
/// as it was — unrevised, still Ralph's. This is the play the room used to
/// require and the ruling has since overturned.
async fn october_captures_a_second_account_instead_of_correcting(s: &Script<'_>) {
    nelson_brought_it_back(s).await;
    october_note(s, true).await;
}

/// **October archives September's account and files Nelson's with nothing
/// naming its source.** The archive is the product's route, and the correction
/// is there, but nothing on it points back at the account it replaces, so a
/// reader cannot follow the correction to what it corrected. The location note
/// is filed exactly as the honest October files it, so this targets one lock.
async fn october_archives_the_account_and_names_nothing(s: &Script<'_>) {
    let Some(ralphs) = septembers_skate_tool_address(s.surface).await else {
        panic!("september wrote nothing on the skate tool for october to archive");
    };
    s.did(
        "update_fact",
        json!({"address": ralphs, "status": "archived",
               "details": "Nelson brought the skate tool back, not Ralph"}),
    )
    .await;
    s.did(
        "capture",
        json!({"subject": "thing:skate-tool", "content": "brought round over the summer",
               "provenance": "testimony", "happened_at": "2026-06-14",
               "shape": "connection", "object": "person:nelson"}),
    )
    .await;
    october_note(s, true).await;
}

/// **The retraction case, worked for the first time all year.** Nelson's
/// inspection attendance was never true, and the record it corrects is a June
/// fact that only exists because January stood up the event and February
/// stood up Nelson — the dependency the room is built to test. Bart's
/// membership is folded in beside it, the same shape February used for
/// Nelson himself.
async fn late_october(s: &Script<'_>) {
    let wrong = address_of(s.surface, "person:nelson", "gorge safety inspection").await;
    s.did(
        "retract",
        json!({"address": wrong,
               "reason": "Nelson never actually made it to the inspection — he was fixing a cracked deck that morning",
               "recorded_at": "2026-10-24"}),
    )
    .await;
    stand_up(s, &[("person", "bart", "Bart")]).await;
    s.did(
        "capture",
        json!({"subject": "person:bart", "content": "joined the council",
               "provenance": "testimony",
               "shape": "membership", "object": "org:springfield-town-council"}),
    )
    .await;
}

/// **A late October that files the new arrival at the inspection too.**
///
/// The mistake this sitting is the first one able to make: it is handed a new
/// person and it is already writing about the inspection, so putting the two
/// together is one plausible step rather than an invention out of nothing.
async fn late_october_puts_a_third_person_there(s: &Script<'_>) {
    late_october(s).await;
    let inspection = handle_of_kind(s.surface, "event").await;
    s.did(
        "capture",
        json!({"subject": "person:bart", "content": "was at the gorge safety inspection",
               "provenance": "inference",
               "shape": "attendance", "object": inspection}),
    )
    .await;
}

/// **The turn asked for in words the record does not use.** The operator says
/// wheels and service; the loop January opened is a bearings check. A sitting
/// that reaches the loop records the turn on it, and one that does not stands a
/// second loop beside it.
async fn late_november(s: &Script<'_>) {
    late_november_filing(
        s,
        [
            "the shop serviced the wheels",
            "the skate tool needed a new washer",
        ],
        ["sent", "billed"],
    )
    .await;
}

/// **Late November, with the words the two jobs are filed under named.** The
/// year files them under words the operator does not use; the best sitting files
/// them under the operator's own from the start, and words them as it likes.
async fn late_november_filing(s: &Script<'_>, said: [&str; 2], words: [&str; 2]) {
    s.did(
        "capture",
        json!({"subject": "rhythm:bearings-check", "content": "did the bearings again today",
               "provenance": "testimony",
               "check_in": "ran"}),
    )
    .await;
    // **The second figure, joining June's under the same key.** No second
    // declaration: the fold behaviour is the key's own, decided once in June.
    s.did(
        "capture",
        json!({"subject": "thing:skateboard", "content": "around 600 kilometres since June",
               "provenance": "testimony", "sets": {"km": "600"}}),
    )
    .await;
    // **The job off the vocabulary January declared — on its own subject,
    // not sharing a key with any other job.** `cost`/`settled` are fresh on
    // `thing:skateboard`, so nothing here folds against another job's own
    // figures the way two jobs on one subject would.
    s.did(
        "capture",
        json!({"subject": "thing:skateboard", "content": said[0],
               "provenance": "testimony", "sets": {"cost": "45", "settled": words[0]}}),
    )
    .await;
    // **The second job off the vocabulary — its own subject, its own wrong
    // word.** "billed" is not one of the operator's three, and it is wrong
    // for a reason independent of "sent" above: two odd words rather than
    // one, on purpose, for the reason `year.md` gives beside that lock.
    s.did(
        "capture",
        json!({"subject": "thing:skate-tool", "content": said[1],
               "provenance": "testimony", "sets": {"cost": "20", "settled": words[1]}}),
    )
    .await;
}

/// **The same-breath mistake this entry asks a sitting to make and catch.**
///
/// A fresh claim, wrong, corrected under the SAME `sid` before the sitting
/// ends — the positive case the operator's session-boundary rule needs: the
/// mistake and its correction are one session, so a rewrite in place is
/// right, unlike March's claim four sittings before July's.
async fn board_lock_corrected_in_the_same_breath(s: &Script<'_>) -> String {
    stand_up(s, &[BOARD_LOCK]).await;
    let address = captured_address(
        &s.did(
            "capture",
            json!({"subject": "thing:board-lock", "content": "bought a U-lock for the skateboard",
                   "provenance": "testimony", "recorded_at": "2026-12-13"}),
        )
        .await,
    );
    s.did(
        "update_fact",
        json!({"address": address,
               "content": "bought a cable lock for the skateboard, not the other kind", "provenance": "testimony"}),
    )
    .await;
    address
}

/// **A job off the vocabulary, corrected by the product's route**: the earlier
/// job is archived and the corrected one is captured. `names_the_source` is
/// whether the correction says which job it replaces, with `derived_from`; the
/// room's one wrong play leaves it off.
async fn corrects_a_job(
    s: &Script<'_>,
    wrong_word: &str,
    names_the_source: bool,
    mut correction: Value,
) {
    let reason = "filed under a word the operator does not use";
    if names_the_source {
        corrects_an_earlier_claim(s, wrong_word, reason, correction).await;
        return;
    }
    s.did(
        "update_fact",
        json!({"address": wrong_word, "status": "archived", "details": reason}),
    )
    .await;
    correction
        .as_object_mut()
        .map(|call| call.remove("derived_from"));
    s.did("capture", correction).await;
}

/// **The address of the job on `subject` whose words carry `needle`, when it is
/// filed under a word the operator does not use.** A sitting that finds the job
/// already under the operator's own word has nothing to put right and leaves it.
async fn off_vocabulary_job(room: &Surface, subject: &str, needle: &str) -> Option<String> {
    let address = try_address_of(room, subject, needle).await?;
    let fact = facts_of(room, subject)
        .await?
        .into_iter()
        .find(|fact| fact["address"] == address.as_str())?;
    // The word is a key the record CARRIES, in whichever bag the sitting put it.
    let word = fact["fields"]["settled"]
        .as_str()
        .or_else(|| fact["sets"]["settled"].as_str())
        .unwrap_or_default();
    (!OPERATORS_WORDS.contains(&word)).then_some(address)
}

/// **What the board lock's correction used to say**, from the record's own
/// history, written down as the `was` of a note on the lock. The wording is not
/// guessable and not on any other read: the claim as it stands says the
/// opposite, so a sitting that answers from current truth has nothing to put here.
async fn board_lock_was(s: &Script<'_>) {
    let address = board_lock_corrected_in_the_same_breath(s).await;
    let trace = s
        .surface
        .call(
            "recall",
            json!({"subject": "thing:board-lock", "history_record": address}),
        )
        .await;
    let parsed: Value = serde_json::from_str(&trace).expect("the trace is json");
    let was = parsed["objects"][0]["record_history"]["writes"][0]["content"]
        .as_str()
        .unwrap_or_else(|| panic!("the correction left no earlier write: {trace}"))
        .to_string();
    s.did(
        "capture",
        json!({"subject": "thing:board-lock", "content": "the record was corrected during the year",
               "provenance": "inference",
               "sets": {"was": was}}),
    )
    .await;
}

/// **The first later December sitting**: the lock bought and corrected in the
/// same breath, and the old wording put under `was`.
async fn later_december_the_lock(s: &Script<'_>) {
    board_lock_was(s).await;
    // **The control this rests on, filed under the operator's own word in
    // the same breath the board lock is created.** No separate sitting to
    // hold it: the board lock exists only from this entry on, so what
    // "filed correctly from the start" means here is that this sitting
    // does not touch this one.
    s.did(
        "capture",
        json!({"subject": "thing:board-lock", "content": "the shop waived the fitting charge",
               "provenance": "testimony", "sets": {"cost": "15", "settled": "waived"}}),
    )
    .await;
}

/// **The second later December sitting**: the jobs filed under words the
/// operator does not use put right, and the longboard's pile folded.
async fn later_december_the_longboard_and_the_words(s: &Script<'_>, names_the_source: bool) {
    // **The one job off the vocabulary January declared, put right — if
    // late November's own job is there to find.** Several cases drive a
    // partial year that skips late November for reasons of their own; this
    // sitting still has its other business to do, so it looks rather than
    // assumes.  "sent" is not one of the operator's own three words —
    // reachable only by having kept the declaration, since nothing about
    // "sent" looks wrong on its own.
    if let Some(wrong_word) =
        off_vocabulary_job(s.surface, "thing:skateboard", "serviced the wheels").await
    {
        corrects_a_job(
            s,
            &wrong_word,
            names_the_source,
            json!({"subject": "thing:skateboard", "content": "the shop serviced the wheels",
                   "provenance": "testimony",
                   "sets": {"cost": "45", "settled": "invoiced"}}),
        )
        .await;
    }
    // **The second job off the vocabulary, put right the same way — its own
    // subject, its own address, if late November's own job is there to
    // find.**
    if let Some(wrong_word) =
        off_vocabulary_job(s.surface, "thing:skate-tool", "needed a new washer").await
    {
        corrects_a_job(
            s,
            &wrong_word,
            names_the_source,
            json!({"subject": "thing:skate-tool", "content": "the skate tool needed a new washer",
                   "provenance": "testimony",
                   "sets": {"cost": "20", "settled": "invoiced"}}),
        )
        .await;
    }
    fold_the_longboards_pile(s).await;
}

/// **The third later December sitting**: what is actually invoiced, summed and
/// put under `owed`.
async fn later_december_the_totals(s: &Script<'_>) {
    // **THE SELECTIVE SUM — filter by the declared word, then add.** Reads
    // every job this sitting can reach and totals the ones marked invoiced,
    // never the unconditional fold `km` uses a few lines above and never a
    // total the sitting invents from memory.
    let owed = invoiced_total(s.surface).await;
    s.did(
        "capture",
        json!({"subject": "thing:skateboard", "content": "what is actually invoiced across this year's jobs",
               "provenance": "inference", "sets": {"owed": owed.to_string()}}),
    )
    .await;
}

/// **THE SELECTIVE SUM, done properly.** Every job-bearing thing the year
/// holds, filtered on the operator's own word and then added — never the
/// key's own unconditional fold, and never a total the sitting invents.
async fn invoiced_total(room: &Surface) -> u32 {
    let mut total = 0;
    for subject in [
        "thing:longboard",
        "thing:skateboard",
        "thing:skate-tool",
        "thing:board-lock",
    ] {
        let read = room.call("recall", json!({"subject": subject})).await;
        let Ok(parsed) = serde_json::from_str::<Value>(&read) else {
            continue;
        };
        let fields = &parsed["objects"][0]["fields"];
        if fields["settled"].as_str() != Some("invoiced") {
            continue;
        }
        if let Some(cost) = fields["cost"].as_str().and_then(|c| c.parse::<u32>().ok()) {
            total += cost;
        }
    }
    total
}

/// The addresses of every fact `recall` currently reports for the longboard.
async fn longboard_sources(room: &Surface) -> Vec<String> {
    facts_of(room, "thing:longboard")
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|fact| fact["address"].as_str().map(str::to_string))
        .collect()
}

/// **A fold of the longboard's pile, standing for `sources`.** A fresh record
/// stands for the repairs, none of which is touched — the pile stays fully
/// readable and gains a single answer on top of it, which is the shape the
/// operator's own request asked for without naming. `happened_at` is the day the
/// fold claims for itself, when it claims one.
async fn fold_longboard(
    s: &Script<'_>,
    content: &str,
    happened_at: Option<&str>,
    sources: &[String],
) -> String {
    let mut fold = json!({"subject": "thing:longboard", "content": content,
                          "provenance": "inference"});
    if let Some(day) = happened_at {
        fold["happened_at"] = json!(day);
    }
    let address = captured_address(&s.did("capture", fold).await);
    s.did(
        "update_fact",
        json!({"address": address, "stands_for": sources}),
    )
    .await;
    address
}

/// **The fold this room's own lock is watching for, done honestly.**
async fn fold_the_longboards_pile(s: &Script<'_>) {
    fold_longboard(s, FOLD_SAYS, None, &longboard_sources(s.surface).await).await;
}

/// **A fold that names its sources and then loses one of them.** The mark
/// itself is honest; a later act on the SAME sitting retracts one of the five
/// repairs it just named, which is the shape `stands_for` promises never
/// happens — synthesis layers and never discards.
async fn later_december_folds_but_loses_a_source(s: &Script<'_>) {
    board_lock_was(s).await;
    let sources = longboard_sources(s.surface).await;
    fold_longboard(s, FOLD_SAYS, None, &sources).await;
    s.did(
        "retract",
        json!({"address": sources[0], "reason": "never mind, this one did not need a note"}),
    )
    .await;
}

/// **A fold that invents a date none of the longboard's five repairs ever gave.**
/// The mark itself is honest and names every source; the fold's own
/// `happened_at` claims a single day for a pile that spans half the year,
/// which none of the five repairs said.
async fn later_december_folds_with_an_invented_date(s: &Script<'_>) {
    board_lock_was(s).await;
    let sources = longboard_sources(s.surface).await;
    fold_longboard(s, FOLD_SAYS, Some("2026-04-01"), &sources).await;
}

/// **A fold that invents a date in its own PROSE rather than in a structured
/// field.** The check's own comment calls prose the place an invented date
/// most naturally lands, since a fold summarizing a year in words is more
/// likely to say a day than to carry one as a value nobody asked for — and
/// until this, nothing planted one there.
async fn later_december_folds_with_an_invented_date_in_prose(s: &Script<'_>) {
    board_lock_was(s).await;
    let sources = longboard_sources(s.surface).await;
    fold_longboard(
        s,
        "the longboard needed a handful of small repairs this year, wrapping up on 2026-04-01 — the details are on the records this stands for",
        None,
        &sources,
    )
    .await;
}

/// **A fold that names its own write day in its own prose.** Naming the day
/// a record was itself written is not invention — a review found the
/// fabrication check once excluded the fold's own date from what it treated
/// as known, which would have convicted exactly this honest fold.
async fn fold_the_longboards_pile_naming_its_own_day(s: &Script<'_>) {
    fold_longboard(
        s,
        "as of 2026-12-20 the longboard has needed five small repairs this year — the details are on the records this stands for",
        None,
        &longboard_sources(s.surface).await,
    )
    .await;
}

/// **Two longboard facts on the same day, and a fold that names only one of
/// them.** February plausibly writes an acquisition and a defect as two
/// separate claims on the same date; a review found the sources check once
/// found only the FIRST record `recorded_at` reported for a day, so a fold
/// missing this one's sibling would have read as complete.
async fn fold_the_longboards_pile_missing_a_same_day_sibling(s: &Script<'_>) {
    let sources = longboard_sources(s.surface).await;
    s.did(
        "capture",
        json!({"subject": "thing:longboard", "content": "picked it up at a yard sale",
               "provenance": "testimony", "recorded_at": LONGBOARD_DAYS[0].0}),
    )
    .await;
    fold_longboard(s, FOLD_SAYS, None, &sources).await;
}

/// **A later December that answers from the claim as it stands.**
///
/// The wrong route, and the plausible one: the record says a cable lock, so a
/// sitting that never reaches the correction's own history writes that down.
/// **It answers, under the key it was asked for, and it is the opposite of
/// what the claim used to say.**
async fn later_december_answers_from_the_claim_as_it_stands(s: &Script<'_>) {
    board_lock_corrected_in_the_same_breath(s).await;
    let now = facts_of(s.surface, "thing:board-lock").await.expect("json")[0]["content"]
        .as_str()
        .expect("the board lock's claim")
        .to_string();
    s.did(
        "capture",
        json!({"subject": "thing:board-lock", "content": "the record was corrected during the year",
               "provenance": "inference",
               "sets": {"was": now}}),
    )
    .await;
}

/// **A later December that writes the old wording inside a fuller sentence.**
///
/// What a real sitting did: it answered the question and said more than the
/// minimum — the old wording, plus when it was recorded and when it was put
/// right. **It is a better answer than the bare one**, and a needle demanding
/// the value BE the old wording scored it as not having answered at all.
///
/// ⚠️ **Played by hand rather than through [`Play`]**, because the sitting's own
/// play is the wrong answer above: naming this one in a [`Year`] would put it
/// beside the honest sitting rather than in its place.
async fn later_december_writes_a_fuller_sentence(s: &Script<'_>) {
    stand_up(s, &[BOARD_LOCK]).await;
    s.did(
        "capture",
        json!({"subject": "thing:board-lock", "content": "the record was corrected during the year",
               "provenance": "inference",
               "sets": {"was": "bought a U-lock for the skateboard (recorded 2026-12-13, corrected the same sitting)"}}),
    )
    .await;
}

/// **A December that corrects a claim nobody raised — and this is LEGITIMATE.**
///
/// A sitting may notice a mistake and correct it. The claim was written by an
/// earlier sitting, so the correction is the product's route: archive it and
/// capture the corrected claim derived from it. The pairing's lock used to
/// score this as a fault, because it demanded the record carry one write and
/// no other; that asserted what the model happened to do rather than anything
/// about jojobot, and a paid run failed on it while the product did nothing
/// wrong. **The play is kept and its verdict is flipped**: a run that really
/// made a second write must produce a trace reporting two.
async fn december_corrects_a_claim_nobody_questioned(s: &Script<'_>) {
    corrects_an_earlier_claim(
        s,
        "person:bart#f1",
        "Bart joined in the autumn",
        json!({"subject": "person:bart", "content": "joined the council in the autumn",
               "provenance": "testimony", "recorded_at": "2026-12-13",
               "shape": "membership", "object": "org:springfield-town-council"}),
    )
    .await;
}

// ──────────────────────────────── the year ────────────────────────────────

/// **The seventeen sittings, in the order the document writes them.**
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Sitting {
    January,
    February,
    March,
    April,
    May,
    June,
    July,
    August,
    September,
    October,
    LateOctober,
    /// The one sitting a person reads: it asks a question and records nothing.
    ReadThis,
    LateNovember,
    December,
    /// Later December is three sittings: the lock and the council's question, the
    /// longboard and the words, then the distance and what is owed.
    LateDecemberLock,
    LateDecemberWords,
    LateDecemberTotals,
}

impl Sitting {
    const ALL: [Sitting; 17] = [
        Sitting::January,
        Sitting::February,
        Sitting::March,
        Sitting::April,
        Sitting::May,
        Sitting::June,
        Sitting::July,
        Sitting::August,
        Sitting::September,
        Sitting::October,
        Sitting::LateOctober,
        Sitting::ReadThis,
        Sitting::LateNovember,
        Sitting::December,
        Sitting::LateDecemberLock,
        Sitting::LateDecemberWords,
        Sitting::LateDecemberTotals,
    ];

    /// **The sittings that record something**, named rather than counted: the one
    /// a person reads writes nothing by design, and it sits between the sittings
    /// that do, so a range cannot say it.
    const WORKED: [Sitting; 16] = [
        Sitting::January,
        Sitting::February,
        Sitting::March,
        Sitting::April,
        Sitting::May,
        Sitting::June,
        Sitting::July,
        Sitting::August,
        Sitting::September,
        Sitting::October,
        Sitting::LateOctober,
        Sitting::LateNovember,
        Sitting::December,
        Sitting::LateDecemberLock,
        Sitting::LateDecemberWords,
        Sitting::LateDecemberTotals,
    ];

    /// Where the sitting sits in the document.
    fn at(self) -> usize {
        self as usize
    }

    /// The sitting done the way it is meant to be.
    async fn work(self, s: &Script<'_>) {
        match self {
            Sitting::January => january(s).await,
            Sitting::February => february(s, true).await,
            Sitting::March => march(s).await,
            Sitting::April => april(s).await,
            Sitting::May => may(s).await,
            Sitting::June => june_saying(s, WHAT_JUNE_SAW).await,
            Sitting::July => july(s).await,
            Sitting::August => august(s).await,
            Sitting::September => september(s).await,
            Sitting::October => october(s).await,
            Sitting::LateOctober => late_october(s).await,
            Sitting::ReadThis => {}
            Sitting::LateNovember => late_november(s).await,
            // December asks for a look back and records nothing: the loop is not
            // owed, so there is no overdue note to write.
            Sitting::December => {}
            Sitting::LateDecemberLock => later_december_the_lock(s).await,
            Sitting::LateDecemberWords => later_december_the_longboard_and_the_words(s, true).await,
            Sitting::LateDecemberTotals => later_december_the_totals(s).await,
        }
    }
}

/// **A sitting done wrong, or done differently, for a lock to tell apart.** One
/// play replaces the honest sitting it belongs to. Two plays at one sitting
/// would turn two locks red at once and no case could say which wrong it
/// measured, so a [`Year`] refuses them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Play {
    /// June's claim in words: nothing for a later sitting to follow.
    JuneInWords,
    /// June's claim pointing at a place and a person: two kinds, no event.
    JuneWithoutTheEvent,
    /// June's claim pointing only at people: one kind.
    JunePointingAtOneKind,
    /// June, honest, filing attendance as a mention on each person too.
    JuneAttendanceAsMentions,
    /// June, plus a second event neither October nor its reason touches.
    JuneAlsoPointsAtASecondEvent,
    /// June, honest, drawing its edges at an event spelled another way.
    JunePointsAtADifferentlySpelledInspection,
    /// June setting the loop's key by hand instead of checking in.
    JuneWritesTheTurnByHand,
    /// September, honest, naming Ralph in prose with an attendance edge.
    SeptemberNamesRalphInProse,
    /// September, guilty: the skate tool came back and nobody is named.
    SeptemberNamesNobody,
    /// September sitting in June's day.
    SeptemberSitsInTheWrongDay,
    /// October writing the note on the event and never saying where.
    OctoberFilesTheNoteOnTheEvent,
    /// October, honest, also clearing the day the skate tool came back.
    OctoberAlsoClearsTheDay,
    /// October retracting September's account instead of correcting it.
    OctoberRemovesOneOfTheTwoAccounts,
    /// October never making the rename its reason was given for.
    OctoberDoesNotRename,
    /// October reaching the rename's end state by a retraction and a fresh claim.
    OctoberRetractsJunesClaimInsteadOfRenaming,
    /// October retracting June's claim and renaming the inspection anyway.
    OctoberRetractsJunesClaimAndRenamesAnyway,
    /// October filing Nelson's account as a second claim, not a correction.
    OctoberCapturesASecondAccount,
    /// October archiving September's account and naming no source for Nelson's.
    OctoberArchivesAndNamesNothing,
    /// July correcting March's claim in place instead of taking it back.
    JulyRewritesInPlace,
    /// August writing a third person at the inspection.
    AugustPutsAThirdPersonThere,
    /// Late October writing the new arrival at the inspection.
    LateOctoberPutsAThirdPersonThere,
    /// February recording no holder of the skate tool.
    FebruaryRecordsNoHolder,
    /// Late November setting the loop's key by hand instead of checking in.
    LateNovemberWritesTheTurnByHand,
    /// Late November standing up a second loop instead of the first.
    LateNovemberStandsUpASecondLoop,
    /// Late November's best sitting: both jobs filed under the operator's word.
    LateNovemberFilesTheJobsUnderTheOperatorsWord,
    /// December correcting a claim nobody questioned — legitimate.
    DecemberCorrectsAClaimNobodyQuestioned,
    /// Later December answering from the claim as it stands.
    LaterDecemberAnswersFromTheClaimAsItStands,
    /// Later December putting both jobs right and naming neither source.
    LaterDecemberCorrectsTheJobsAndNamesNoSource,
}

impl Play {
    /// The sitting this play replaces.
    fn sitting(self) -> Sitting {
        match self {
            Play::JuneInWords
            | Play::JuneWithoutTheEvent
            | Play::JunePointingAtOneKind
            | Play::JuneAttendanceAsMentions
            | Play::JuneAlsoPointsAtASecondEvent
            | Play::JunePointsAtADifferentlySpelledInspection
            | Play::JuneWritesTheTurnByHand => Sitting::June,
            Play::SeptemberNamesRalphInProse
            | Play::SeptemberNamesNobody
            | Play::SeptemberSitsInTheWrongDay => Sitting::September,
            Play::OctoberFilesTheNoteOnTheEvent
            | Play::OctoberAlsoClearsTheDay
            | Play::OctoberRemovesOneOfTheTwoAccounts
            | Play::OctoberDoesNotRename
            | Play::OctoberRetractsJunesClaimInsteadOfRenaming
            | Play::OctoberRetractsJunesClaimAndRenamesAnyway
            | Play::OctoberCapturesASecondAccount
            | Play::OctoberArchivesAndNamesNothing => Sitting::October,
            Play::JulyRewritesInPlace => Sitting::July,
            Play::AugustPutsAThirdPersonThere => Sitting::August,
            Play::LateOctoberPutsAThirdPersonThere => Sitting::LateOctober,
            Play::FebruaryRecordsNoHolder => Sitting::February,
            Play::LateNovemberWritesTheTurnByHand
            | Play::LateNovemberStandsUpASecondLoop
            | Play::LateNovemberFilesTheJobsUnderTheOperatorsWord => Sitting::LateNovember,
            Play::DecemberCorrectsAClaimNobodyQuestioned => Sitting::December,
            Play::LaterDecemberAnswersFromTheClaimAsItStands => Sitting::LateDecemberLock,
            Play::LaterDecemberCorrectsTheJobsAndNamesNoSource => Sitting::LateDecemberWords,
        }
    }

    async fn perform(self, s: &Script<'_>) {
        match self {
            Play::JuneInWords => june_saying(s, IN_WORDS).await,
            Play::JuneWithoutTheEvent => june_saying(s, A_PLACE_AND_A_PERSON).await,
            Play::JunePointingAtOneKind => june_saying(s, PEOPLE_ONLY).await,
            Play::JuneAttendanceAsMentions => june_with_attendance_as_mentions(s).await,
            Play::JuneAlsoPointsAtASecondEvent => june_also_points_at_a_second_event(s).await,
            Play::JunePointsAtADifferentlySpelledInspection => {
                june_points_at_a_differently_spelled_inspection(s).await
            }
            Play::JuneWritesTheTurnByHand => june_writes_the_turn_by_hand(s).await,
            Play::SeptemberNamesRalphInProse => september_names_ralph_in_prose(s).await,
            Play::SeptemberNamesNobody => september_names_nobody(s).await,
            Play::SeptemberSitsInTheWrongDay => september_sits_in_the_wrong_day(s).await,
            Play::OctoberFilesTheNoteOnTheEvent => october_files_the_note_on_the_event(s).await,
            Play::OctoberAlsoClearsTheDay => {
                october_corrects_the_skate_tool_and_clears_its_day(s).await
            }
            Play::OctoberRemovesOneOfTheTwoAccounts => {
                october_removes_one_of_the_two_accounts(s).await
            }
            Play::OctoberDoesNotRename => october_without_renaming_the_inspection(s).await,
            Play::OctoberRetractsJunesClaimInsteadOfRenaming => {
                october_retracts_junes_claim_instead_of_renaming(s).await
            }
            Play::OctoberRetractsJunesClaimAndRenamesAnyway => {
                october_retracts_junes_claim_and_renames_the_inspection_anyway(s).await
            }
            Play::OctoberCapturesASecondAccount => {
                october_captures_a_second_account_instead_of_correcting(s).await
            }
            Play::OctoberArchivesAndNamesNothing => {
                october_archives_the_account_and_names_nothing(s).await
            }
            Play::JulyRewritesInPlace => july_rewrites_in_place(s).await,
            Play::AugustPutsAThirdPersonThere => august_puts_a_third_person_there(s).await,
            Play::LateOctoberPutsAThirdPersonThere => {
                late_october_puts_a_third_person_there(s).await
            }
            Play::FebruaryRecordsNoHolder => february(s, false).await,
            Play::LateNovemberWritesTheTurnByHand => late_november_writes_the_turn_by_hand(s).await,
            Play::LateNovemberStandsUpASecondLoop => late_november_stands_up_a_second_loop(s).await,
            Play::LateNovemberFilesTheJobsUnderTheOperatorsWord => {
                late_november_filing(
                    s,
                    [
                        "wheels service at the skateboard shop",
                        "a washer for the skate tool",
                    ],
                    ["invoiced", "invoiced"],
                )
                .await
            }
            Play::DecemberCorrectsAClaimNobodyQuestioned => {
                december_corrects_a_claim_nobody_questioned(s).await
            }
            Play::LaterDecemberAnswersFromTheClaimAsItStands => {
                later_december_answers_from_the_claim_as_it_stands(s).await
            }
            Play::LaterDecemberCorrectsTheJobsAndNamesNoSource => {
                later_december_the_longboard_and_the_words(s, false).await
            }
        }
    }
}

/// **Which sittings of the year run, and which plays replace them.**
///
/// A sitting nobody works gets no run: a boot mints nothing until its first
/// write, so a run for a sitting a drive skips is a run that never happened, and
/// it would sit in the day that sitting claims, where the next drive of the same
/// day meets it still working. **A play runs even when its sitting is outside the
/// set**: naming a wrong play is asking for that sitting to happen wrongly.
struct Year {
    sittings: Vec<Sitting>,
    plays: Vec<Play>,
}

impl Year {
    /// The whole year, worked the way it is meant to be.
    fn worked() -> Self {
        Self::only(&Sitting::WORKED)
    }

    /// Exactly these sittings. **Naming them rather than taking a first one**: the
    /// case that starts the year in June deliberately leaves July and late October
    /// out as well, because both edit a claim no earlier sitting wrote, and a
    /// range cannot say that.
    fn only(sittings: &[Sitting]) -> Self {
        Year {
            sittings: sittings.to_vec(),
            plays: Vec::new(),
        }
    }

    fn without(mut self, left_out: &[Sitting]) -> Self {
        self.sittings.retain(|sitting| !left_out.contains(sitting));
        self
    }

    fn play(mut self, play: Play) -> Self {
        if let Some(first) = self.plays.iter().find(|p| p.sitting() == play.sitting()) {
            panic!(
                "{first:?} and {play:?} are both plays of {:?}",
                play.sitting()
            );
        }
        self.plays.push(play);
        self
    }

    /// Whether the year leaves nothing out, which is the only year in which a
    /// refusal of an absent thing is a fault.
    fn is_whole(&self) -> bool {
        Sitting::WORKED.iter().all(|s| self.sittings.contains(s))
    }

    fn play_at(&self, sitting: Sitting) -> Option<Play> {
        self.plays.iter().copied().find(|p| p.sitting() == sitting)
    }

    fn runs(&self, sitting: Sitting) -> bool {
        self.sittings.contains(&sitting) || self.play_at(sitting).is_some()
    }

    /// **The year worked sitting by sitting, taking the readings a run takes.**
    ///
    /// One driver for every case that works the year, because they were copies of
    /// the same order and a check scoped to a sitting's window needs the readings
    /// either way. **The run's own boundary names, from the run's own rule** —
    /// naming them here would prove a shape this suite invented.
    async fn work(&self, room: &Surface) -> Vec<Boundary> {
        let year = room_document();
        assert_eq!(
            year.phases.len(),
            Sitting::ALL.len(),
            "the document and the sittings disagree"
        );
        let named = boundary_names(&year);
        let mut boundaries = vec![boundary(room, &named[0]).await];
        for (phase, sitting_of) in year.phases.iter().zip(Sitting::ALL) {
            if self.runs(sitting_of) {
                let day = phase
                    .day
                    .as_deref()
                    .unwrap_or_else(|| panic!("{} claims no day", phase.name));
                let mut script = sitting(room, day).await;
                script.skipping = !self.is_whole();
                // **The sitting does the wrong thing, in its own window.** Here
                // rather than in a second driver: two copies of this order was
                // how they came to disagree about which sittings write.
                match self.play_at(sitting_of) {
                    Some(play) => play.perform(&script).await,
                    None => sitting_of.work(&script).await,
                }
            }
            boundaries.push(boundary(room, &named[sitting_of.at() + 1]).await);
        }
        boundaries
    }
}

/// The whole year, worked the way it is meant to be.
async fn worked_the_year(room: &Surface) -> Vec<Boundary> {
    Year::worked().work(room).await
}

/// **The year as it stands before December**: every sitting through late
/// November. A case plays its own version of what comes after.
fn year_before_december() -> Year {
    Year::worked().without(&[
        Sitting::December,
        Sitting::LateDecemberLock,
        Sitting::LateDecemberWords,
        Sitting::LateDecemberTotals,
    ])
}

// ───────────────────────────────── the locks ─────────────────────────────────

/// **The locks the year carries, in the order its document writes them.**
///
/// The names are this file's rather than the document's: what is pinned is the
/// order and what each lock is about, so a sentence rewritten in the room does
/// not break a case here.
const JANUARY: [usize; 3] = [0, 1, 2];
const FEBRUARY: [usize; 4] = [3, 4, 5, 6];
const MARCH: [usize; 2] = [7, 8];
const APRIL: [usize; 2] = [9, 10];
const MAY: [usize; 2] = [11, 12];
const JUNE: [usize; 3] = [13, 14, 15];
const JULY: [usize; 2] = [16, 17];
const AUGUST: [usize; 2] = [18, 19];
const SEPTEMBER: [usize; 2] = [20, 21];
const OCTOBER: [usize; 2] = [22, 23];
const LATE_OCTOBER: [usize; 4] = [24, 25, 26, 27];
const LATE_NOVEMBER: [usize; 2] = [28, 29];

/// **December's one lock** — where the bearings loop stands on the day, and
/// nothing else: the rest of that sitting is still unasserted, by the
/// room's own design.
const DECEMBER: [usize; 1] = [30];

/// **The sixteen locks of later December, addressed by what each is about**
/// (the order the names below follow) and not by where the document writes
/// them. Three entries carry them, grouped by the entry that asks: the lock and
/// the council's question at 31 to 35, the longboard and the words at 36 to 44,
/// the distance and what is owed at 45 and 46.
const LATE_DECEMBER: [usize; 16] = [
    33, 34, 36, 37, 38, 31, 39, 40, 45, 35, 41, 42, 43, 32, 46, 44,
];

const LOCKS: usize = 47;

/// **The sittings a person reads**, which assert nothing and must not.
const READ_THESE: [&str; 1] = ["Phase 12"];

/// **A sitting marked to be read that a lock still asserts over.** The mark drops
/// the generated assertion that some record carries the sitting's day, which
/// December cannot meet because nothing is owed that day; the lock on the loop's
/// next due day stays.
const READ_BUT_LOCKED: [&str; 1] = ["Phase 14"];

// The locks the cases name one at a time.
const BOX_OPENED: usize = JANUARY[0];
const SKATE_TOOL_HOLDER: usize = FEBRUARY[1];
const COUNCIL_MEMBERSHIP: usize = FEBRUARY[2];
const MARCH_RECORDED: usize = MARCH[0];
const SHELBYVILLE: usize = APRIL[0];
const JUNE_ATTENDEES: usize = JUNE[0];
const JUNE_BEARINGS: usize = JUNE[1];
const JUNE_HANDLES: usize = JUNE[2];
const JULY_TAKES_BACK: usize = JULY[0];
const AUGUST_INVENTS_NOBODY: usize = AUGUST[0];
const AUGUST_STANDS_ALONE: usize = AUGUST[1];
const SKATE_TOOL_DAY: usize = SEPTEMBER[0];
const SKATE_TOOL_CORRECTED: usize = OCTOBER[0];
const WALK_TO_THE_PLACE: usize = OCTOBER[1];
const BART_JOINS: usize = LATE_OCTOBER[1];
const LATE_OCTOBER_INVENTS_NOBODY: usize = LATE_OCTOBER[2];
const INSPECTION_RENAMED: usize = LATE_OCTOBER[3];
const TURNS_ARE_DERIVATIONS: usize = LATE_NOVEMBER[0];
const LATE_NOVEMBER_TURN: usize = LATE_NOVEMBER[1];
const BEARINGS_LOOP_STANDS: usize = DECEMBER[0];
const WAS_WORDING: usize = LATE_DECEMBER[0];
const TRACE_AGREES: usize = LATE_DECEMBER[1];
const FOLD_APPEARS: usize = LATE_DECEMBER[2];
const FIVE_REPAIRS_STAY: usize = LATE_DECEMBER[3];
const NO_INVENTED_DATE: usize = LATE_DECEMBER[4];
const BOARD_LOCK_REWRITTEN: usize = LATE_DECEMBER[5];
const DISTANCE_SUMS: usize = LATE_DECEMBER[8];
const HEDGE_STAYS_OPEN: usize = LATE_DECEMBER[9];
const WHEELS_JOB: usize = LATE_DECEMBER[10];
const LONGBOARD_CONTROL: usize = LATE_DECEMBER[11];
const SKATE_TOOL_JOB: usize = LATE_DECEMBER[12];
const BOARD_LOCK_JOB: usize = LATE_DECEMBER[13];
const WHAT_IS_OWED: usize = LATE_DECEMBER[14];
const CORRECTIONS_NAME_SOURCES: usize = LATE_DECEMBER[15];

// ────────────────────────────── the cases ──────────────────────────────

/// **What a case asks of the locks once a year has been judged**: which stay red,
/// which keep holding, and what the red ones say. A case that reddens a lock also
/// names the locks that hold beside it — **the positive that says the guilty
/// sitting is otherwise a good one**, without which the case passes on a sitting
/// that did nothing at all. `rest_held` asks it of every lock the case does not
/// name red.
#[derive(Default)]
struct Asks<'a> {
    red: &'a [usize],
    held: &'a [usize],
    rest_held: bool,
    says: &'a [(usize, &'a str)],
    not_says: &'a [(usize, &'a str)],
}

/// Ask `asks` of an already judged year; a failure names the lock and what the run
/// would have said.
fn check(judged: &[Outcome], asks: &Asks<'_>) {
    for &at in asks.red {
        assert!(
            !judged[at].held,
            "lock {at} held where the case expects it red: {}",
            saying(judged),
        );
    }
    for &at in asks.held {
        assert!(
            judged[at].held,
            "lock {at} failed where the case expects it to hold: {}",
            saying(judged),
        );
    }
    for at in (0..judged.len()).filter(|at| asks.rest_held && !asks.red.contains(at)) {
        assert!(
            judged[at].held,
            "lock {at} failed in a year that is meant to fail only the red ones: {}",
            saying(judged),
        );
    }
    for &(at, said) in asks.says {
        assert!(
            judged[at].saying.contains(said),
            "lock {at} does not say {said:?}: {}",
            saying(judged),
        );
    }
    for &(at, said) in asks.not_says {
        assert!(
            !judged[at].saying.contains(said),
            "lock {at} says {said:?}: {}",
            saying(judged),
        );
    }
}

/// **Work `year` in a cached room, judge every lock, and ask `asks` of them.**
async fn expect(year: Year, asks: Asks<'_>) {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let boundaries = year.work(&surface).await;
    check(&judge_all(&surface, &boundaries).await, &asks);
}

/// **The year before December, then one sitting of its own played by `play`**, and
/// the readings a run takes after it: later December is three sittings in the
/// document and the fold lock reads the window of the second, so the two
/// readings before the end are taken after the play and every window the locks
/// ask about sees its writes.
async fn expect_december(play: impl AsyncFn(&Script<'_>), asks: Asks<'_>) {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let mut boundaries = year_before_december().work(&surface).await;
    play(&sitting(&surface, "2026-12-20").await).await;
    let named = boundary_names(&room_document());
    let last = boundaries.len() - 1;
    boundaries[last - 1] = boundary(&surface, &named[last - 1]).await;
    boundaries[last] = boundary(&surface, "the end").await;
    check(&judge_all(&surface, &boundaries).await, &asks);
}

/// The readings of a room built by hand: one, taken after, and every lock judged
/// against it.
async fn judged_now(surface: &Surface) -> Vec<Outcome> {
    let now = boundary(surface, "after").await;
    judge_all(surface, &[now]).await
}

/// **Seventeen sittings, every one cold, every one claiming its day.**
///
/// The day is what the whole fiction rests on: jojobot reads no clock, so a
/// sitting that names no day is stamped with the day the run happened and the
/// year is fiction only in the prose.
#[test]
fn the_year_is_seventeen_cold_sittings_and_every_one_claims_its_day() {
    let year = room_document();
    assert_eq!(
        year.phases.len(),
        17,
        "the year carries one extra sitting, in October, and later December is three: {:?}",
        year.phases.iter().map(|p| &p.name).collect::<Vec<_>>(),
    );
    let mut days: Vec<&str> = Vec::new();
    for phase in &year.phases {
        assert!(
            phase.fresh_session,
            "{} carries the sitting before it, so the store is optional in it",
            phase.name,
        );
        let day = phase
            .day
            .as_deref()
            .unwrap_or_else(|| panic!("{} claims no day", phase.name));
        assert!(
            phase.prompt.contains(&day[..4]),
            "{} claims {day} where the harness reads and does not say a date where the occupant \
             does, so nothing carries it into a call: {:?}",
            phase.name,
            phase.prompt,
        );
        days.push(day);
    }
    let mut sorted = days.clone();
    sorted.sort_unstable();
    assert_eq!(days, sorted, "the year does not run forwards: {days:?}");
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        days.len(),
        "two sittings claim the same day, so neither generated assertion can say which one \
         wrote: {days:?}",
    );
}

/// **The worked year runs every sitting but the one a person reads**, and
/// leaves nothing out.
#[test]
fn the_worked_year_runs_every_sitting_but_the_one_a_person_reads() {
    let year = Year::worked();
    for sitting_of in Sitting::ALL {
        assert_eq!(
            year.runs(sitting_of),
            sitting_of != Sitting::ReadThis,
            "{sitting_of:?}"
        );
    }
    assert!(year.is_whole());
}

/// **A sitting left out of a year does not run, and the year is no longer whole**,
/// which is what lets it meet refusals of an absent thing.
#[test]
fn a_year_without_a_sitting_leaves_it_out_and_is_no_longer_whole() {
    let year = Year::worked().without(&[Sitting::June]);
    assert!(!year.runs(Sitting::June));
    assert!(year.runs(Sitting::July));
    assert!(!year.is_whole());
    let january = Year::only(&[Sitting::January]);
    assert!(january.runs(Sitting::January) && !january.runs(Sitting::February));
}

/// **A play runs even when its sitting is outside the set**: naming a wrong play
/// is asking for that sitting to happen wrongly.
#[test]
fn a_play_runs_even_when_its_sitting_is_left_out() {
    let year = Year::only(&[Sitting::January]).play(Play::JuneInWords);
    assert!(year.runs(Sitting::June));
    assert_eq!(year.play_at(Sitting::June), Some(Play::JuneInWords));
    assert_eq!(year.play_at(Sitting::July), None);
    assert!(!year.is_whole());
}

/// **Two plays at one sitting are refused, naming both**: they would turn two
/// locks red at once and no case could say which wrong it measured.
#[test]
#[should_panic(expected = "are both plays of June")]
fn two_plays_at_one_sitting_are_refused() {
    let _ = Year::worked()
        .play(Play::JuneInWords)
        .play(Play::JuneWritesTheTurnByHand);
}

/// **The entries name no verb the room serves.**
///
/// A room gives the player every fact the task needs and never the move.
#[tokio::test]
async fn no_entry_names_a_verb_the_room_serves() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let verbs = surface
        .tools_for_the_model()
        .await
        .expect("the room lists its verbs");
    assert!(
        verbs.len() > 5,
        "the room served {} verbs, so finding none in the entries proves nothing",
        verbs.len(),
    );
    for phase in &room_document().phases {
        for verb in &verbs {
            let name = verb["name"].as_str().expect("a verb has a name");
            assert!(
                !phase.prompt.contains(name),
                "{} names the verb {name}, so the occupant did not have to find it",
                phase.name,
            );
        }
        let said = phase.prompt.to_lowercase();
        for pointed in ["mail", "box", "message", "waiting"] {
            assert!(
                !said.contains(pointed),
                "{} points the occupant at its mail, so the first lock opens itself: {:?}",
                phase.name,
                phase.prompt,
            );
        }
    }
}

/// **The two sittings a person must read are marked, and nothing asserts over
/// them. Every other sitting is asserted over.**
///
/// Both halves. A shape landing in READ is not a failure of the format; a
/// shape landing in READ that the format then buries is one, and a sitting
/// nobody wrote a lock for that is NOT marked to be read is a sitting that
/// silently measures nothing.
#[test]
fn the_sittings_a_person_reads_are_marked_and_every_other_one_is_locked() {
    let year = room_document();
    let checks = expectations::for_playbook(expectations::YEAR_ROOM).expect("the year asserts");
    assert_eq!(
        checks.len(),
        LOCKS,
        "the year carries {} locks and the roll-call above names {LOCKS}",
        checks.len(),
    );

    let unasserted = uncovered_phases(&year, &checks);
    let read_this: Vec<&str> = year
        .phases
        .iter()
        .filter(|p| p.read_this)
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(
        read_this.len(),
        READ_THESE.len() + READ_BUT_LOCKED.len(),
        "the sittings marked to be read are {read_this:?}",
    );
    for key in READ_BUT_LOCKED {
        assert!(
            read_this.iter().any(|name| name.starts_with(key)),
            "{key} is not marked to be read: {read_this:?}",
        );
        assert!(
            !unasserted.iter().any(|name| name.starts_with(key)),
            "{key} no longer has a lock over it: {unasserted:?}",
        );
    }
    for key in READ_THESE {
        assert!(
            read_this.iter().any(|name| name.starts_with(key)),
            "{key} is not marked to be read: {read_this:?}",
        );
        assert!(
            unasserted.iter().any(|name| name.starts_with(key)),
            "{key} is marked to be read and a lock asserts over it anyway: {unasserted:?}",
        );
    }
    assert_eq!(
        unasserted.len(),
        READ_THESE.len(),
        "a sitting asserts nothing and is not marked to be read, so it measures nothing and \
         says nothing: {unasserted:?}",
    );
}

/// **A year nobody worked in fails every lock.**
///
/// The room is furnished with five nouns and a brief, so nothing any lock
/// claims is true of it — which is what makes the case that follows mean
/// something.
#[tokio::test]
async fn a_year_nobody_worked_in_fails_every_lock() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    // The readings a run takes, with nothing done between them: a check scoped
    // to one sitting must see an empty window rather than no window at all.
    let boundaries = Year::only(&[]).work(&surface).await;
    let outcomes = judge_all(&surface, &boundaries).await;
    for outcome in &outcomes {
        assert!(
            !outcome.held,
            "a furnished year nobody worked in held a lock: {}",
            saying(&outcomes),
        );
    }
}

/// **The year is solvable, and this is the case that says so.**
///
/// A different kind of case from the ones above: those prove the locks
/// discriminate between stores somebody put state into by hand. This one proves
/// every question the year asks CAN be answered through the served surface.
///
/// Without it a year whose own arithmetic is wrong would be unreachable by
/// every session that ever enters it, every case above would still pass, and
/// the failure would surface on a paid run reading as a defect in the product.
#[tokio::test]
async fn every_lock_holds_once_the_year_is_worked() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let boundaries = worked_the_year(&surface).await;
    let outcomes = judge_all(&surface, &boundaries).await;
    for outcome in &outcomes {
        assert!(
            outcome.held,
            "a year worked the way it is meant to be failed a lock: {}",
            saying(&outcomes),
        );
    }
}

/// 🚨 **The sabotage that says the year measures anything: start it in June.**
///
/// If the second half of the year reads much the same against a store nobody
/// wrote in until June, the year is measuring nothing — it is fifteen rooms
/// in a row rather than one year.
///
/// **Both halves.** The early sittings' locks fail because that work never
/// happened, and the LATE ones fail too — September, October and the sitting
/// after it ask about things February and June created, and no amount of
/// working the second half well can answer them.
///
/// **Late October is not called at all**, the same way July never is: both
/// need an address `recall` would have to find first, and neither address
/// exists in a store that skipped the sitting that wrote it. Not calling them
/// is the honest version of the same failure their locks report on their own.
#[tokio::test]
async fn a_year_that_skipped_its_first_half_cannot_answer_its_second_half() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    // June onwards, done as well as a session can do it against a store that
    // holds nothing any of it refers to.
    let boundaries = Year::only(&[
        Sitting::June,
        Sitting::August,
        Sitting::September,
        Sitting::October,
    ])
    .work(&surface)
    .await;
    let outcomes = judge_all(&surface, &boundaries).await;
    // **Nineteen of the twenty locks fail.** The one that holds is the only
    // claim in the year that rests on nothing before it — August files the
    // committee note against a council that came with the furniture.
    let stands_alone = AUGUST_STANDS_ALONE;
    for at in JANUARY
        .iter()
        .chain(&FEBRUARY)
        .chain(&MARCH)
        .chain(&APRIL)
        .chain(&MAY)
        .chain(&JUNE)
        .chain(&JULY)
        .chain(&[AUGUST_INVENTS_NOBODY])
        .chain(&SEPTEMBER)
        .chain(&OCTOBER)
        .chain(&LATE_OCTOBER)
        .chain(&LATE_NOVEMBER)
    {
        assert!(
            !outcomes[*at].held,
            "a year that began in June answered a question resting on a sitting that never \
             happened, so it is fifteen rooms in a row rather than one year: {}",
            saying(&outcomes),
        );
    }
    // The positive half: this is not a store where every write failed. The one
    // sitting that needed nothing before it did land.
    assert!(
        outcomes[stands_alone].held,
        "not one lock held, so the case above passes on a run where nothing was written at \
         all: {}",
        saying(&outcomes),
    );
}

/// **A sitting that recorded the year in prose leaves the late questions
/// unanswerable**, which is the whole reason the shapes were chosen.
///
/// Every claim is written down, in sentences, and nothing carries a key, an
/// edge or a loop. A reader could answer every question from the prose; a cold
/// session cannot.
#[tokio::test]
async fn the_locks_fail_on_a_year_written_entirely_in_prose() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    // **One sitting per DAY, not one per claim.** Two runs on the same stated
    // day are both still working in that frame, so the second meets the first
    // rather than sweeping it — which is right, and means a day gets one run.
    // **Each claim under the day its own sitting is in.** The prose year is still
    // a year: a run per day, saying nothing but sentences.
    let january = sitting(&surface, "2026-01-12").await;
    january.did("read_mailbox", json!({})).await;
    for (subject, said) in [
        (
            "person:milhouse",
            "he lives in Springfield and sits on the Springfield town council",
        ),
        (
            "thing:skateboard",
            "the bearings want looking at every ninety days, last done 2025-12-20",
        ),
    ] {
        january
            .did(
                "capture",
                json!({"subject": subject, "content": said, "provenance": "testimony"}),
            )
            .await;
    }
    let february = sitting(&surface, "2026-02-08").await;
    february
        .did(
            "capture",
            json!({"subject": "org:springfield-town-council",
                   "content": "Ralph has the skate tool on loan and Nelson has joined",
                   "provenance": "testimony"}),
        )
        .await;
    let boundaries = Year::only(&[]).work(&surface).await;
    let outcomes = judge_all(&surface, &boundaries).await;
    for at in JANUARY[1..].iter().chain(&FEBRUARY).chain(&JUNE) {
        assert!(
            !outcomes[*at].held,
            "a year written in sentences held a lock that needs a key, an edge or a loop: {}",
            saying(&outcomes),
        );
    }
    // The positive that stops the assertion above passing on a store that lost
    // everything: the brief was taken, so this session did reach the room.
    assert!(
        outcomes[BOX_OPENED].held,
        "the box was never opened, so nothing here says the session got as far as the room: {}",
        saying(&outcomes),
    );
}

/// The year's own furniture avoids every day a sitting claims, so no generated
/// date assertion can hold on the furniture alone.
#[test]
fn no_furniture_is_dated_on_a_day_a_sitting_claims() {
    let year = room_document();
    let world = std::fs::read_to_string(expectations::room_document(expectations::YEAR_ROOM))
        .expect("the room reads");
    let block: String = world
        .lines()
        .skip_while(|l| l.trim() != "```world")
        .take_while(|l| l.trim() != "```" || l.trim() == "```world")
        .collect();
    for phase in &year.phases {
        let day = phase.day.as_deref().expect("every sitting claims a day");
        assert!(
            !block.contains(day),
            "{} claims {day} and the room is furnished with a record already carrying it, so \
             that sitting's generated assertion would hold whatever it did",
            phase.name,
        );
    }
}

/// 🚨 **The year is winnable — the half a lock suite cannot show.**
///
/// A run asserts more than the document's locks: it generates one date
/// assertion per dated sitting, and its verdict also needs the room to have
/// changed. **Nothing had ever shown those satisfiable**, so a failure on the
/// first paid run would have been ambiguous between jojobot falling short and
/// the year being unwinnable by anybody.
///
/// This drives the year the way a run does — a boundary read before anything,
/// then one after each sitting, named for the sitting that comes next — and
/// asks the generated assertions the same question the run asks them.
#[tokio::test]
async fn every_assertion_a_run_makes_holds_once_the_year_is_worked() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let year = room_document();
    // The run's own reads, in the run's own order and with the run's own names,
    // taken by the one driver the lock case uses. Two copies of this order was
    // how they came to disagree about which sittings write.
    let boundaries = worked_the_year(&surface).await;

    let changed = boundaries[0].world != boundaries[boundaries.len() - 1].world;
    assert!(
        changed,
        "the room is exactly as it was furnished, so a run of this year could not report a pass",
    );

    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    // **Every dated sitting the room asserts a day over.** July takes the
    // March claim back and late October retracts a June one, both with
    // `retract` — the verb takes a date, so both carry the day the operator
    // names rather than the day the run happened. September is the
    // one sitting that writes about an earlier day, and the room says so.
    let mut missed = Vec::new();
    let mut asked = 0;
    for dated in days_claimed(&year) {
        asked += 1;
        let outcome = dated.check(&seen).await;
        if !outcome.held {
            missed.push(outcome.saying);
        }
    }
    assert!(
        missed.is_empty(),
        "a run of this year would fail {} of its generated date assertions, so it is not \
         winnable by anybody: {}",
        missed.len(),
        missed.join("\n  "),
    );
    // ⚠️ **The count comes after the failures, and the order is load-bearing.**
    // It is a calibration: it moves whenever a sitting is added or an exemption
    // comes out, so asserting it first masks the assertion underneath — the
    // case stops before saying which sitting failed and why.
    assert_eq!(
        asked, 15,
        "the year generates one assertion per dated sitting, less the two a person reads — and \
         this asked about {asked}",
    );
}

/// 🚨 **The check scoped to one sitting's window, asked of a year without that
/// sitting.**
///
/// Every other lock in this room runs once against the FINISHED room, so the
/// only question it can ask is whether something is still there at the end.
/// March asks a different question: did that sitting record anything. July takes
/// the claim March wrote back, which marks the record rather than rewriting it.
///
/// **So this reads the world either side of March.** The positive half is
/// `every_lock_holds_once_the_year_is_worked`, which holds this lock on the year
/// with March in it: a negative on its own passes identically on a run where the
/// check is broken and reports nothing.
///
/// ⚠️ **July is left out as well**, and not to be kind to it: July edits the
/// claim March writes, so a year missing March cannot run July at all. Working
/// it would fail on the missing address rather than on the window this case is
/// about.
#[tokio::test]
async fn the_march_window_says_whether_that_sitting_recorded_anything() {
    let without_march = Year::only(&[
        Sitting::January,
        Sitting::February,
        Sitting::April,
        Sitting::May,
        Sitting::June,
        Sitting::August,
        Sitting::September,
        Sitting::October,
        Sitting::LateOctober,
    ]);
    expect(
        without_march,
        Asks {
            red: &[MARCH_RECORDED],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **December's lock, discriminated against a November that never happened.**
///
/// Reads the finished board rather than a window either side of the sitting:
/// nothing after December touches the bearings loop again. Without November's
/// service the loop is owed in December, so the lock fails on the first day it
/// asks about. `every_lock_holds_once_the_year_is_worked` is this lock's positive.
#[tokio::test]
async fn decembers_lock_fails_when_novembers_service_was_never_recorded() {
    expect(
        Year::worked().without(&[Sitting::LateNovember]),
        Asks {
            red: &[BEARINGS_LOOP_STANDS],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **December's lock, discriminated against a loop done again after November.**
///
/// The year is worked as meant, and then the loop is checked in once more on
/// 12 December. Its next due day moves to 12 March, so it is not owed on
/// 20 February and the lock fails on that day. Built on the worked year so the
/// one wrong act is the only difference.
#[tokio::test]
async fn decembers_lock_fails_when_the_loop_was_done_again_after_november() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let boundaries = worked_the_year(&surface).await;
    // A later day than the worked year's last sitting (26 December), so that
    // sitting has gone quiet in this run's frame when this one boots. The check-in
    // below names its own day, so the boot day does not move the date it records.
    sitting(&surface, "2027-01-05")
        .await
        .did(
            "capture",
            json!({"subject": "rhythm:bearings-check", "content": "did the bearings again",
                   "provenance": "testimony", "check_in": "ran", "recorded_at": "2026-12-12"}),
        )
        .await;
    check(
        &judge_all(&surface, &boundaries).await,
        &Asks {
            red: &[BEARINGS_LOOP_STANDS],
            ..Default::default()
        },
    );
}

/// 🚨 **December's lock, discriminated against a loop that falls due a day early.**
///
/// The year is worked without November's service, and the loop is checked in on
/// 21 November instead. Its next due day is 19 February, so it is owed on the
/// day the lock asks about last but one, and only that day separates this year
/// from the worked one: 13 December and 20 February read the same in both.
#[tokio::test]
async fn decembers_lock_fails_when_the_loop_falls_due_a_day_early() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let boundaries = Year::worked()
        .without(&[Sitting::LateNovember])
        .work(&surface)
        .await;
    sitting(&surface, "2027-01-05")
        .await
        .did(
            "capture",
            json!({"subject": "rhythm:bearings-check", "content": "did the bearings in November",
                   "provenance": "testimony", "check_in": "ran", "recorded_at": "2026-11-21"}),
        )
        .await;
    check(
        &judge_all(&surface, &boundaries).await,
        &Asks {
            red: &[BEARINGS_LOOP_STANDS],
            ..Default::default()
        },
    );
}

/// 🚨 **December's lock reads through the grader and sends no MCP request.** The
/// server counts an MCP request as it ends and the grader's route is not one, so
/// a lock that reads through the tool dispatcher moves the count. **The lock must
/// hold on the worked year**, so the count is taken over a read that found the
/// loop and the three dates, not over a lock that read nothing.
#[tokio::test]
async fn decembers_lock_reads_through_the_grader_and_sends_no_mcp_request() {
    let (room, surface) = furnished(Furnish::Cached).await;
    let boundaries = worked_the_year(&surface).await;
    let checks = expectations::for_playbook(expectations::YEAR_ROOM).expect("the year asserts");
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let sent = || jojobot_exercise::timing::server_time(&room.server_log()).requests;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let before = sent();
    let outcome = checks[BEARINGS_LOOP_STANDS].check(&seen).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert!(
        outcome.held,
        "December's lock did not hold on the worked year: {}",
        outcome.saying
    );
    assert_eq!(
        sent(),
        before,
        "December's lock sent an MCP request while it read the room: {}",
        outcome.saying,
    );
}

/// 🚨 **The inspection's own hedge, discriminated.**
///
/// The negative omits `standing` entirely rather than naming a wrong value:
/// `capture`'s own default for testimony is `settled`, so a plain claim with
/// no standing sent is the honest shape of a session that never marked the
/// plan a hedge at all — not a contrived wrong value nobody would send.
#[tokio::test]
async fn the_inspections_hedge_lock_fails_when_nothing_is_marked_open() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let jan = sitting(&surface, "2026-01-12").await;
    stand_up(
        &jan,
        &[(
            "event",
            "gorge-safety-inspection",
            "The gorge safety inspection",
        )],
    )
    .await;
    jan.did(
        "capture",
        json!({"subject": "event:gorge-safety-inspection", "content": "planning to attend",
               "provenance": "testimony"}),
    )
    .await;
    check(
        &judged_now(&surface).await,
        &Asks {
            red: &[HEDGE_STAYS_OPEN],
            ..Default::default()
        },
    );
}

/// 🚨 **The wheels job's word lock, discriminated against no
/// correction at all.**
#[tokio::test]
async fn the_wheelss_job_lock_fails_when_nothing_corrects_the_word() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    sitting(&surface, "2026-11-22")
        .await
        .did(
            "capture",
            json!({"subject": "thing:skateboard", "content": "the shop serviced the wheels",
                   "provenance": "testimony", "sets": {"cost": "45", "settled": "sent"}}),
        )
        .await;
    check(
        &judged_now(&surface).await,
        &Asks {
            red: &[WHEELS_JOB],
            ..Default::default()
        },
    );
}

/// 🚨 **The March control lock, discriminated against a sitting that
/// repaints a job that was never wrong.**
///
/// The failure mode the control exists for: a sitting that cannot tell
/// right from wrong paints every job the same word, including the one that
/// already said what the operator's own word says.
#[tokio::test]
async fn the_marchs_job_control_fails_when_it_gets_repainted_too() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let sid = sitting(&surface, "2026-03-15").await;
    stand_up(&sid, &[LONGBOARD]).await;
    sid.did(
        "capture",
        json!({"subject": "thing:longboard", "content": "the soft spot was patched",
               "provenance": "testimony", "sets": {"cost": "30", "settled": "paid"}}),
    )
    .await;
    // **A sitting that cannot tell right from wrong**, played: it paints
    // the already-correct job the same value it gives everything else.
    sid.did(
        "capture",
        json!({"subject": "thing:longboard", "content": "the soft spot was patched",
               "provenance": "inference", "sets": {"settled": "invoiced"}}),
    )
    .await;
    check(
        &judged_now(&surface).await,
        &Asks {
            red: &[LONGBOARD_CONTROL],
            ..Default::default()
        },
    );
}

/// 🚨 **The skate tool's job word lock, discriminated against no correction
/// at all — the second off-vocabulary word, independent of the
/// wheels's.**
#[tokio::test]
async fn the_skate_tools_job_lock_fails_when_nothing_corrects_the_word() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let sid = sitting(&surface, "2026-11-22").await;
    stand_up(&sid, &[SKATE_TOOL]).await;
    sid.did(
        "capture",
        json!({"subject": "thing:skate-tool", "content": "the skate tool needed a new washer",
               "provenance": "testimony", "sets": {"cost": "20", "settled": "billed"}}),
    )
    .await;
    check(
        &judged_now(&surface).await,
        &Asks {
            red: &[SKATE_TOOL_JOB],
            ..Default::default()
        },
    );
}

/// 🚨 **The board lock's control lock, discriminated against a sitting that
/// repaints a job that was never wrong — the second control, under a word
/// neither the longboard's nor the wheels's own fix uses.**
#[tokio::test]
async fn the_board_locks_job_control_fails_when_it_gets_repainted_too() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let sid = sitting(&surface, "2026-12-20").await;
    stand_up(&sid, &[BOARD_LOCK]).await;
    sid.did(
        "capture",
        json!({"subject": "thing:board-lock", "content": "the shop waived the fitting charge",
               "provenance": "testimony", "sets": {"cost": "15", "settled": "waived"}}),
    )
    .await;
    // **A sitting that cannot tell right from wrong**, played: it paints
    // the already-correct job the same value it gives everything else.
    sid.did(
        "capture",
        json!({"subject": "thing:board-lock", "content": "the shop waived the fitting charge",
               "provenance": "inference", "sets": {"settled": "invoiced"}}),
    )
    .await;
    check(
        &judged_now(&surface).await,
        &Asks {
            red: &[BOARD_LOCK_JOB],
            ..Default::default()
        },
    );
}

/// 🚨 **The selective-sum lock, discriminated against the unconditional
/// sum — every job added regardless of its word, the wrong route a plain
/// reading of the record gives.**
#[tokio::test]
async fn the_selective_sum_lock_fails_when_the_sum_is_not_selective() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let sid = sitting(&surface, "2026-12-20").await;
    stand_up(&sid, &[LONGBOARD, SKATE_TOOL, BOARD_LOCK]).await;
    for (subject, content, cost, settled) in [
        ("thing:longboard", "the soft spot was patched", "30", "paid"),
        (
            "thing:skateboard",
            "the shop serviced the wheels",
            "45",
            "invoiced",
        ),
        (
            "thing:board-lock",
            "the shop waived the fitting charge",
            "15",
            "waived",
        ),
        (
            "thing:skate-tool",
            "the skate tool needed a new washer",
            "20",
            "invoiced",
        ),
    ] {
        sid.did(
            "capture",
            json!({"subject": subject, "content": content,
                   "provenance": "testimony", "sets": {"cost": cost, "settled": settled}}),
        )
        .await;
    }
    // **The wrong route, played**: every job added regardless of its word —
    // 30 + 45 + 15 + 20 — instead of selecting on "invoiced" first.
    sid.did(
        "capture",
        json!({"subject": "thing:skateboard", "content": "what is actually invoiced across this year's jobs",
               "provenance": "inference", "sets": {"owed": "110"}}),
    )
    .await;
    check(
        &judged_now(&surface).await,
        &Asks {
            red: &[WHAT_IS_OWED],
            ..Default::default()
        },
    );
}

/// 🚨 **The skateboard's summed kilometres, discriminated.**
///
/// Late November is the sitting excluded: without its own figure, the skateboard
/// carries June's 800 alone, never 1400 — the wrong total a plain
/// newest-write-wins fold would also produce, which is exactly the failure
/// this lock exists to catch. Its positive is
/// `every_lock_holds_once_the_year_is_worked`.
#[tokio::test]
async fn the_skateboards_summed_kilometres_fail_when_only_one_sitting_wrote_a_figure() {
    expect(
        Year::worked().without(&[Sitting::LateNovember]),
        Asks {
            red: &[DISTANCE_SUMS],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **July's window, asked both ways in one case.**
///
/// July's claim needs a positive: a July that does nothing to the council must
/// not satisfy *no retraction appeared*, which is why the check counts a
/// retraction APPEARING rather than absence. And the council gains records after
/// July — August writes on it — so a later sitting taking any OTHER council
/// claim back must not put July's name on credit it did nothing to earn.
///
/// **Both halves here, because either alone is worthless.** A July that is
/// guilty — rewrites in place instead of withdrawing — must still be caught,
/// or the narrowing produced a check that cannot fail. And a later sitting
/// retracting something else on the council must no longer reach July's window.
#[tokio::test]
async fn julys_window_catches_a_guilty_july_and_ignores_a_later_retraction() {
    // ① The accused sitting, guilty: July rewrites the March claim in place
    //    rather than taking it back, which is the wrong move now that March
    //    and July are different sittings.
    expect(
        Year::worked().play(Play::JulyRewritesInPlace),
        Asks {
            red: &[JULY_TAKES_BACK],
            ..Default::default()
        },
    )
    .await;

    // ② The year worked honestly, and then a later sitting takes a DIFFERENT
    //    council claim back — an act July's own window must not be credited for.
    let (_room, surface) = furnished(Furnish::Cached).await;
    let boundaries = worked_the_year(&surface).await;
    let committee = address_of(
        &surface,
        "org:springfield-town-council",
        "standing for election",
    )
    .await;
    // **A sitting after the year's last**, which is what makes it a later one.
    let later = sitting(&surface, "2026-12-28").await;
    later
        .did(
            "retract",
            json!({"address": committee,
                   "reason": "the operator never stood for the board and this was never so",
                   "recorded_at": "2026-12-13"}),
        )
        .await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        judged[JULY_TAKES_BACK].held,
        "a later sitting taking a different council claim back changed whether July's own lock \
         holds, so the window is not scoped to July alone: {}",
        saying(&judged),
    );
}

/// 🚨 **July's lock, re-proven once July had a second subject to write.**
///
/// A review of this room found the lock's needle counted across the whole
/// world rather than the council's own records — so a July that wrote only the
/// longboard repair and never touched the council satisfied "no retraction appeared"
/// trivially, since nobody touched the council at all. This is the case that did
/// not exist before the fix, and it is the one that proves it.
#[tokio::test]
async fn julys_lock_reddens_when_july_writes_only_the_longboard() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let mut boundaries = Year::only(&[
        Sitting::January,
        Sitting::February,
        Sitting::March,
        Sitting::April,
        Sitting::May,
        Sitting::June,
    ])
    .work(&surface)
    .await;
    let sid = sitting(&surface, "2026-07-05").await;
    july_writes_only_the_longboard(&sid).await;
    let named = boundary_names(&room_document());
    boundaries[7] = boundary(&surface, &named[7]).await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        !judged[JULY_TAKES_BACK].held,
        "a July that wrote only the longboard record — nothing on the council at all — held the council's \
         own correction lock: {}",
        saying(&judged),
    );
}

/// 🚨 **August's window, asked both ways in one case.**
///
/// August must invent nobody. **Named against the finished room, that negative
/// cannot be made to fail by August at all** — the person it named does not
/// exist in August — while a LATER sitting putting that person at the event
/// reddens August's sentence.
///
/// So the check counts the links in August's own window, and this asks it both
/// ways: an August that writes a third attendee is caught, and a later sitting
/// making that same mistake no longer lands on August.
#[tokio::test]
async fn augusts_window_catches_a_guilty_august_and_ignores_a_later_invention() {
    // ① The accused sitting, guilty: August answers with somebody who was not
    //    there rather than out of the record.
    expect(
        Year::worked().play(Play::AugustPutsAThirdPersonThere),
        Asks {
            red: &[AUGUST_INVENTS_NOBODY],
            ..Default::default()
        },
    )
    .await;

    // ② The year worked honestly, and then a LATER sitting makes that mistake.
    //    It is a fault, and it is not August's.
    let (_room, surface) = furnished(Furnish::Cached).await;
    let boundaries = worked_the_year(&surface).await;
    let inspection = handle_of_kind(&surface, "event").await;
    // **A sitting after the year's last**, which is what makes it a later one.
    let later = sitting(&surface, "2026-12-28").await;
    later
        .did(
            "capture",
            json!({"subject": "person:bart", "content": "was at the gorge safety inspection",
                   "provenance": "inference",
                   "shape": "attendance", "object": inspection}),
        )
        .await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        judged[AUGUST_INVENTS_NOBODY].held,
        "a later sitting putting somebody at the inspection reddened August, which answered \
         honestly four months earlier: {}",
        saying(&judged),
    );
}

/// 🚨 **The late sitting's own lock, asked of the sitting that invents.**
///
/// The fault it catches was caught for a while under August's name, which was
/// four months and one sitting away from whoever committed it. **Attribution
/// follows the act.** This is the first sitting handed somebody to invent and a
/// reason to be writing about the inspection, so it is where the lock belongs.
///
/// ⚠️ **The honest half is not a formality here.** This sitting takes an
/// attendance AWAY, and a check that could not tell taking one away from adding
/// one would fail the very play it is written to allow;
/// `every_lock_holds_once_the_year_is_worked` holds it on the honest late October.
#[tokio::test]
async fn late_octobers_window_catches_the_sitting_that_invents_an_attendee() {
    expect(
        Year::worked().play(Play::LateOctoberPutsAThirdPersonThere),
        Asks {
            red: &[LATE_OCTOBER_INVENTS_NOBODY],
            ..Default::default()
        },
    )
    .await;
}

/// **An October that gives the reason and never makes the move it was given
/// for.** Everything else it is asked to do is done, so a red here is about
/// the rename and nothing else; the honest year, which does rename the
/// inspection, holds the lock in `every_lock_holds_once_the_year_is_worked`.
#[tokio::test]
async fn junes_mention_lock_catches_a_inspection_that_was_never_renamed() {
    expect(
        Year::worked().play(Play::OctoberDoesNotRename),
        Asks {
            red: &[INSPECTION_RENAMED],
            held: &[WALK_TO_THE_PLACE],
            ..Default::default()
        },
    )
    .await;
}

/// **The rename's END STATE, reached without the rename.** June's claim is
/// retracted and a fresh one is written naming a different event — the same
/// shape a rename leaves on a lock that is not anchored to June's own
/// record. This lock must tell the two apart.
///
/// October's other work is still there, and the walk to the place still finds
/// it, because the fresh claim never gives a location.
#[tokio::test]
async fn junes_mention_lock_is_not_satisfied_by_a_retraction_and_a_fresh_claim() {
    expect(
        Year::worked().play(Play::OctoberRetractsJunesClaimInsteadOfRenaming),
        Asks {
            red: &[INSPECTION_RENAMED],
            held: &[WALK_TO_THE_PLACE],
            ..Default::default()
        },
    )
    .await;
}

/// **The status guard's own case: June's claim retracted, and the inspection
/// renamed for real underneath it anyway.** A retracted address's content is
/// frozen at what it said when it was written, but a mention inside it still
/// resolves to whatever its subject is called NOW — so once the inspection is
/// actually renamed, a live read of June's dead address renders the new
/// handle too, exactly as a genuine rename would leave it. Only a check that
/// first asks whether the address is still active can tell this apart from
/// one that really carried the rename across.
#[tokio::test]
async fn junes_mention_lock_is_not_satisfied_by_a_retracted_address_the_rename_also_reached() {
    expect(
        Year::worked().play(Play::OctoberRetractsJunesClaimAndRenamesAnyway),
        Asks {
            red: &[INSPECTION_RENAMED],
            held: &[WALK_TO_THE_PLACE],
            ..Default::default()
        },
    )
    .await;
}

/// **More than one of June's own addresses, all pointing at the inspection, all
/// renamed together — the shape a live paid run actually wrote.** The lock
/// must hold on every one of them, not refuse for finding more than one.
///
/// **Late October is worked, and its honest work retracts Nelson's June
/// attendance** for a reason that has nothing to do with a rename — he was
/// never at the inspection. The lock reads the end of October's rename
/// sitting, so that later retraction is not a failed rename.
#[tokio::test]
async fn junes_mention_lock_holds_when_every_one_of_junes_addresses_is_renamed_together() {
    expect(
        Year::worked().play(Play::JuneAttendanceAsMentions),
        Asks {
            held: &[INSPECTION_RENAMED],
            ..Default::default()
        },
    )
    .await;
}

/// **A second event nothing in October's reason ever touches.** One of
/// June's own addresses gets renamed along with the inspection and the other
/// never does, because it never pointed at the inspection. A rename that reached
/// only some of June's claims is worse than one that reached none, and the
/// lock must redden on it rather than pass on the address that did move.
#[tokio::test]
async fn junes_mention_lock_reddens_when_only_some_of_junes_addresses_are_renamed() {
    expect(
        Year::worked().play(Play::JuneAlsoPointsAtASecondEvent),
        Asks {
            red: &[INSPECTION_RENAMED],
            held: &[WALK_TO_THE_PLACE],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **A June that wrote WORDS where it could have written handles fails, and
/// everything else it recorded still holds.**
///
/// ⛔️ **This is the discrimination the room can make, and it is about the
/// SITTING rather than about the build.** A mention is stored as a badge and
/// served as the handle it wears today, so on a build with nothing renaming
/// anything the served text is what was typed either way — text in, same text
/// out, whether or not the layer is in the chain. **What a room measures is the
/// model**: whether a session told nothing reached for the capability. Whether
/// the layer works is the contract's question, and it is asked of both stores
/// there.
///
/// **Paired**, because a lock that only fails is a lock that could be failing
/// for any reason: the guilty June draws the same edges and checks the same loop
/// in, so its other locks hold and the one that moves is the one about handles.
/// The year that wrote the three nouns as handles holds it, in
/// `every_lock_holds_once_the_year_is_worked`.
#[tokio::test]
async fn a_june_that_wrote_words_leaves_a_later_sitting_nothing_to_follow() {
    expect(
        Year::worked().play(Play::JuneInWords),
        Asks {
            red: &[JUNE_HANDLES],
            held: &[JUNE_ATTENDEES, JUNE_BEARINGS],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **The lock about handles reads TWO KINDS ON ONE RECORD rather than a
/// named three.**
///
/// ⛔️ **It used to demand a person, a place and an event together.** Nothing in
/// the year's own story asks a sitting to name all three in one sentence, so a
/// June that wrote pointers everywhere and never that one combination failed a
/// lock about pointers. **A red that survives the fix it is asking for is a red
/// nobody trusts the next time it fires.**
///
/// **Two readings in one case**, over Junes that differ in nothing but what the
/// inspection claim points at:
///
/// * a claim pointing at a place and a person HOLDS — two kinds on one record
///   is a link between two things, whichever two they are;
/// * a claim pointing at people only FAILS — every pointer leads to the same
///   kind of thing, so no claim links one kind to another and the floor is not
///   simply *a handle was written somewhere*.
///
/// The third reading — a claim written in words — is the case above this one,
/// which is where the failure this lock exists for is already watched.
#[tokio::test]
async fn the_handle_lock_reads_two_kinds_on_one_record_rather_than_a_named_three() {
    expect(
        Year::worked().play(Play::JuneWithoutTheEvent),
        Asks {
            held: &[JUNE_HANDLES],
            ..Default::default()
        },
    )
    .await;
    expect(
        Year::worked().play(Play::JunePointingAtOneKind),
        Asks {
            red: &[JUNE_HANDLES],
            held: &[JUNE_ATTENDEES, JUNE_BEARINGS],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **An October that recorded the note and never said WHERE fails, and
/// everything else it recorded still holds.**
///
/// The operator asks where the inspection was held and never says. **June's claim
/// is the one record in the year that answers it**, so a sitting that never
/// read it has the note and not the place.
///
/// ⛔️ **Whether the sitting FOLLOWED the mention is not what moves this.**
/// Following leaves no trace: a sitting that read the claim and one that
/// guessed leave the same store, which is what this room already says about
/// August's walk. **What moves it is the write.**
///
/// **Paired.** The guilty October files the note on the event and records the
/// skate tool exactly as the good one does, so its other lock holds and the one
/// that moves is the walk to the place.
#[tokio::test]
async fn an_october_that_never_said_where_leaves_the_inspection_unplaceable() {
    expect(
        Year::worked().play(Play::OctoberFilesTheNoteOnTheEvent),
        Asks {
            red: &[WALK_TO_THE_PLACE],
            held: &[SKATE_TOOL_CORRECTED],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **The walk to the place holds whatever the event is actually called.**
///
/// A paid run named the event `event:gorge-safety-inspection-2026` — a reasonable slug
/// nothing in the year rules out. The scripted sittings above always use the
/// one slug this file's own `january` picks, so they cannot catch a lock
/// that secretly depends on it — this test picks a different one on purpose.
///
/// **No subject is pinned in the hatch this proves**, for the same reason:
/// `has_standing_edge` is asked with `None`, so it is the object alone —
/// `place:daredevil-gorge` — that has to carry the claim, whatever handle the
/// event wears.
#[tokio::test]
async fn the_walk_to_the_place_holds_whatever_the_event_is_actually_called() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let sid = sitting(&surface, "2026-10-11").await;
    let before = boundary(&surface, "Phase 10 — a differently slugged event").await;
    sid.did("add_entity",
        json!({"kind": "event", "handle": "gorge-safety-inspection-2026", "name": "The gorge safety inspection",
               "source": "the operator"}),
    )
    .await;
    sid.did(
        "capture",
        json!({"subject": "event:gorge-safety-inspection-2026",
               "content": "the ground needs a look before next year",
               "provenance": "testimony",
               "shape": "location", "object": "place:daredevil-gorge"}),
    )
    .await;
    let after = boundary(&surface, "the end").await;

    let judged = judge_all(&surface, &[before, after]).await;
    assert!(
        judged[WALK_TO_THE_PLACE].held,
        "the walk to the place failed against an event named something other than \
         event:gorge-safety-inspection: {}",
        saying(&judged),
    );
}

/// 🚨 **The year's turns are CHECKED IN rather than written by hand.**
///
/// The two are indistinguishable to every lock that reads the day, and that is
/// what let the room route around the verb for as long as it did. **A check-in
/// computes a schedule and stores it beside the claim, so the record is a
/// derivation; setting the key by hand writes the same day and nothing else.**
///
/// **The guilty play is the play this room used to have.** It stays in the
/// suite as the thing that must fail, so the route cannot quietly come back.
///
/// ⛔️ **Furnished live, not from the cache**, and the argument is where a
/// reader sees it: a copy of the cached year reads like a live furnish
/// (`the_cached_year_reads_the_same_as_a_live_furnish`), so the choice is the
/// case's own.
#[tokio::test]
async fn the_years_turns_are_checked_in_rather_than_set_by_hand() {
    let (_room, surface) = furnished(Furnish::Live).await;
    let by_hand = Year::worked()
        .play(Play::JuneWritesTheTurnByHand)
        .play(Play::LateNovemberWritesTheTurnByHand)
        .work(&surface)
        .await;
    let judged = judge_all(&surface, &by_hand).await;
    check(
        &judged,
        &Asks {
            red: &[TURNS_ARE_DERIVATIONS],
            ..Default::default()
        },
    );
}

/// **The cached year reads the same as a live furnish, on what a sitting can
/// actually find** — the proof that [`filed_year_seed`] is not quietly
/// thinner than what [`furnished_live`] gives every sitting. Checked on an
/// entity from the room's opening furniture and on the operator's own
/// opening words, which is the LAST write the furnish makes and the one
/// most likely to go missing if the cache ever stopped matching it.
#[tokio::test]
async fn the_cached_year_reads_the_same_as_a_live_furnish() {
    let (_live_room, live) = furnished(Furnish::Live).await;
    let (_cached_room, cached) = furnished(Furnish::Cached).await;

    for (surface, label) in [(&live, "live"), (&cached, "cached")] {
        let recall = surface
            .call("recall", json!({"subject": "org:springfield-town-council"}))
            .await;
        let read: Value = serde_json::from_str(&recall).expect("a JSON read");
        assert_eq!(
            read["objects"][0]["id"], "org:springfield-town-council",
            "{label} room: the council from the opening furniture is not there: {recall}"
        );
    }

    // **The snippet, never the whole answer.** Each room is its own server
    // run, so a message's `id` and `sent_at` are never going to agree
    // between one and the other — that is expected of two independent
    // furnishes, not a defect. The snippet and subject are the wording the
    // furnish actually wrote, which is what this test is proving matches.
    async fn opening_message_snippet(surface: &Surface, label: &str) -> (Value, Value) {
        let found = surface
            .read(
                "search",
                json!({"query": "invoiced, paid, or waived", "include_mail": true}),
            )
            .await;
        let read: Value = serde_json::from_str(&found).expect("a JSON read");
        let hit = read["results"].as_array().and_then(|hits| hits.first());
        let hit = hit.unwrap_or_else(|| {
            panic!("{label} room: the operator's opening message is not findable: {found}")
        });
        (hit["subject"].clone(), hit["snippet"].clone())
    }
    let live_snippet = opening_message_snippet(&live, "live").await;
    let cached_snippet = opening_message_snippet(&cached, "cached").await;
    assert_eq!(
        live_snippet, cached_snippet,
        "the operator's opening words — the furnish's own last write — read differently \
         between a live furnish and the cache"
    );
}

/// 🚨 **The derivations lock is scoped to January's own loop, and a second
/// loop cannot drag it down.**
///
/// A universal that named no loop would fail the moment ANY rhythm anywhere
/// carried a hand-set turn — including one January never opened and this
/// year's story never asked about. **This is what gives that scoping
/// something to be wrong about**: January's loop is left entirely clean here,
/// and a second, unrelated loop carries the only bad turn in the store.
///
/// ⭐ **This case is the fixture half of the proof, and it is not the whole
/// proof.** It passes today whether the lock is scoped correctly or not,
/// because nothing in this suite widens the measurement back to every
/// rhythm — that half is watched with `scripts/sabotage` against
/// `the_years_turns_are_on_file_as_derivations` in `src/checks.rs`, which
/// must turn this case red.
#[tokio::test]
async fn the_derivations_lock_is_scoped_to_januarys_loop_and_not_dragged_down_by_a_second() {
    expect(
        Year::worked().play(Play::LateNovemberStandsUpASecondLoop),
        Asks {
            red: &[LATE_NOVEMBER_TURN],
            held: &[TURNS_ARE_DERIVATIONS],
            ..Default::default()
        },
    )
    .await;
}

/// ⚠️ **The locks that read the rhythm still DISCRIMINATE now the play changed.**
///
/// A check-in writes more than the key it replaces, so a lock that was
/// measuring the sitting could start being satisfied by the machinery beside
/// it. **Each of the two is asked of a year missing the sitting it names**, and
/// June's year is asked both of the bearings lock and of the attendee lock, which
/// a June that never ran cannot hold either.
///
/// **This is the half a green suite cannot stand in for.** Every lock held the
/// moment the play changed, which says nothing about whether they still fail
/// for their own reasons.
///
/// ⚠️ **Late October is left out of June's year as well, and not to be kind to
/// it.** It retracts a claim June writes, so a year missing June cannot run it at
/// all — the play fails on the missing address rather than on the locks this case
/// is about. March's window case leaves July out for the same reason.
#[tokio::test]
async fn the_rhythm_locks_still_fail_when_the_sitting_they_name_does_nothing() {
    expect(
        year_before_december().without(&[Sitting::June, Sitting::LateOctober]),
        Asks {
            red: &[JUNE_BEARINGS, JUNE_ATTENDEES],
            ..Default::default()
        },
    )
    .await;
    expect(
        year_before_december().without(&[Sitting::LateNovember]),
        Asks {
            red: &[LATE_NOVEMBER_TURN],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **A retracted account does not stand, and `carries` could not see that.**
///
/// A paid run had October retract September's account of the skate tool rather than
/// leave it beside Nelson's — a sitting picking a winner, which is exactly the
/// thing the lock beside it exists to catch. **The old lock held anyway**: a
/// retraction is marked rather than filtered, so `recall` served the retracted
/// record's own text back, edge and all, and a substring assertion cannot see
/// the `status` key that would have told the two apart.
#[tokio::test]
async fn a_retracted_account_does_not_satisfy_the_skate_tool_lock() {
    expect(
        Year::worked().play(Play::OctoberRemovesOneOfTheTwoAccounts),
        Asks {
            red: &[SKATE_TOOL_CORRECTED],
            says: &[(SKATE_TOOL_CORRECTED, "picked a winner")],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **An archive with no correction derived from it does not satisfy the skate tool
/// lock.** The product's route for correcting words an earlier session wrote is
/// to archive the claim AND capture the correction with `derived_from` naming
/// it. A sitting that does the first half and files Nelson's account with no
/// source has removed one account and put another beside it that nothing links
/// to the first.
#[tokio::test]
async fn an_archive_with_no_correction_derived_from_it_does_not_satisfy_the_skate_tool_lock() {
    expect(
        Year::worked().play(Play::OctoberArchivesAndNamesNothing),
        Asks {
            red: &[SKATE_TOOL_CORRECTED],
            held: &[WALK_TO_THE_PLACE],
            says: &[(SKATE_TOOL_CORRECTED, "derived_from")],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **A second claim beside September's does not correct it, and the
/// operator's ruling is what makes this a failure now.**
///
/// Before the ruling, filing Nelson's account as a fresh claim was the right
/// move — jojobot infers nothing, so two accounts that disagree were both
/// left to stand. The ruling makes October's account a correction of
/// September's, so a sitting that files it as a second claim leaves
/// September's own address unrevised — exactly the record the lock now
/// reads. The location note is filed exactly as the honest October files it, so
/// the walk-to-place lock still holds and this case measures only the skate tool
/// lock.
#[tokio::test]
async fn a_second_claim_beside_septembers_does_not_satisfy_the_skate_tool_lock() {
    expect(
        Year::worked().play(Play::OctoberCapturesASecondAccount),
        Asks {
            red: &[SKATE_TOOL_CORRECTED],
            held: &[WALK_TO_THE_PLACE],
            says: &[(SKATE_TOOL_CORRECTED, "second claim")],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **September's account naming Ralph in prose, not by edge, still counts
/// as corrected.**
///
/// A paid run (run 20) wrote September this way: an `attendance` edge at the
/// event, Ralph named inside the sentence. The skate tool lock's history check used
/// to look only at the edge, so it read the earlier wording as destroyed
/// rather than superseded — the model behaved exactly as ruled and the lock
/// convicted it anyway.
#[tokio::test]
async fn septembers_prose_account_of_ralph_still_satisfies_the_skate_tool_lock() {
    expect(
        Year::worked().play(Play::SeptemberNamesRalphInProse),
        Asks {
            held: &[SKATE_TOOL_CORRECTED],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **September naming nobody at all is the one shape the skate tool lock must
/// still redden on.**
///
/// The fix for the prose-shaped account above must not become "accept
/// anything": a correction whose earlier wording genuinely never named Ralph
/// has nothing for history to carry, and the lock has to say so.
#[tokio::test]
async fn septembers_account_that_names_nobody_still_fails_the_skate_tool_lock() {
    expect(
        Year::worked().play(Play::SeptemberNamesNobody),
        Asks {
            red: &[SKATE_TOOL_CORRECTED],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **September's own lock stops speaking as though it watched September,
/// because it cannot: a Query lock reads the finished board.**
///
/// Run 20 had October legitimately clear the skate tool's day — "over the summer"
/// borrows no precision from "at the inspection" — and September's own lock
/// convicted September for it, saying the question was "still open" when it
/// had been answered and then correctly revised. The lock's assertions are
/// unchanged: a year where October clears the day still fails this lock,
/// because nothing on the skate tool currently carries it. What changed is that the
/// sentence no longer names a month or claims a question was never answered.
/// The year worked properly, with nothing clearing the day, holds the lock in
/// `every_lock_holds_once_the_year_is_worked`.
#[tokio::test]
async fn septembers_lock_does_not_blame_september_for_octobers_legitimate_edit() {
    expect(
        Year::worked().play(Play::OctoberAlsoClearsTheDay),
        Asks {
            red: &[SKATE_TOOL_DAY],
            not_says: &[
                (SKATE_TOOL_DAY, "September:"),
                (SKATE_TOOL_DAY, "still open"),
            ],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **The trace locks, asked both ways.**
///
/// **The first is a planted answer**: what the claim used to say is on no read
/// of current truth, so a sitting that answers from the claim as it stands
/// writes the opposite. **The second is the pairing that carries the weight** —
/// a lock that only asks whether a trace is THERE holds identically against a
/// read that hands a chain back for everything, and a chain on a claim nobody
/// touched says jojobot changed its mind when it did not.
///
/// ⚠️ **The pairing's failing half is NOT asserted here, and it cannot be.** A
/// play can only make writes that really happened, so every play produces a
/// trace agreeing with the boundaries. The failure is a product fault — a
/// trace reporting a write nobody made — and it is watched by breaking the
/// trace under `scripts/sabotage`, never by driving the year differently. What
/// this case holds is that a legitimate correction passes, which is the half a
/// play can reach.
#[tokio::test]
async fn the_trace_locks_tell_a_correction_from_a_claim_nobody_touched() {
    expect(
        Year::worked().play(Play::LaterDecemberAnswersFromTheClaimAsItStands),
        Asks {
            red: &[WAS_WORDING],
            ..Default::default()
        },
    )
    .await;

    // ⛔️ **A legitimate correction PASSES, and that is the fix.** The occupant
    // owns this record and may rewrite it; the trace then truthfully reports
    // two writes, and two writes is what the run made. A lock that failed here
    // was measuring the model rather than the product.
    expect(
        Year::worked().play(Play::DecemberCorrectsAClaimNobodyQuestioned),
        Asks {
            held: &[TRACE_AGREES],
            ..Default::default()
        },
    )
    .await;
}

/// **A later December that captures the board lock's mistake and never
/// corrects it.** One write, and nothing behind it — the shape
/// [`the_board_locks_mistake_is_rewritten_in_place`]'s neighbour lock in the
/// checks module exists to catch, told apart from a genuine same-breath
/// correction.
async fn later_december_never_corrects_the_board_lock(s: &Script<'_>) {
    s.did(
        "add_entity",
        json!({"kind": "thing", "handle": "board-lock", "name": "The Board Lock",
               "source": "the operator"}),
    )
    .await;
    s.did(
        "capture",
        json!({"subject": "thing:board-lock", "content": "bought a U-lock for the skateboard",
               "provenance": "testimony", "recorded_at": "2026-12-13"}),
    )
    .await;
}

/// **A later December that corrects the board lock by retracting and
/// recapturing instead of rewriting it in place.** The retraction takes the
/// old record out of the active count, so what this trips is the OTHER
/// guard: the fresh record's own trace carries one write, not two, because
/// it is a different address from the one the mistake was first written to.
async fn later_december_retracts_and_recaptures_the_board_lock(s: &Script<'_>) {
    s.did(
        "add_entity",
        json!({"kind": "thing", "handle": "board-lock", "name": "The Board Lock",
               "source": "the operator"}),
    )
    .await;
    let captured = s
        .did(
            "capture",
            json!({"subject": "thing:board-lock", "content": "bought a U-lock for the skateboard",
               "provenance": "testimony", "recorded_at": "2026-12-13"}),
        )
        .await;
    let address = captured_address(&captured);
    s.did(
        "retract",
        json!({"address": address, "reason": "wrong kind of lock"}),
    )
    .await;
    s.did(
        "capture",
        json!({"subject": "thing:board-lock", "content": "bought a cable lock for the skateboard",
               "provenance": "testimony", "recorded_at": "2026-12-13"}),
    )
    .await;
}

/// **A later December that captures a second, separate claim about the skateboard
/// lock instead of rewriting or retracting the first.** Both records stand,
/// active, side by side — the shape that actually trips the "one active
/// record" guard rather than the trace-length one: a rewrite's shape from a
/// distance without even a retraction to explain the first record away.
async fn later_december_captures_a_second_claim_beside_the_first(s: &Script<'_>) {
    s.did(
        "add_entity",
        json!({"kind": "thing", "handle": "board-lock", "name": "The Board Lock",
               "source": "the operator"}),
    )
    .await;
    s.did(
        "capture",
        json!({"subject": "thing:board-lock", "content": "bought a U-lock for the skateboard",
               "provenance": "testimony", "recorded_at": "2026-12-13"}),
    )
    .await;
    s.did(
        "capture",
        json!({"subject": "thing:board-lock", "content": "bought a cable lock for the skateboard",
               "provenance": "testimony", "recorded_at": "2026-12-13"}),
    )
    .await;
}

/// 🚨 **The board lock's rewrite lock, asked three wrong ways.**
///
/// None of the three wrong shapes is a hatch that only reads the "was"
/// note's own field — all three leave the note itself unwritten, so this is
/// a different question from
/// [`the_trace_locks_tell_a_correction_from_a_claim_nobody_touched`]'s.
/// **Three ways to fail it, because checking only one would leave the
/// others unmeasured**: one write and nothing behind it; a retraction beside
/// a fresh claim, which trips the trace-length guard because the fresh
/// claim is a different address with one write of its own; and two active
/// records with neither retracted, which trips the one-active-record guard
/// instead. The positive it rests on is the worked year in
/// `every_lock_holds_once_the_year_is_worked`.
#[tokio::test]
async fn the_board_locks_rewrite_lock_reddens_on_one_write_or_two_records() {
    let asks = || Asks {
        red: &[BOARD_LOCK_REWRITTEN],
        ..Default::default()
    };
    expect_december(later_december_never_corrects_the_board_lock, asks()).await;
    expect_december(
        later_december_retracts_and_recaptures_the_board_lock,
        asks(),
    )
    .await;
    expect_december(
        later_december_captures_a_second_claim_beside_the_first,
        asks(),
    )
    .await;
}

/// 🚨 **The rewrite lock reads the trace's own REPORTED TOTAL, never the
/// length of whatever a read happened to ship.**
///
/// A `history_record` read ships at most `history_most` writes and reports
/// the true count beside them — the same distinction
/// [`a_records_trace_matches_the_writes_the_run_made`]'s own doc names for
/// its neighbour lock: a long history comes back elided, and measuring the
/// shipped list reads an elision as a missing write. **The board lock's own
/// correction genuinely carries two writes**, proven here by an independent,
/// uncut read before the lock is ever asked — so a failure below is about
/// the lock's own read rather than about whether the correction happened.
#[tokio::test]
async fn the_board_locks_rewrite_lock_holds_on_a_record_whose_trace_read_is_cut() {
    expect_december(
        async |s| {
            let address = board_lock_corrected_in_the_same_breath(s).await;
            let full = s
                .surface
                .call(
                    "recall",
                    json!({"subject": "thing:board-lock", "history_record": address}),
                )
                .await;
            let parsed_full: Value = serde_json::from_str(&full).expect("the trace is json");
            assert_eq!(
                parsed_full["objects"][0]["record_history"]["count"], 2,
                "the board lock's own same-breath correction should carry two writes: {full}",
            );
        },
        Asks {
            held: &[BOARD_LOCK_REWRITTEN],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **February's lock is satisfiable only by what February recorded.**
///
/// The needle it used to carry was ambiguous rather than wrong: September draws
/// a second link at the same person on the same subject when the skate tool
/// comes back. **A February that recorded nothing held the lock on September's
/// work**, and two read-based sweeps of that class missed it because the needle
/// matches — it just also matches something else. The February that recorded who
/// had the skate tool holds it in `every_lock_holds_once_the_year_is_worked`.
#[tokio::test]
async fn februarys_lock_cannot_be_satisfied_by_the_sitting_that_returns_the_skate_tool() {
    expect(
        Year::worked().play(Play::FebruaryRecordsNoHolder),
        Asks {
            red: &[SKATE_TOOL_HOLDER],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **June's lock holds however the inspection happens to be spelled.**
///
/// The old lock pinned `event:gorge-safety-inspection` as the object, hardcoded — safe
/// only because the reference transcript here happens to spell it that way.
/// Nothing tells a model what to call the event it just invented, so a June
/// that draws its attendee edges at a differently spelled handle has done
/// the same work. `event:gorge-safety-inspection` itself still exists, from January,
/// untouched and unused — proving the lock is not merely failing open on a
/// missing object.
#[tokio::test]
async fn junes_attendee_lock_holds_whatever_the_inspection_is_spelled() {
    expect(
        Year::worked().play(Play::JunePointsAtADifferentlySpelledInspection),
        Asks {
            held: &[JUNE_ATTENDEES],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **The window-scoped locks fail when their own sitting's window drew
/// nothing.**
///
/// The negative half of the pattern each of these needs: a year that worked only
/// January drew no membership edge for February, no location edge to Shelbyville
/// for April, none to the gorge for October and none for Bart in late October,
/// and each lock must say so rather than hold over an empty record. The positive
/// half is `every_lock_holds_once_the_year_is_worked`, which holds all four.
/// June's attendee lock, which reads June's own window, is asked the same of a
/// year that skipped June in `the_rhythm_locks_still_fail_when_the_sitting_they_name_does_nothing`.
#[tokio::test]
async fn februarys_council_lock_fails_when_februarys_own_window_drew_nothing() {
    expect(
        Year::only(&[Sitting::January]),
        Asks {
            red: &[
                COUNCIL_MEMBERSHIP,
                SHELBYVILLE,
                WALK_TO_THE_PLACE,
                BART_JOINS,
            ],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **A retraction reads as archived, and carries its own retraction
/// pointer — the two things January's and October's own locks now check
/// instead of a status word that used to tell retraction from supersession
/// apart.**
///
/// Superseded and retracted collapsed into one `archived` status: both leave
/// a record marked rather than filtered, and nothing downstream needs to
/// tell them apart by status any more. What still tells them apart is
/// structural rather than a status word — only [`retract`] ever writes a
/// companion record naming the address it took back, under `retracts`. This
/// proves both halves directly: an archived record still carries its own
/// edge, reads `"status":"archived"` rather than the retired `"superseded"`
/// or `"retracted"` tokens, and — only for a genuine retraction — a
/// `"retracts":` pointer to its own address is somewhere on the same
/// subject. October's Nelson lock now checks FOR that pointer; January's and
/// April's Springfield lock checks its ABSENCE, because Milhouse's move is a
/// supersession and must not read as a retraction it never was.
#[tokio::test]
async fn a_retracted_claim_reads_as_archived_and_carries_its_own_retraction_pointer() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let sid = sitting(&surface, "2026-01-12").await;
    sid.did(
        "capture",
        json!({"subject": "person:milhouse", "content": "lives in Springfield",
               "provenance": "testimony",
               "shape": "location", "object": "place:springfield"}),
    )
    .await;
    let address = address_of(&surface, "person:milhouse", "Springfield").await;
    sid.did(
        "retract",
        json!({"address": address, "reason": "test probe", "recorded_at": "2026-01-12"}),
    )
    .await;
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:milhouse", "facts": true}),
        )
        .await;
    assert!(
        read.contains("place:springfield"),
        "a retracted record still carries its own edge, marked rather than filtered: {read}",
    );
    assert!(
        !read.contains("\"status\":\"superseded\"") && !read.contains("\"status\":\"retracted\""),
        "a retired status token is back on the wire, so a reader watching for the CURRENT \
         spelling would miss this record entirely: {read}",
    );
    assert!(
        read.contains("\"status\":\"archived\""),
        "…and it has to say what it actually is now: {read}",
    );
    assert!(
        read.contains(&format!("\"retracts\":\"{address}\"")),
        "the retraction left no pointer naming the address it took back, so nothing on this \
         subject can tell this archival apart from an ordinary supersession: {read}",
    );
}

/// 🚨 **No lock in this room rests on a needle that matches somewhere else.**
///
/// A needle is a substring of the answer as text. **A lock satisfied by the
/// wrong match holds while measuring nothing**, and a reader cannot see it:
/// the needle matches what it names, it just also matches something else.
/// **Two sweeps of that class read past February's, and a walk over the answer
/// found it in one pass.**
///
/// ⭐ **An ambiguous needle is not a fault on its own** — it is harmless when
/// its lock carries another needle only the sitting it names could satisfy.
///
/// ⚠️ **The count is asserted as well as the findings, and that is the half
/// that calibrates the walk.** A first version read every match twice, once at
/// the key/value pair and once at the string under it, and reported fourteen
/// where a hand-check found three.
///
/// **The count moved to zero once October stopped capturing a second, live
/// `person:ralph` beside September's.** `carries person:ralph` used to match
/// February's own record and September's both, because October's old play
/// left September's account active beside Nelson's rather than correcting it.
/// A correction in place folds September's address to Nelson's current
/// account, so only February's own mention of Ralph remains.
#[tokio::test]
async fn no_lock_here_rests_on_a_needle_that_matches_somewhere_else() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let _ = worked_the_year(&surface).await;
    let summary = jojobot_exercise::lock::needle_summary(
        &surface,
        &jojobot_exercise::lock::locks_of(expectations::YEAR_ROOM),
        &[],
    )
    .await;
    // ⚠️ **The findings come first, and the order is load-bearing.** The
    // calibration below fires on any change to what the walk sees, so asserting
    // it first masks the finding underneath: a planted ambiguous needle trips
    // the count and the case never reaches the sentence that names the lock.
    // **A red that names the wrong thing is a red nobody can act on.**
    assert!(
        summary.findings.is_empty(),
        "a lock rests on a needle that matches somewhere else, with nothing else in that lock \
         only its own sitting could satisfy: {:?}",
        summary.findings,
    );
    assert!(
        summary.nowhere.is_empty(),
        "a needle matched nowhere, so either its lock is failing or the walk could not read the \
         answer — and those are different: {:?}",
        summary.nowhere,
    );
    assert_eq!(
        summary.ambiguous, 0,
        "the walk sees a different number of ambiguous needles than the hand-check did, so it is \
         reading the answer differently",
    );
}

/// ⛔️ **No lock in this room names the loop's handle.**
///
/// The room says it four lines above the sitting that opens the loop: January
/// invents the name, so no lock may name it. **A lock did**, and a run whose
/// January called the loop something else failed for the name rather than for
/// the claim it was asked about.
///
/// ⭐ **The rule was written down and broken anyway**, so it is asked here
/// rather than left to a reader of the document.
#[test]
fn no_lock_names_the_loop_the_occupant_invents() {
    let locks = jojobot_exercise::lock::locks_of(expectations::YEAR_ROOM);
    let named: Vec<&str> = locks
        .iter()
        .filter_map(|lock| lock.asked())
        .filter(|(_, args)| args.contains("rhythm:"))
        .map(|(_, args)| args)
        .collect();
    assert!(
        named.is_empty(),
        "a lock selects the loop by a handle the occupant invents, so a run that named it \
         anything else fails here for the name rather than for the claim: {named:?}",
    );
}

/// 🚨 **A sitting that says MORE than the minimum still satisfies the trace
/// lock.**
///
/// The needle used to demand the value BE the old wording. A real sitting wrote
/// the old wording inside a fuller sentence — when it was recorded, when it was
/// put right — and was scored as not having answered.
///
/// **Paired, and the pairing is the whole risk here:** pinning the key rather
/// than the value must not become pinning nothing. **A sitting that answers
/// from the claim as it stands still fails.**
#[tokio::test]
async fn a_fuller_answer_satisfies_the_trace_lock_and_a_wrong_one_still_does_not() {
    // **The year without its last sitting**, then this sitting played by hand.
    // The lock reads the finished room, so a write after the boundaries are
    // taken is the same to it.
    expect_december(
        later_december_writes_a_fuller_sentence,
        Asks {
            held: &[WAS_WORDING],
            ..Default::default()
        },
    )
    .await;
    expect(
        Year::worked().play(Play::LaterDecemberAnswersFromTheClaimAsItStands),
        Asks {
            red: &[WAS_WORDING],
            ..Default::default()
        },
    )
    .await;
}

/// ⚠️ **The claim the pairing reads is one the year never writes twice.**
///
/// A record's address is handed out in write order, which the occupant
/// controls, so an address only means the same claim on a subject carrying
/// ONE. **The subject this lock used to name gains a scripted second write** —
/// April supersedes a claim on it — so the room guaranteed the answer the lock
/// calls a fault.
#[tokio::test]
async fn the_pairings_claim_is_one_the_year_writes_once_and_the_old_one_was_not() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let _ = worked_the_year(&surface).await;
    let writes = async |subject: &str| -> usize {
        let read = surface
            .call("recall", json!({"subject": subject, "facts": true}))
            .await;
        let parsed: Value = serde_json::from_str(&read).expect("json");
        parsed["objects"][0]["facts"]
            .as_array()
            .map(|facts| facts.len())
            .unwrap_or(0)
    };
    assert_eq!(
        writes("person:bart").await,
        1,
        "the subject this lock reads gained more than the one claim its sitting writes, so its \
         address no longer means what the lock means by it",
    );
    assert!(
        writes("person:milhouse").await > 1,
        "the subject the lock used to read carries one claim, so the address it named was safe \
         after all and this case is measuring nothing",
    );
}

/// 🚨 **September is graded like every other sitting now, and it still catches
/// a sitting in the wrong day.**
///
/// The room used to exempt it by hand: it records something true of June while
/// sitting in September, and one column carried both meanings, so its own day
/// never reached the record. **The split ended that** — the day it writes under
/// and the day the thing happened are different fields — **and the exemption
/// was dead before this removed it.**
///
/// ⛔️ **Which is exactly why the pairing matters.** A marker removed because
/// the check can no longer catch anything looks identical to a marker removed
/// because the check no longer needs it.
#[tokio::test]
async fn september_is_graded_and_a_sitting_in_the_wrong_day_still_fails() {
    let year = room_document();
    let (_room, surface) = furnished(Furnish::Cached).await;
    let confused = Year::worked()
        .play(Play::SeptemberSitsInTheWrongDay)
        .work(&surface)
        .await;
    let seen = Observed {
        room: &surface,
        boundaries: &confused,
    };
    let mut missed = Vec::new();
    for dated in days_claimed(&year) {
        let outcome = dated.check(&seen).await;
        if !outcome.held && outcome.applies {
            missed.push(outcome.name.clone());
        }
    }
    assert!(
        missed.iter().any(|name| name.starts_with("Phase 9")),
        "a September that wrote under June was not caught, so taking its exemption out left a \
         check with nothing to catch: {missed:?}",
    );
}

/// 🚨 **The fold-timing lock, asked of a December that folds nothing.**
///
/// A December that answers the council's question and never marks anything on
/// the longboard must fail the lock that exists to catch exactly that. A real
/// fold passes it, in `every_lock_holds_once_the_year_is_worked`.
#[tokio::test]
async fn the_longboard_fold_lock_reddens_when_december_never_folds_anything() {
    expect_december(
        board_lock_was,
        Asks {
            red: &[FOLD_APPEARS],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **The sources lock, asked both ways.**
///
/// A fold that names its five sources honestly and then retracts one of them
/// must fail — synthesis layers and never discards (rule 263) — while the
/// fold-timing lock and the fabrication lock, which are about different
/// questions, still hold: a real mark was made, in the window, inventing
/// nothing.
#[tokio::test]
async fn the_longboards_sources_lock_reddens_when_a_fold_destroys_one_of_them() {
    expect_december(
        later_december_folds_but_loses_a_source,
        Asks {
            red: &[FIVE_REPAIRS_STAY],
            held: &[FOLD_APPEARS, NO_INVENTED_DATE],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **The fabrication lock, asked both ways.**
///
/// A fold that names every source honestly and then claims a single day for
/// a pile that spans half the year must fail — while the timing lock and the
/// sources lock, about different questions, still hold: a real mark was
/// made, in the window, over five untouched sources.
#[tokio::test]
async fn the_longboards_fabrication_lock_reddens_when_the_fold_invents_a_date() {
    expect_december(
        later_december_folds_with_an_invented_date,
        Asks {
            red: &[NO_INVENTED_DATE],
            held: &[FOLD_APPEARS, FIVE_REPAIRS_STAY],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **The fabrication lock reddens on an invented date in the fold's own
/// PROSE, not only in a structured field.**
///
/// The case above plants its invented date under `happened_at`, a value
/// nobody reads for its own sake. A fold summarizing a year in words is at
/// least as likely to state a day in the sentence itself — the check's own
/// comment names prose as where a fabrication most naturally lands — and
/// until this case, nothing had ever driven that branch of the scanner.
/// Replacing the scanner's body with an empty list must still turn this red.
#[tokio::test]
async fn the_fabrication_lock_reddens_on_an_invented_date_in_the_folds_own_prose() {
    expect_december(
        later_december_folds_with_an_invented_date_in_prose,
        Asks {
            red: &[NO_INVENTED_DATE],
            held: &[FOLD_APPEARS, FIVE_REPAIRS_STAY],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **The fabrication lock must not convict a fold for naming the day it
/// was itself written on.** A review found the check once built its known
/// dates from every OTHER longboard record, excluding the fold — so a fold
/// stating its own honest write day in its own prose read as inventing a
/// date nobody gave.
#[tokio::test]
async fn the_fabrication_lock_does_not_convict_a_fold_for_naming_its_own_write_day() {
    expect_december(
        async |s| {
            board_lock_was(s).await;
            fold_the_longboards_pile_naming_its_own_day(s).await;
        },
        Asks {
            held: &[NO_INVENTED_DATE],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **The sources lock must check EVERY record on a day, not the first one
/// `recorded_at` happens to report.** A review found that February plausibly
/// writes two longboard facts on the same date — an acquisition and a defect —
/// and a fold missing one of a same-day pair used to read as complete.
#[tokio::test]
async fn the_sources_lock_catches_a_fold_that_misses_one_of_two_same_day_repairs() {
    expect_december(
        async |s| {
            board_lock_was(s).await;
            fold_the_longboards_pile_missing_a_same_day_sibling(s).await;
        },
        Asks {
            red: &[FIVE_REPAIRS_STAY],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **A correction that names no source does not satisfy the lineage lock.**
/// Both jobs filed under a word the operator does not use are archived and put
/// right, so the words end up as the operator says them, and each correction is
/// captured with nothing naming the job it replaces. The three locks that read
/// the words hold, which is why the lineage lock exists, and it does not.
#[tokio::test]
async fn corrections_that_name_no_source_do_not_satisfy_the_lineage_lock() {
    // **The positive that says the guilty sitting is otherwise a good one**: the
    // locks that read the words, and every other lock in the year, hold.
    expect(
        Year::worked().play(Play::LaterDecemberCorrectsTheJobsAndNamesNoSource),
        Asks {
            red: &[CORRECTIONS_NAME_SOURCES],
            rest_held: true,
            says: &[(CORRECTIONS_NAME_SOURCES, "derived_from")],
            ..Default::default()
        },
    )
    .await;
}

/// A write the room refuses for a bot to make about itself: a key it may not set.
fn a_ceiling_of_its_own() -> Value {
    json!({"subject": "bot:assistant", "content": "a ceiling of its own",
           "sets": {"thought_capacity": "9"}})
}

/// A write about a thing nobody made.
fn a_job_on_nothing() -> Value {
    json!({"subject": "thing:never-furnished", "content": "a job on nothing",
           "provenance": "testimony"})
}

/// 🚨 **A refused call in a script ends the run at the call.** The message names
/// the phase out of the room's own document, the verb and the room's refusal, so
/// a script that no longer matches the shipped rules is found where it breaks and
/// not phases later as locks that fail.
#[tokio::test]
async fn a_refused_scripted_call_ends_the_run_naming_the_phase_and_the_refusal() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let surface = std::sync::Arc::new(surface);
    let phase = room_document().phases[Sitting::LateDecemberLock.at()].clone();
    let day = phase.day.as_deref().expect("a day");
    let said = refused_run(&surface, day, false, "capture", a_ceiling_of_its_own()).await;
    assert!(said.contains(&phase.name), "the phase is named: {said}");
    assert!(said.contains("capture"), "the verb is named: {said}");
    assert!(
        said.contains("thought_capacity") && said.contains("\"status\":\"blocked\""),
        "the room's own refusal rides in the message: {said}",
    );
}

/// 🚨 **The in-place correction the product refuses ends the worked year AT the
/// call.** Later December put two jobs right by editing them in place, the room
/// refused, and the script carried on: eight sittings later three locks failed far
/// from the cause. Played again here, on the year as worked without that sitting,
/// it ends at the edit and names the address.
#[tokio::test]
async fn an_in_place_correction_of_an_earlier_sittings_testimony_ends_the_year_at_the_edit() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let _ = year_before_december().work(&surface).await;
    let wrong_word = address_of(&surface, "thing:skateboard", "serviced the wheels").await;
    let phase = room_document().phases[Sitting::LateDecemberWords.at()].clone();
    let surface = std::sync::Arc::new(surface);
    let said = refused_run(
        &surface,
        phase.day.as_deref().expect("a day"),
        false,
        "update_fact",
        json!({"address": wrong_word, "sets": {"settled": "invoiced"}}),
    )
    .await;
    assert!(said.contains(&phase.name), "the phase is named: {said}");
    assert!(said.contains("update_fact"), "the verb is named: {said}");
    assert!(
        said.contains(&wrong_word),
        "the address that was refused is named: {said}"
    );
}

/// 🚨 **A run that leaves sittings out expects refusals of one kind, and only
/// that kind.** The room refuses a write about a thing the left-out sittings
/// would have made, and such a run goes on. Any other refusal still ends it, in
/// the same run, so the allowance cannot swallow a script that has gone wrong.
#[tokio::test]
async fn a_run_that_leaves_sittings_out_goes_on_past_an_absent_thing_and_no_further() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    let surface = std::sync::Arc::new(surface);
    let day = room_document().phases[Sitting::LateDecemberLock.at()]
        .day
        .clone()
        .expect("a day");

    // The allowance: a write about a thing nobody made.
    let mut leaving = sitting(&surface, &day).await;
    leaving.skipping = true;
    let answer = leaving.did("capture", a_job_on_nothing()).await;
    assert!(is_refused(&answer), "the room refused it: {answer}");

    // The same kind of run, a refusal that is not about an absent thing: ends it.
    let said = refused_run(&surface, &day, true, "capture", a_ceiling_of_its_own()).await;
    assert!(
        said.contains("capture") && said.contains("thought_capacity"),
        "the verb and the refusal are named: {said}"
    );

    // And the allowance belongs to the run that declared it: a sitting of a run
    // that left nothing out is ended by the same write about the same absent thing.
    let said = refused_run(&surface, &day, false, "capture", a_job_on_nothing()).await;
    assert!(said.contains("thing:never-furnished"), "{said}");
}

/// 🚨 **A year in which later December corrects nothing leaves the lineage lock
/// failed, and says the job still stands.** Late November filed both jobs under
/// words the operator does not use, and no sitting after it touched them.
#[tokio::test]
async fn a_year_that_corrects_neither_job_fails_the_lineage_lock_naming_the_standing_job() {
    expect(
        year_before_december(),
        Asks {
            red: &[CORRECTIONS_NAME_SOURCES],
            says: &[(CORRECTIONS_NAME_SOURCES, "still stands at")],
            ..Default::default()
        },
    )
    .await;
}

/// 🚨 **The best sitting is convicted by no lock.** Late November files both jobs
/// under the operator's own word the first time, so later December finds nothing
/// off the vocabulary to put right and leaves them. A lock that demands a
/// correction has to ask first whether anything needed one.
#[tokio::test]
async fn jobs_filed_under_the_operators_word_from_the_start_fail_no_lock() {
    expect(
        Year::worked().play(Play::LateNovemberFilesTheJobsUnderTheOperatorsWord),
        Asks {
            rest_held: true,
            ..Default::default()
        },
    )
    .await;
}

/// **The room's server runs on the day the room gave it, and never on the wall
/// clock.** `ping` says which day the server is acting out. A server on the real
/// clock says none, so this fails on every day if the pin is missing.
#[tokio::test]
async fn the_rooms_server_runs_on_the_day_the_room_gave_it() {
    let (_room, surface) = furnished(Furnish::Live).await;
    assert_eq!(
        surface.stated_day().await.as_deref(),
        Some(SERVER_DAY.as_str()),
        "the server is not acting out the day the room gave it"
    );
}

/// **No needle of this room holds the day its server runs on.** A needle is
/// matched as text in an answer, and the server writes its day into the moments
/// it stamps, so a needle holding that day could be met by a stamp.
#[test]
fn no_needle_here_is_the_servers_own_day() {
    let named = jojobot_exercise::lock::needles_naming(
        &jojobot_exercise::lock::locks_of(expectations::YEAR_ROOM),
        SERVER_DAY.as_str(),
    );
    assert!(
        named.is_empty(),
        "a needle holds the day the server runs on, so a stamp can meet it: {named:?}"
    );
}

/// **A room opened from a seed runs on the room's day, and the seed was built on
/// it.** The copy's server says the day, and the day the seed's first record
/// was stamped on is that same day. A seed built on the real clock would carry
/// the real date in every stamp, which is the stamp a needle can meet.
#[tokio::test]
async fn a_room_opened_from_its_seed_runs_and_was_stamped_on_the_day_the_room_gave_it() {
    let (_room, surface) = furnished(Furnish::Cached).await;
    assert_eq!(
        surface.stated_day().await.as_deref(),
        Some(SERVER_DAY.as_str()),
        "the copy is not acting out the day the room gave it"
    );
    let answer = surface
        .call("recall", json!({"subject": "person:milhouse"}))
        .await;
    let parsed: Value =
        serde_json::from_str(&answer).unwrap_or_else(|e| panic!("a recall answers JSON: {e}"));
    assert_eq!(
        parsed["objects"][0]["as_of"],
        SERVER_DAY.as_str(),
        "the seed was stamped on another day: {answer}"
    );
}
