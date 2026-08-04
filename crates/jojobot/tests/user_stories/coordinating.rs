//! "Run the build for me. Scope the work, hand it out, take the reports — and
//! when I point at a commit six weeks from now, tell me which decision
//! authorized it."
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for.

use super::dsl::Story;

#[tokio::test]
async fn a_coordinator_runs_the_build_and_is_asked_why() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · the board the work happens on ───────────────────────────
    let s = story.session().await;

    s.add("project:jojobot-server", "The Server Build").await;
    // The implementer is a bot like any other, and its box opens with it —
    // which is what makes the dispatch below possible at all.
    s.add("bot:gamma", "Gamma").await;

    // The work queue itself is not modelled here: projects are entities and
    // nest, tasks are not and stay on the board they came from. The queue is
    // the mailbox rail, which is what it is for.

    s.wrap("stood up the board").await;

    // ── session 2 · a defect found by a session doing something else ────────
    let s = story.session().await;

    // It happened, it stays put, and it is not current truth — so it goes in
    // as an event rather than a fact.
    let defect = s
        .event(
            "project:jojobot-server",
            "a claim carrying an escaped quote could not be written at all",
            "defect",
        )
        .await;

    // GAP — an incident has no shape. `event_type` is free text, so "defect"
    // is a word this session chose and nothing else in the system knows, and
    // what it touched, how it was found and what closed it stay in prose. Two
    // sessions recording the same class of incident produce records nothing
    // can compare.
    //   s.incident("project:jojobot-server", touched: &[…], found_by: …, closed_by: …).await;

    s.wrap("recorded the defect where the next session will find it")
        .await;

    // ── session 3 · a decision, with the operator's own words as the receipt ─
    let s = story.session().await;

    let ruling = s
        .fact(
            "project:jojobot-server",
            "a second field carries how settled a claim is, separate from who backs it",
        )
        .await;

    // GAP — nothing distinguishes the operator's exact words from a faithful
    // rendering of them. Both are `testimony`, and the difference is the whole
    // value of a receipt: a session asking why it was built this way needs to
    // know whether it is reading the operator or somebody's summary. The only
    // way to keep the quote is to put it in the claim text and hope nobody
    // tidies it.
    //   s.quoted("project:jojobot-server", verbatim: "…").await;

    s.wrap("recorded the ruling").await;

    // ── session 4 · work goes out, and comes back ───────────────────────────
    let s = story.session().await;

    let dispatched = s
        .post(
            "gamma",
            "Build the second field",
            "The ruling is recorded on the build. Build it test-first and report back.",
        )
        .await;

    s.wrap("dispatched the slice").await;

    // The implementer, on its own connection, which never met the coordinator.
    let g = story.as_bot("bot:gamma").await;
    g.drain()
        .await
        .says("Build the second field")
        .says("report back");
    g.post("otto", "Done", "Shipped it, green, one commit.")
        .await;
    g.processed(&dispatched, "built and reported").await;
    g.wrap("did the work and reported").await;

    // The round trip completes across three sessions that never shared a
    // connection.
    let s = story.session().await;
    s.drain().await.says("Shipped it, green, one commit.");

    // GAP — the rail carried the work and kept no record of it. A message is
    // `processed`, and that is the whole of what a later session learns: not
    // that a slice was scoped, dispatched, built and accepted, and not which
    // of those a given exchange was. The history is reconstructible by reading
    // every message in order and inferring, which is the work a record is
    // supposed to remove.
    //   s.slice("build the second field").dispatched_to("gamma").accepted().await;

    s.wrap("took the report").await;

    // ── session 5 · a commit is questioned ──────────────────────────────────
    let s = story.session().await;

    // What can be answered: the ruling is on the record and reads as the
    // operator's own.
    s.recall("project:jojobot-server")
        .await
        .says("a second field carries how settled a claim is")
        .says("\"provenance\":\"testimony\"");

    // …and a claim can point at the claim it came from, so a consequence of
    // the ruling traces back to it.
    s.guess_from(
        "project:jojobot-server",
        "the schema grew a column because of that ruling",
        &ruling,
    )
    .await;
    s.recall("project:jojobot-server")
        .await
        .says(&format!("\"derived_from\":\"{ruling}\""));

    // GAP — and the chain stops one link short of the thing being questioned.
    // There is no commit: not as an entity, not as a value a claim can carry,
    // not as anything a read returns. The diff in front of the reader is
    // joined to that chain by nothing at all, so answering which decision
    // authorized it means a person matching a diff to a ruling by hand.
    //   s.commit("4eb6c99", authorized_by: &ruling).await;

    // GAP — the defect has the same problem from the other end: nothing
    // connects it to the commit that closed it. The event is on the page, the
    // fix is in the history, and only a person knows they are the same story.
    s.recall("project:jojobot-server")
        .await
        .says("could not be written at all");
    let _ = &defect;
    //   s.closed(&defect, by_commit: "dcedd0a").await;

    s.wrap("answered where the ruling came from, and could not answer for the commit")
        .await;

    // ── session 6 · a second slice, and who is carrying what ────────────────
    let s = story.session().await;

    s.add("bot:delta", "Delta").await;
    s.post(
        "delta",
        "Take the prose codec",
        "Second slice, independent of gamma's. Report back when it is green.",
    )
    .await;

    // Where the coordinator's own mail got to is readable without taking
    // delivery of anything, and the search finds work filed for somebody else.
    s.find("prose codec").await.says("delta");

    // GAP — two slices are now in flight and nothing says so. A message is
    // `new`, `read` or `processed`, which is the state of a message and not
    // the state of the work: an unread dispatch and an unstarted slice are
    // indistinguishable, and so are a read one and a slice half built. The
    // coordinator's own question — who is blocked — has no read behind it.
    //   s.in_flight("project:jojobot-server").says("delta").await;

    s.wrap("two slices out").await;

    // ── session 7 · the next coordinator picks it up ────────────────────────
    let s = story.session().await;

    // Everything the earlier sessions wrote is here for a session that was not
    // there for any of it and was told nothing.
    s.recall("project:jojobot-server")
        .await
        .says("a second field carries how settled a claim is")
        .says("could not be written at all")
        .says("the schema grew a column");
    s.find("escaped quote").await.says("project:jojobot-server");

    // GAP — but every read above needed the project's handle, and nothing
    // hands a fresh coordinator the state of the work. A session that boots
    // and asks where we are gets its own chronology, which is its own past
    // runs and not the build's, and no read composes what is open, what
    // shipped and what is waiting on somebody. That is the question this
    // persona opens with every time.
    //   s.standing("project:jojobot-server").says("delta is mid-slice").await;

    s.wrap("caught up on the build without being re-told").await;

    story.finish().await;
}
