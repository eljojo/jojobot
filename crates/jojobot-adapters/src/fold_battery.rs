//! **The differential battery for the in-memory fold** (decision log 292, 435).
//!
//! Every read the memory verbs make is asked twice: of the fold, and of the
//! store the fold wraps. The two answers must be the same bytes. Beside each
//! answer from the fold, a [`Counted`] store says how many calls reached the
//! store while it was given, and the reads named in `switched` must have made
//! none. A count with no answer beside it proves nothing — a read that never
//! looked would also make no call — so the two checks live in one function and
//! cannot be run apart.
//!
//! The battery is asked after every kind of write, because the image of the
//! store is correct at startup by construction and wrong later only through a
//! write that did not reach it.

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::future::Future;

use jojobot_domain::memory::{
    EntityId, EntityKind, FactAddress, FactId, Memory, MemoryError, WriteSummary,
};

use crate::counted::Counted;

/// What one run of the battery is asked of.
pub struct Probe<'a> {
    /// The fold under test, which wraps `counted`.
    pub folded: &'a dyn Memory,
    /// The store itself, reached without passing the fold or the counter.
    pub direct: &'a dyn Memory,
    /// The counter between the fold and the store.
    pub counted: &'a Counted,
    /// The reads that must make no call to the store.
    pub switched: &'a [&'a str],
    /// Whether the store answers `keys_kept_off_the_thing` for a record that is
    /// not there as the real store does, with the empty set. The fake refuses it
    /// as an unknown record, and the image follows the real store.
    pub kept_off_misses_are_empty: bool,
    /// The change signal at the last run, so each run can say it moved.
    pub last_signal: std::sync::Mutex<Option<WriteSummary>>,
}

