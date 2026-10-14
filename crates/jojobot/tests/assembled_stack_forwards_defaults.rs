//! **A method a port gives a default body is answered by the default in
//! production unless every decorator above the layer that implements it
//! forwards it.** The unit suites cannot see this: each wraps one decorator
//! over a fake, and the harness in `jojobot-mcp` puts `Provisioned` outermost,
//! which is the only reason its cases pass. This file drives the stack the
//! binary builds — the real store, `open_provisioned`, `assemble_memory` and
//! `assemble_ports` — so a decorator that drops one of these methods answers
//! the default here exactly as it does in the deployment.
//!
//! **A test binary of its own, because `open_provisioned` seeds a
//! process-wide kind set and refuses a second call.**

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::mailboxes::DoltMailboxes;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::sessions::DoltSessions;
use jojobot_adapters::owners::MemoryOwners;
use jojobot_adapters::testing::free_port;
use jojobot_domain::mailbox::{MailboxName, Mailboxes};
use jojobot_domain::memory::owned::Provision;
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};
use jojobot_domain::session::{NewEntry, NewSession, Sid};

/// A directory of this run's own, removed when it is done.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const GAMMA_DEFAULT: &str = "The disposable implementer.";

/// What the binary shipped to a fresh instance, plus one default of this
/// case's own on a bot it creates, so the fold has something to lose.
fn supplied() -> jojobot_domain::memory::owned::Provisions {
    let mut supplied = jojobot_mcp::provisions();
    supplied.extend(vec![Provision::field(
        EntityId("bot:gamma".into()),
        "one_liner",
        GAMMA_DEFAULT,
    )]);
    supplied
}

fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

async fn created(memory: &dyn Memory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.into());
    memory
        .add_entity(NewEntity::new(id.clone(), name, "assembled-stack-case"))
        .await
        .expect("the entity is created")
        .written()
        .expect("nothing collides on an empty store");
    id
}

