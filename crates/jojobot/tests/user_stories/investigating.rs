//! "Four of my machines stopped in the night. Work out why — and don't make me
//! re-run what the last session already ruled out."
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for, and
//! an assertion beside it goes red on the day the capability lands.
//!
//! `// NOTE —` marks something a SESSION did not do. jojobot answers nothing
//! about it, so no assertion can hold it and none pretends to — and it
//! proposes no call either, because there is no verb that would fix it.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn an_investigation_keeps_what_it_ruled_out() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · the fleet, and what an earlier run believed ─────────────
    let s = story.session().await;

    // GAP — there is no kind for a machine. A host goes in as a `thing`,
    // alongside the bike pump and the folding chairs, so listing things gives
    // back no fleet and nothing marks these four as the same sort of object.
    //   s.add("host:sigma", "Sigma").await;
    s.refused(
        "add_entity",
        json!({"kind": "host", "handle": "sigma", "name": "Sigma", "source": "user-named"}),
    )
    .await
    .says("host");
    for host in ["thing:sigma", "thing:tau", "thing:upsilon", "thing:phi"] {
        s.add(host, host.split_once(':').expect("kind:slug").1)
            .await;
    }

    // The hypothesis this investigation inherits.
    let bursty = s
        .guess("thing:sigma", "hangs seem to track bursty load")
        .await;

    // GAP — `inference` says nobody confirmed a claim. It cannot say how the
    // claim was reached, so a hunch drawn from three correlated samples and a
    // status word decoded off the host itself arrive wearing the same word.
    //   s.guess_by("thing:sigma", "…", method: "correlation over three samples").await;
    s.recall("thing:sigma")
        .await
        .claim(&bursty)
        .says("\"provenance\":\"inference\"")
        .never_says("\"method\"");

    s.wrap("read what the last run believed").await;

    // ── session 2 · the incident ────────────────────────────────────────────
    let s = story.session().await;

    s.event(
        "thing:sigma",
        "stopped responding, and came back on a power cycle",
        "outage",
    )
    .await;

    // An event's typed fields take what prose would have swallowed: when it
    // happened and how long it lasted, as values rather than as a sentence.
    let outage = s
        .event_with(
            "thing:tau",
            "stopped responding",
            "outage",
            json!({"occurred_at": "2026-08-04T02:14:00Z", "down_seconds": "38"}),
            &[],
        )
        .await;

    // GAP — the fields are stored and nothing reads them AS fields. There is
    // no query over a metadata value, so "which outages lasted over thirty
    // seconds" reads every event and parses the numbers again.
    //   s.events_where("down_seconds", greater_than: 30).await;
    s.has_no_verb("events_where", &["search", "recall"]).await;

    // The diagnosis was the bits, so a paraphrase is not re-checkable: the
    // literal status word and the command that produced it are typed fields on
    // the reading, and `refs` names the host they came off.
    let reading = s
        .event_with(
            "thing:sigma",
            "machine-check status word decoded to two flags",
            "measurement",
            json!({"ran": "mcelog --client", "got": "0xB200000000010A"}),
            &["thing:sigma"],
        )
        .await;

    s.recall("thing:sigma")
        .await
        .claim(&reading)
        .says("0xB200000000010A")
        .says("mcelog --client");
    s.recall("thing:tau").await.claim(&outage).says("38");

    s.wrap("recorded the outage and the reading").await;

    // ── session 3 · the reading was taken off the wrong host ────────────────
    let s = story.session().await;

    // An event is taken back rather than corrected: it did not happen the way
    // it was written down, and a fact would instead be rewritten to the truth.
    // Nothing is removed — the record keeps its address and reads as retracted.
    s.retract(&reading, "the status word was read off a different host")
        .await;
    s.recall("thing:sigma")
        .await
        .says("stopped responding")
        .says("retracted");

    // GAP — a retraction reaches the record and nothing that was built on it.
    // Anything derived from the measurement still stands, still reads as
    // current, and nothing connects the two, so the reader who acts on the
    // conclusion never learns its evidence was withdrawn.
    //   s.retract(&reading, "…").cascading_to_derivations().await;
    s.has_no_verb("cascade", &["retract", "update_fact"]).await;

    s.wrap("took back a measurement, and its conclusions stayed")
        .await;

    // ── session 4 · a conclusion, and the retraction that is worth more ─────
    let s = story.session().await;

    let deliberate = s
        .guess_from(
            "thing:tau",
            "its reset was a deliberate reboot, since the kernel changed across the boot",
            &bursty,
        )
        .await;

    // Refuted an hour later with direct access. The claim is rewritten in
    // place, so a reader gets one answer rather than two and a judgement.
    s.correct(
        &deliberate,
        "its reset was NOT a deliberate reboot — refuted by direct access",
    )
    .await;
    s.recall("thing:tau")
        .await
        .says("NOT a deliberate reboot")
        .never_says("since the kernel changed across the boot");

    // GAP — the rewrite fixes the value and loses the reasoning error, which is
    // the part that generalises: a kernel change across a boot says nothing
    // about whether the boot was deliberate, because a host already switched
    // but not rebooted comes up on the newest generation however it goes down.
    // The next investigator can repeat the inference for free.
    //   s.refuted(&deliberate, because: "…", reasoning_error: "…").await;
    s.has_no_verb("refute", &["update_fact", "retract"]).await;

    s.wrap("was wrong, and said so").await;

    // ── session 5 · what is already ruled out ───────────────────────────────
    let s = story.session().await;

    // An exclusion is an ordinary dated claim, and all five come back in one
    // read.
    for ruled_out in [
        "not memory — the machine-check flags do not indicate it",
        "not mains power — the other hosts on the same circuit stayed up",
        "not a deploy — nothing shipped in the window",
        "not the watchdog — its log is empty across the window",
        "not a shared kernel bug — the hosts run different generations",
    ] {
        s.fact("thing:sigma", ruled_out).await;
    }
    s.recall("thing:sigma")
        .await
        .says("not memory")
        .says("not mains power")
        .says("not a deploy")
        .says("not the watchdog")
        .says("not a shared kernel bug");

    // NOTE — nothing marks them as exclusions, so nothing directs the next
    // investigator to read them before proposing a test. The most expensive
    // mistake in this work is re-running an experiment somebody already ran,
    // and the record makes it available rather than preventing it.
    //   s.excluded("thing:sigma", "memory", by: "the machine-check flags").await;

    s.wrap("wrote down what it is not").await;

    // ── session 6 · state that was true this morning ────────────────────────
    let s = story.session().await;

    s.fact(
        "thing:upsilon",
        "running the previous kernel, with the new one staged for next boot",
    )
    .await;
    s.recall("thing:upsilon").await.says("staged for next boot");

    // GAP — that is true until the next reboot and reads as true forever. A
    // claim has no shelf life, so a session reading a month-old kernel state
    // and acting on it is worse off than one that knew nothing. Facts about
    // machines go stale in a way facts about people do not.
    //   s.fact_until("thing:upsilon", "…", stale_after: "the next boot").await;
    s.recall("thing:upsilon")
        .await
        .says("staged for next boot")
        .never_says("stale_after");

    s.wrap("recorded state that will quietly stop being true")
        .await;

    // ── session 7 · the questions the record cannot answer ──────────────────
    let s = story.session().await;

    // Per subject, each host's own record is intact and readable.
    s.recall("thing:sigma").await.says("stopped responding");
    s.recall("thing:tau").await.says("deliberate reboot");

    // GAP — but no read takes a window and returns every event across every
    // host inside it. "Did these two fail together" has to be re-assembled by
    // hand, one subject at a time, by whoever thinks to ask.
    //   s.events_between("2026-08-04T02:00Z", "2026-08-04T03:00Z").await;
    s.has_no_verb("events_between", &["search", "recall"]).await;

    // Topology turned a four-host outage into one: two of the four were guests
    // on a third. Of the typed shapes, `location` points at a place,
    // `membership` at an org and `attendance` at an event, so a link between
    // two machines can only be the untyped one.
    s.fact_about(
        "thing:upsilon",
        "runs as a guest on another host",
        "connection",
        "thing:sigma",
    )
    .await;

    // The edge is walkable, and its meaning is nowhere: "runs on", "is stored
    // on" and "is near" are one edge here. The wire name says so.
    s.recall("thing:upsilon")
        .await
        .says("\"type\":\"relatedTo\"")
        .says("\"object\":\"thing:sigma\"");
    s.through("connection", "thing:sigma", "thing")
        .await
        .says("thing:upsilon");

    // GAP — so the walk proves the two hosts are linked and cannot say that one
    // going down takes the other with it, which is the entire content of the
    // finding that collapsed a four-host outage into one.
    //   s.fact_about("thing:upsilon", "…", "runs-on", "thing:sigma").await;
    s.has_no_verb("depends_on", &["capture", "search"]).await;

    // A decision that constrains future work.
    s.add("project:jojobot-server", "The Server Build").await;
    s.fact(
        "project:jojobot-server",
        "do not run the crash test — the operator's call",
    )
    .await;

    // GAP — it lands as a fact about a project because a decision has no shape
    // of its own, and nothing carries it to whoever next proposes that step. A
    // constraint that has to be searched for gets re-litigated by every session
    // that does not know to look.
    //   s.decided("project:jojobot-server", "do not run the crash test", binds: …).await;
    s.has_no_verb("decision", &["capture", "search"]).await;

    s.wrap("asked what the record could not answer").await;

    story.finish().await;
}