impl Probe<'_> {
    /// One read, asked of both, compared as bytes.
    async fn check<T, A, B>(
        &self,
        when: &str,
        name: &'static str,
        subject: &str,
        via_fold: A,
        via_store: B,
    ) where
        T: Debug,
        A: Future<Output = T>,
        B: Future<Output = T>,
    {
        self.counted.reset();
        let from_fold = format!("{:?}", via_fold.await);
        let made = self.counted.calls();
        let seen = self.counted.seen();
        let from_store = format!("{:?}", via_store.await);
        assert_eq!(
            from_fold, from_store,
            "{when}: {name}({subject}) answered differently from the fold than from the store"
        );
        if self.switched.contains(&name) {
            assert_eq!(
                made, 0,
                "{when}: {name}({subject}) reached the store {made} time(s): {seen:?}"
            );
        }
    }

    /// Every read, for every handle the store answers to and a few it does not.
    pub async fn agree(&self, when: &str) {
        let f = self.folded;
        let d = self.direct;

        let all = d.list_entities(None).await.expect("the store lists");
        let former = d.former_handles().await.expect("the store lists renames");
        let mut handles: Vec<EntityId> = all.iter().map(|e| e.id.clone()).collect();
        handles.extend(former.iter().map(|h| h.former.clone()));
        handles.push(EntityId("person:wendy".into()));
        handles.push(EntityId("work:xi".into()));
        handles.sort();
        handles.dedup();

        self.check(
            when,
            "list_entities",
            "None",
            f.list_entities(None),
            d.list_entities(None),
        )
        .await;
        let mut kinds: Vec<EntityKind> = all.iter().map(|e| e.kind).collect();
        kinds.sort_by_key(|k| k.to_string());
        kinds.dedup();
        for kind in kinds {
            self.check(
                when,
                "list_entities",
                &kind.to_string(),
                f.list_entities(Some(kind)),
                d.list_entities(Some(kind)),
            )
            .await;
        }
        self.check(
            when,
            "former_handles",
            "",
            f.former_handles(),
            d.former_handles(),
        )
        .await;
        self.check(
            when,
            "declared_types",
            "",
            f.declared_types(),
            d.declared_types(),
        )
        .await;
        self.check(
            when,
            "declared_kinds",
            "",
            f.declared_kinds(),
            d.declared_kinds(),
        )
        .await;
        self.check(when, "scan", "", f.scan(), d.scan()).await;
        // **The fold's change signal is its own number, not the store's
        // counts**, so it is not compared with the store's. What it owes is to
        // stay put across reads, to move with every write, and to cost no call.
        self.counted.reset();
        let first = f.write_summary().await.expect("the signal is read");
        let again = f.write_summary().await.expect("the signal is read");
        let made = self.counted.calls();
        assert!(first.is_some(), "{when}: the fold offers a change signal");
        assert_eq!(
            first, again,
            "{when}: the change signal moved with no write"
        );
        if self.switched.contains(&"write_summary") {
            assert_eq!(
                made, 0,
                "{when}: write_summary reached the store {made} time(s)"
            );
        }
        {
            let mut seen = self.last_signal.lock().expect("signal lock");
            if seen.is_some() {
                assert_ne!(
                    *seen, first,
                    "{when}: the change signal did not move although a write landed"
                );
            }
            *seen = first;
        }

        for h in &handles {
            let s = h.as_str();
            self.check(when, "recall", s, f.recall(h), d.recall(h))
                .await;
            self.check(when, "fields", s, f.fields(h), d.fields(h))
                .await;
            self.check(
                when,
                "fields_versioned",
                s,
                f.fields_versioned(h),
                d.fields_versioned(h),
            )
            .await;
            self.check(when, "field_ages", s, f.field_ages(h), d.field_ages(h))
                .await;
            self.check(
                when,
                "claim_histories",
                s,
                sorted(f.claim_histories(h)),
                sorted(d.claim_histories(h)),
            )
            .await;
            self.check(when, "scan_entity", s, f.scan_entity(h), d.scan_entity(h))
                .await;
            // The document a scan reads and the records a recall reads are one
            // read of the same rows; the image keeps one copy of them.
            if let (Ok(Some(doc)), Ok(recalled)) = (d.scan_entity(h).await, d.recall(h).await) {
                assert_eq!(
                    format!("{:?}", doc.facts),
                    format!("{recalled:?}"),
                    "{when}: scan_entity({s}) and recall({s}) answer with different records"
                );
            }
            self.check(when, "backing", s, f.backing(h), d.backing(h))
                .await;
            self.check(when, "key_trails", s, f.key_trails(h), d.key_trails(h))
                .await;
            self.check(
                when,
                "referring_to",
                s,
                f.referring_to(h),
                d.referring_to(h),
            )
            .await;
            self.check(when, "children", s, f.children(h), d.children(h))
                .await;
            self.check(
                when,
                "thought_writers",
                s,
                f.thought_writers(h),
                d.thought_writers(h),
            )
            .await;
            self.check(
                when,
                "entity_history",
                s,
                f.entity_history(h),
                d.entity_history(h),
            )
            .await;
            self.check(
                when,
                "composed_prose",
                s,
                f.composed_prose(h, "own words"),
                d.composed_prose(h, "own words"),
            )
            .await;

            let held = d.fields(h).await.unwrap_or_default();
            self.check(
                when,
                "echoed_defaults",
                s,
                f.echoed_defaults(h, &held),
                d.echoed_defaults(h, &held),
            )
            .await;

            let mut keys: Vec<String> = held.keys().cloned().collect();
            keys.push("note".into());
            keys.push("own_note".into());
            keys.push("blocks".into());
            keys.push("a-key-nobody-wrote".into());
            for key in &keys {
                let subject = format!("{s}, {key}");
                self.check(
                    when,
                    "history",
                    &subject,
                    f.history(h, key),
                    d.history(h, key),
                )
                .await;
            }

            let facts = d.recall(h).await.unwrap_or_default();
            let mut addresses: Vec<FactAddress> = facts.iter().map(|x| x.address()).collect();
            addresses.push(FactAddress {
                home: h.clone(),
                local: FactId("f-nobody-wrote-this".into()),
            });
            for address in &addresses {
                let subject = address.to_string();
                self.check(
                    when,
                    "claim_history",
                    &subject,
                    f.claim_history(address),
                    d.claim_history(address),
                )
                .await;
                let bogus = !facts.iter().any(|x| x.id == address.local);
                if self.kept_off_misses_are_empty || !bogus {
                    self.check(
                        when,
                        "keys_kept_off_the_thing",
                        &subject,
                        f.keys_kept_off_the_thing(address),
                        d.keys_kept_off_the_thing(address),
                    )
                    .await;
                }
                self.check(
                    when,
                    "built_on",
                    &subject,
                    f.built_on(address),
                    d.built_on(address),
                )
                .await;
            }
        }
    }
}