#[tokio::test]
async fn the_stack_the_binary_builds_answers_what_the_layer_beneath_implements() {
    let path =
        std::env::temp_dir().join(format!("jojobot-assembled-forwards-{}", std::process::id()));
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let _scratch = Scratch(path.clone());
    let mut server = Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    migrate::run(server.pool()).await.expect("the schema");

    let supplied = supplied();
    let bare = DoltMemory::open(server.pool().clone()).knowing(supplied.clone());
    // A clone shares the store's counters, and `open_provisioned` takes the
    // store itself.
    let counted = bare.clone();
    let resolved = jojobot::wiring::open_provisioned(bare, supplied)
        .await
        .expect("the build's provisions do not collide with an empty store");
    let (folded, indexed) =
        jojobot::wiring::assemble_memory(resolved.clone()).expect("the index opens");
    let owners = Arc::new(MemoryOwners::new(indexed.clone()));
    let mail = Arc::new(DoltMailboxes::open(server.pool().clone(), owners));
    let stamp = jojobot_domain::session::history::WriteStamp::new();
    let sessions = Arc::new(DoltSessions::open(server.pool().clone()).stamped_by(stamp.clone()));
    let wired = jojobot::wiring::assemble_ports(
        indexed.clone(),
        mail,
        sessions.clone(),
        stamp,
        jojobot_domain::clock::Clock::Real,
    );

    let gamma = created(indexed.as_ref(), "bot:gamma", "Gamma").await;
    let piano = created(indexed.as_ref(), "thing:piano", "Piano").await;

    // ── changes: implemented by the counting layer under mention and index ──
    //
    // Search reads the count to skip a board read, so a layer between the
    // index and the counting one that left the default would answer "no count"
    // and send every refresh to the board. A write through the outermost port
    // moves the count, and a read does not.
    let boxes_before = wired
        .mailboxes
        .changes()
        .expect("the assembled mail port keeps a count of its writes");
    wired
        .mailboxes
        .create_mailbox(&MailboxName("gamma".into()), &gamma, None)
        .await
        .expect("the box opens");
    let boxes_after = wired.mailboxes.changes().expect("a count");
    assert!(
        boxes_after > boxes_before,
        "a write through the outermost mail port moves the count beneath it"
    );
    wired
        .mailboxes
        .list_mailboxes()
        .await
        .expect("the boxes list");
    assert_eq!(
        wired.mailboxes.changes(),
        Some(boxes_after),
        "a read moves nothing"
    );
    assert!(
        wired
            .sessions
            .write_summary()
            .await
            .expect("the signal reads")
            .is_some(),
        "the assembled session port offers a change signal"
    );

    // ── echoed_defaults: implemented by `Provisioned` alone ─────────────────
    //
    // Asked of the outermost layer, which is where `capture` and
    // `update_fact` ask it. A supplied default and a supplied record's own
    // field are both named; a value that differs is not — the second half is
    // what makes the first one a measurement rather than "names everything".
    assert_eq!(
        indexed
            .echoed_defaults(&gamma, &fields(&[("one_liner", GAMMA_DEFAULT)]))
            .await,
        vec!["one_liner".to_string()],
        "a value equal to a shipped default is named through every layer above `Provisioned`"
    );
    assert!(
        indexed
            .echoed_defaults(&gamma, &fields(&[("one_liner", "something else")]))
            .await
            .is_empty(),
        "a value that is not today's default is never named"
    );
    assert_eq!(
        indexed
            .echoed_defaults(
                &EntityId("view:loops".into()),
                &fields(&[("selects", "rhythm")]),
            )
            .await,
        vec!["selects".to_string()],
        "a shipped record's own field is named the same way"
    );

    // ── fields_versioned: implemented by the store, consumed by `Folded` ────
    indexed
        .capture(NewFact {
            fields: fields(&[("note", "first")]),
            sets: fields(&[("note", "first")]).into_keys().collect(),
            ..NewFact::about(gamma.clone(), "gamma's first write", date(2026, 9, 1))
        })
        .await
        .expect("capture ok")
        .written()
        .expect("nothing collides");
    let (held, first) = resolved
        .fields_versioned(&gamma)
        .await
        .expect("a versioned read answers");
    assert!(
        first > 0,
        "the layer `Folded` reads through carries the store's write count, not the default's 0"
    );
    assert_eq!(
        held.get("one_liner").map(String::as_str),
        Some(GAMMA_DEFAULT),
        "the versioned read still folds in what the build ships: {held:?}"
    );
    assert_eq!(held.get("note").map(String::as_str), Some("first"));
    indexed
        .capture(NewFact {
            fields: fields(&[("note", "second")]),
            sets: fields(&[("note", "second")]).into_keys().collect(),
            ..NewFact::about(gamma.clone(), "gamma's second write", date(2026, 9, 2))
        })
        .await
        .expect("capture ok")
        .written()
        .expect("nothing collides");
    let (_, second) = resolved
        .fields_versioned(&gamma)
        .await
        .expect("a versioned read answers");
    assert!(
        second > first,
        "the marker grows with every write, which is what the stale-read guard compares"
    );
    assert_eq!(
        folded
            .fields(&gamma)
            .await
            .expect("the fold answers")
            .get("one_liner")
            .map(String::as_str),
        Some(GAMMA_DEFAULT),
        "the fold is refreshed from the versioned read, so it must hold the shipped default too"
    );

    // ── backing, built_on, referring_to, claim_histories: implemented by the
    //    store, defaulted on the port ────────────────────────────────────────
    //
    // Each is asked of the outermost layer. The store counts every run of its
    // own override, so a layer that drops the method and answers with the
    // port's default body leaves the count where it was. The positive beside
    // each is that the answer is right: a count says which path ran and
    // nothing about whether it answered.
    let kettle = created(indexed.as_ref(), "thing:kettle", "Kettle").await;
    let tuned = indexed
        .capture(NewFact {
            fields: fields(&[("pitch", "low")]),
            sets: fields(&[("pitch", "low")]).into_keys().collect(),
            ..NewFact::about(gamma.clone(), "gamma tuned the piano", date(2026, 9, 4))
        })
        .await
        .expect("capture ok")
        .written()
        .expect("nothing collides");
    // **Declared a reference**, the way the build declares an entitlement's
    // `admits`: the store keeps such a value as the target's permanent id and
    // reads it back as the handle, so a pointer read that asks the store for
    // the handle alone finds nothing.
    indexed
        .declare_type(
            DeclaredType::new(
                "assembled-pointing",
                vec![Field::required("fires", ValueType::Reference)],
            ),
            None,
        )
        .await
        .expect("the declaration lands");
    let derived = indexed
        .capture(NewFact {
            derived_from: Some(tuned.address()),
            fields: fields(&[("fires", piano.as_str())]),
            sets: fields(&[("fires", piano.as_str())]).into_keys().collect(),
            ..NewFact::about(
                gamma.clone(),
                format!("gamma boiled @{kettle}"),
                date(2026, 9, 5),
            )
        })
        .await
        .expect("capture ok")
        .written()
        .expect("nothing collides");

    let before = counted.targeted_reads();
    let backing = indexed.backing(&gamma).await.expect("backing answers");
    assert_eq!(
        counted.targeted_reads(),
        before + 1,
        "backing reached the store's own read, not the port's default"
    );
    assert_eq!(
        backing.get("pitch").map(|b| &b.fact),
        Some(&tuned.id),
        "the key's backing names the claim that wrote it: {backing:?}"
    );

    let before = counted.targeted_reads();
    let built = indexed
        .built_on(&tuned.address())
        .await
        .expect("built_on answers");
    assert_eq!(
        counted.targeted_reads(),
        before + 1,
        "built_on reached the store's own read, not the port's default"
    );
    assert_eq!(
        built.iter().map(|f| &f.id).collect::<Vec<_>>(),
        vec![&derived.id],
        "the claim standing on it comes back: {built:?}"
    );

    let before = counted.targeted_reads();
    let pointing = indexed
        .referring_to(&piano)
        .await
        .expect("referring_to answers");
    assert_eq!(
        counted.targeted_reads(),
        before + 1,
        "referring_to reached the store's own read, not the port's default"
    );
    assert_eq!(
        pointing.iter().map(|f| &f.id).collect::<Vec<_>>(),
        vec![&derived.id],
        "the record naming the handle in a field comes back: {pointing:?}"
    );

    let before = counted.targeted_reads();
    let chains = indexed
        .claim_histories(&gamma)
        .await
        .expect("claim_histories answers");
    assert_eq!(
        counted.targeted_reads(),
        before + 1,
        "claim_histories reached the store's own read, not the port's default"
    );
    assert_eq!(
        chains.get(&derived.id).map(Vec::len),
        Some(1),
        "a claim with one write has an entry with one element: {chains:?}"
    );

    // The mention resolves in the layer that owns the resolution: a rename
    // changes what a past claim reads as, and the batched read must agree
    // with the single-claim read beside it.
    let teapot = EntityId("thing:teapot".into());
    indexed
        .rename_entity(&kettle, &teapot, None, date(2026, 9, 6), None, None, None)
        .await
        .expect("rename ok")
        .written()
        .expect("the rename is not blocked");
    let chains = indexed
        .claim_histories(&gamma)
        .await
        .expect("claim_histories answers");
    let content = &chains[&derived.id][0].content;
    assert!(
        content.contains(teapot.as_str()) && !content.contains("kettle"),
        "a mention reads as the handle the thing answers to now: {content:?}"
    );
    let single = indexed
        .claim_history(&derived.address())
        .await
        .expect("claim_history answers");
    assert_eq!(
        single[0].content, *content,
        "the batched read renders exactly what the single read does"
    );

    // ── entity_history: the port's default refuses, so a decorator that left it
    //    would answer the refusal for every thing ───────────────────────────────
    //
    // Asked of the outermost port. The page of the mention layer renders a
    // mention like the page does, and the store's versions pass through every
    // layer between.
    indexed
        .set_prose(
            &piano,
            &format!("tuned by {}", gamma.as_str().replace("bot:", "@bot:")),
            Some("hst1"),
        )
        .await
        .expect("a prose write");
    let versions = indexed
        .entity_history(&piano)
        .await
        .expect("entity_history reached the store, not the port's default");
    assert_eq!(
        versions.len(),
        2,
        "creation and the prose write: {versions:?}"
    );
    assert_eq!(versions[1].session.as_deref(), Some("hst1"));
    let page = &versions[1]
        .snapshot
        .as_ref()
        .expect("the version kept its row")
        .prose;
    assert!(
        page.contains("@bot:gamma") && !page.contains("@#"),
        "a mention in a kept page reads as the handle, never as the stored mark: {page:?}"
    );

    // ── a merge: the store's version marker is a count of the thing's own
    //    writes, and a merge moves them to the survivor ───────────────────────
    //
    // The merged-away handle's count falls to nothing, which is below what the
    // fold installed when it was written, so a refresh that has to beat the
    // installed version is refused and the old fields are served until a
    // restart.
    let alpha = created(indexed.as_ref(), "person:alpha", "Alpha").await;
    let beta = created(indexed.as_ref(), "person:beta", "Beta").await;
    indexed
        .capture(NewFact {
            fields: fields(&[("due_on", "2026-09-14")]),
            derived: fields(&[("due_on", "2026-09-14")]).into_keys().collect(),
            ..NewFact::about(alpha.clone(), "alpha's own due date", date(2026, 9, 1))
        })
        .await
        .expect("capture ok")
        .written()
        .expect("nothing collides");
    assert_eq!(
        indexed
            .fields(&alpha)
            .await
            .expect("fields answer")
            .get("due_on")
            .map(String::as_str),
        Some("2026-09-14"),
        "the thing holds the key before the merge, which is what the negative below rests on"
    );
    indexed
        .merge(
            &alpha,
            &beta,
            Some("same thing"),
            date(2026, 9, 7),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect("merge ok");
    assert_eq!(
        indexed
            .fields(&beta)
            .await
            .expect("fields answer")
            .get("due_on")
            .map(String::as_str),
        Some("2026-09-14"),
        "the survivor holds what moved"
    );
    assert_eq!(
        indexed
            .fields(&alpha)
            .await
            .expect("the merged-away handle still answers")
            .get("due_on"),
        None,
        "the merged-away handle must not go on serving what it held before the merge"
    );

    // ── the session summaries: implemented by the store ─────────────────────
    let began = |sid: &str, focus: String, at: &str| NewSession {
        bot: gamma.clone(),
        sid: Sid(sid.into()),
        focus,
        started_at: at.parse().expect("a fixed instant"),
        timezone: None,
        started_on: None,
        id: None,
        key_hash: None,
    };
    let busy = wired
        .sessions
        .begin(began(
            "pr01",
            format!("tuning @{piano}"),
            "2026-01-01T00:00:00Z",
        ))
        .await
        .expect("begin ok");
    wired
        .sessions
        .append(
            &busy.id,
            NewEntry::manual(
                "a beat",
                "2026-01-01T00:05:00Z".parse().expect("a fixed instant"),
                None,
            ),
        )
        .await
        .expect("append ok");
    let quiet = wired
        .sessions
        .begin(began(
            "pr02",
            "nothing journalled".to_string(),
            "2026-01-02T00:00:00Z",
        ))
        .await
        .expect("begin ok");

    let delta = EntityId("bot:delta".into());
    indexed
        .rename_entity(&gamma, &delta, None, date(2026, 9, 3), None, None, None)
        .await
        .expect("rename ok")
        .written()
        .expect("the rename is not blocked");

    let before = sessions.full_reads();
    for asked_as in [&gamma, &delta] {
        let mine = wired
            .sessions
            .summaries_of(asked_as)
            .await
            .expect("summaries answer");
        let ids: Vec<_> = mine.iter().map(|s| s.id.clone()).collect();
        assert_eq!(
            ids,
            vec![quiet.id.clone(), busy.id.clone()],
            "a renamed bot's runs are counted under either name ({asked_as}): {mine:?}"
        );
        assert!(
            mine.iter().all(|s| s.bot == delta),
            "a summary names the bot as it is called now: {mine:?}"
        );
        let busy_summary = mine.iter().find(|s| s.id == busy.id).expect("listed");
        assert_eq!(busy_summary.entry_count, 1, "{busy_summary:?}");
        assert!(
            busy_summary.focus.contains(piano.as_str()),
            "a mention in a focus reads as the handle, not a stored id: {busy_summary:?}"
        );
    }
    let everyone = wired
        .sessions
        .all_summaries()
        .await
        .expect("all summaries answer");
    assert_eq!(everyone.len(), 2, "{everyone:?}");
    assert!(everyone.iter().all(|s| s.bot == delta), "{everyone:?}");
    // **The two reads that stand for one run and for the number of runs**: a
    // layer that left either to the default answers the same and reads every run
    // in full, which the counter below is what tells apart.
    let one = wired
        .sessions
        .summary_of(&busy.id)
        .await
        .expect("a summary answers")
        .expect("the run exists");
    assert_eq!(one.bot, delta, "{one:?}");
    assert!(
        one.focus.contains(piano.as_str()),
        "a mention in a focus reads as the handle, not a stored id: {one:?}"
    );
    assert_eq!(
        wired
            .sessions
            .run_counts()
            .await
            .expect("the counts answer"),
        [(delta.clone(), 2)].into_iter().collect(),
        "a renamed bot's runs are one count, under the handle it answers to now"
    );
    assert!(
        wired
            .sessions
            .write_summary()
            .await
            .expect("the signal answers")
            .is_some(),
        "the store offers a cheap change signal, so the layer above it must pass it on rather \
         than say there is none"
    );
    assert_eq!(
        sessions.full_reads(),
        before,
        "no summary read, and no change signal, may fall back to reading a run in full"
    );

    // The positive the counter rests on: a counter that never moves proves
    // what a broken one proves.
    wired
        .sessions
        .sessions_of(&delta)
        .await
        .expect("sessions_of ok");
    assert_eq!(
        sessions.full_reads(),
        before + 2,
        "reading both runs in full is the cost the summaries exist to avoid"
    );

    // ── merge_events and moved_address: implemented by the store ────────────
    //
    // A layer that left either to the trait default would answer "no event" and
    // "nothing moved" in production, over a store that kept both.
    let keeper = created(indexed.as_ref(), "person:krusty", "Krusty").await;
    let twin = created(indexed.as_ref(), "person:skinner", "Skinner").await;
    let claim = indexed
        .capture(NewFact::about(
            twin.clone(),
            "keeps a ledger of favours",
            date(2026, 9, 1),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    indexed
        .merge(
            &twin,
            &keeper,
            Some("one man, filed twice"),
            date(2026, 9, 2),
            &gamma,
            Some("stack-run"),
        )
        .await
        .expect("the fold lands");
    let events = indexed
        .merge_events(Some(&keeper))
        .await
        .expect("the events read through every layer");
    assert_eq!(
        events.len(),
        1,
        "the outermost memory port answers the event the store kept: {events:?}"
    );
    assert_eq!(events[0].session.as_deref(), Some("stack-run"));
    assert!(
        indexed
            .moved_address(&claim.address())
            .await
            .expect("the map reads through every layer")
            .is_some(),
        "the outermost memory port says where the claim went"
    );

    server.stop().await;
}