/// A `HashMap` has no order, and the bytes of its Debug form would differ
/// between two reads of the same answer.
async fn sorted<F>(read: F) -> Result<BTreeMap<String, String>, MemoryError>
where
    F: Future<
        Output = Result<
            std::collections::HashMap<FactId, Vec<jojobot_domain::memory::ClaimWrite>>,
            MemoryError,
        >,
    >,
{
    Ok(read
        .await?
        .into_iter()
        .map(|(id, chain)| (id.to_string(), format!("{chain:?}")))
        .collect())
}

// ── the walk: every kind of write, each followed by the whole battery ────────

use jiff::civil::date;
use jojobot_domain::memory::testing::contract::{state, state_patch};
use jojobot_domain::memory::types::{DeclaredType, Field, Origin, ValueType};
use jojobot_domain::memory::{
    Edge, EdgeShape, EntityPatch, FactPatch, Guarded, NewEntity, NewFact, Provenance,
};

fn id(handle: &str) -> EntityId {
    EntityId(handle.to_string())
}

fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

async fn add(store: &dyn Memory, handle: &str, name: &str, parent: Option<&str>) {
    let new = NewEntity {
        parent: parent.map(id),
        ..NewEntity::new(id(handle), name, "fold-battery")
    };
    match store.add_entity(new.clone()).await.expect("add_entity") {
        Guarded::Written(_) => {}
        Guarded::Blocked {
            attempted,
            candidates,
        } => {
            let token = jojobot_domain::memory::guard::override_token(&attempted, &candidates);
            store
                .add_entity(NewEntity {
                    override_token: Some(token),
                    override_reason: Some("a fixture sharing a name".into()),
                    ..new
                })
                .await
                .expect("add_entity over the screen")
                .written()
                .expect("blocked twice");
        }
    }
}

async fn say(
    store: &dyn Memory,
    handle: &str,
    content: &str,
    pairs: &[(&str, &str)],
) -> jojobot_domain::memory::Fact {
    let mut fact = state(NewFact {
        fields: fields(pairs),
        ..NewFact::about(id(handle), content, date(2026, 9, 1))
    });
    fact.session = Some("fold-battery".into());
    store
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("capture blocked")
}

/// **Every kind of write the fold has to keep current, each followed by the
/// whole battery.** `p.folded` takes the writes; `p.direct` is the oracle.
pub async fn walk_every_kind_of_write(p: &Probe<'_>) {
    let w = p.folded;
    let caller = id("bot:sigma");
    p.agree("an empty store").await;

    add(w, "thing:delta", "Delta Station", None).await;
    add(w, "thing:epsilon", "Epsilon Annex", None).await;
    add(w, "person:homer", "Homer", None).await;
    add(w, "person:bart", "Bart", None).await;
    add(w, "place:springfield", "Springfield", None).await;
    add(w, "work:gamma", "Gamma job", Some("thing:delta")).await;
    add(w, "work:sigma", "Sigma job", Some("thing:delta")).await;
    p.agree("after entities are added").await;

    let first = say(
        w,
        "work:gamma",
        "gamma is under way",
        &[("status", "now"), ("rank", "700")],
    )
    .await;
    say(
        w,
        "work:sigma",
        "sigma waits",
        &[("status", "waiting"), ("rank", "100")],
    )
    .await;
    say(w, "person:homer", "homer likes donuts", &[("donuts", "3")]).await;
    // A key written as the record's own, which stays on the record and never
    // reaches the thing: the case `keys_kept_off_the_thing` answers.
    let mut own = NewFact {
        fields: fields(&[("own_note", "stays here")]),
        ..NewFact::about(id("person:homer"), "homer keeps a note", date(2026, 9, 1))
    };
    own.session = Some("fold-battery".into());
    w.capture(own)
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
    p.agree("after claims are captured").await;

    let mut at = NewFact::about(id("person:homer"), "homer is in town", date(2026, 9, 2));
    at.edge = Some(Edge {
        shape: EdgeShape::Location,
        object: id("place:springfield"),
    });
    at.session = Some("fold-battery".into());
    w.capture(at)
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
    p.agree("after a claim draws an edge").await;

    let mut built = state(NewFact {
        fields: fields(&[("blocks", "work:sigma")]),
        derived_from: Some(first.address()),
        refs: vec![id("person:bart")],
        ..NewFact::about(id("work:gamma"), "gamma blocks sigma", date(2026, 9, 3))
    });
    built.session = Some("fold-battery".into());
    w.capture(built)
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
    p.agree("after a claim names another thing in a field, a ref and a source")
        .await;

    say(
        w,
        "person:homer",
        "homer blocks sigma too",
        &[("blocks", "work:sigma")],
    )
    .await;
    say(
        w,
        "thing:epsilon",
        "epsilon blocks both",
        &[("blocks", "work:sigma,work:gamma")],
    )
    .await;
    say(
        w,
        "work:gamma",
        "gamma blocks sigma again",
        &[("blocks", "work:sigma")],
    )
    .await;
    p.agree("after several holders name one thing").await;

    let cleared = say(
        w,
        "work:sigma",
        "sigma had a note",
        &[("note", "keep"), ("rank", "100")],
    )
    .await;
    w.update_fact(
        &cleared.address(),
        state_patch(FactPatch {
            provenance: Some(Provenance::Inference),
            clear_fields: vec!["note".into()],
            ..Default::default()
        }),
        &caller,
    )
    .await
    .expect("update_fact")
    .written()
    .expect("not blocked");
    p.agree("after a key is taken off a record").await;

    w.update_fact(
        &first.address(),
        state_patch(FactPatch {
            content: Some("gamma is nearly done".into()),
            provenance: Some(Provenance::Inference),
            fields: fields(&[("status", "next"), ("rank", "710")]),
            ..Default::default()
        }),
        &caller,
    )
    .await
    .expect("update_fact")
    .written()
    .expect("not blocked");
    p.agree("after a claim is rewritten").await;

    w.set_prose(&id("person:homer"), "Homer, and what he is for.", None)
        .await
        .expect("set_prose");
    p.agree("after prose is set").await;

    w.update_entity(
        &id("work:sigma"),
        EntityPatch {
            name: Some("Sigma errand".into()),
            aliases: Some(vec!["the sigma thing".into()]),
            ..Default::default()
        },
    )
    .await
    .expect("update_entity")
    .written()
    .expect("not blocked");
    p.agree("after an entity is edited").await;

    w.declare_type(
        DeclaredType::new(
            "fold-ranked",
            vec![Field::required("rank", ValueType::Number)],
        ),
        None,
    )
    .await
    .expect("declare_type");
    p.agree("after a type is declared").await;

    w.declare_kind(
        "fold-kind",
        Origin::Declared,
        vec![Field::required("rank", ValueType::Number)],
    )
    .await
    .expect("declare_kind");
    p.agree("after a kind is declared").await;

    w.rename_entity(
        &id("work:sigma"),
        &id("work:tau"),
        None,
        date(2026, 9, 4),
        jojobot_domain::memory::Override::none(),
        None,
    )
    .await
    .expect("rename_entity")
    .written()
    .expect("not blocked");
    p.agree("after a rename").await;

    w.archive_entity(&id("person:bart"), "not needed", None)
        .await
        .expect("archive_entity");
    p.agree("after an archive").await;
    w.restore_entity(&id("person:bart"), None)
        .await
        .expect("restore_entity");
    p.agree("after a restore").await;

    let doomed = say(
        w,
        "person:bart",
        "bart wrote it down wrong",
        &[("donuts", "9")],
    )
    .await;
    w.retract(
        &doomed.address(),
        Some("written wrong"),
        date(2026, 9, 5),
        &caller,
        Some("fold-battery"),
    )
    .await
    .expect("retract");
    p.agree("after a retraction").await;

    w.add_entity_with_first_claim(
        NewEntity::new(id("work:zeta"), "Zeta job", "fold-battery"),
        {
            let mut f = state(NewFact {
                fields: fields(&[("status", "someday")]),
                ..NewFact::about(id("work:zeta"), "zeta, born with a claim", date(2026, 9, 6))
            });
            f.session = Some("fold-battery".into());
            f
        },
    )
    .await
    .expect("add_entity_with_first_claim")
    .written()
    .expect("not blocked");
    p.agree("after a thing is made with its first claim").await;

    w.update_entity_with_claim(
        &id("work:zeta"),
        EntityPatch {
            name: Some("Zeta errand".into()),
            ..Default::default()
        },
        {
            let mut f = state(NewFact {
                fields: fields(&[("status", "next")]),
                ..NewFact::about(
                    id("work:zeta"),
                    "zeta is renamed with a claim",
                    date(2026, 9, 7),
                )
            });
            f.session = Some("fold-battery".into());
            f
        },
    )
    .await
    .expect("update_entity_with_claim")
    .written()
    .expect("not blocked");
    p.agree("after a thing is edited with a claim").await;

    w.merge(
        &id("person:bart"),
        &id("person:homer"),
        Some("the same person"),
        date(2026, 9, 8),
        &caller,
        Some("fold-battery"),
    )
    .await
    .expect("merge");
    p.agree("after a merge").await;

    say(
        w,
        "person:homer",
        "homer after the merge",
        &[("donuts", "4")],
    )
    .await;
    p.agree("after a claim on the survivor of a merge").await;

    // ── thoughts on a thread: who has written into it ─────────────────────────
    add(w, "thread:simpson-kitchen", "Kitchen line", None).await;
    for bot in ["bot:omega", "bot:psi", "bot:rho", "bot:upsilon"] {
        add(w, bot, bot, None).await;
    }
    p.agree("after a thread and four bots are added").await;
    for (bot, thought) in [
        ("bot:omega", "omega is thinking about the thread"),
        ("bot:psi", "psi is thinking about the thread"),
        ("bot:rho", "rho is thinking about the thread"),
    ] {
        thinks_about(w, bot, thought, "thread:simpson-kitchen").await;
    }
    // A claim that is not a connection, and a bot that wrote nothing: neither
    // is a writer.
    say(w, "bot:upsilon", "upsilon keeps to itself", &[]).await;
    p.agree("after bots write thoughts into a thread").await;

    w.archive_entity(&id("bot:psi"), "retired", Some("fold-battery"))
        .await
        .expect("archive");
    p.agree("after a writing bot is archived").await;

    w.merge(
        &id("bot:rho"),
        &id("bot:omega"),
        Some("the same bot"),
        date(2026, 9, 9),
        &caller,
        Some("fold-battery"),
    )
    .await
    .expect("merge bots");
    p.agree("after a writing bot is merged into another").await;

    w.rename_entity(
        &id("thread:simpson-kitchen"),
        &id("thread:milhouse-moves"),
        None,
        date(2026, 9, 10),
        jojobot_domain::memory::Override::none(),
        Some("fold-battery"),
    )
    .await
    .expect("rename the thread")
    .written()
    .expect("rename blocked");
    p.agree("after the thread is renamed").await;

    // A version names its parent by the handle the parent wears NOW, so renaming
    // a parent changes what its children's versions read as.
    w.rename_entity(
        &id("thing:delta"),
        &id("thing:the-mailbox"),
        None,
        date(2026, 9, 11),
        jojobot_domain::memory::Override::none(),
        Some("fold-battery"),
    )
    .await
    .expect("rename the parent")
    .written()
    .expect("rename blocked");
    p.agree("after a parent is renamed").await;
}

/// A thought: a claim on a bot drawing a `connection` edge at a thread.
async fn thinks_about(store: &dyn Memory, bot: &str, thought: &str, thread: &str) {
    let mut fact = NewFact::about(id(bot), thought, date(2026, 9, 4));
    fact.edge = Some(Edge {
        shape: EdgeShape::Connection,
        object: id(thread),
    });
    fact.session = Some("fold-battery".into());
    store
        .capture(fact)
        .await
        .expect("capture a thought")
        .written()
        .expect("thought blocked");
}
