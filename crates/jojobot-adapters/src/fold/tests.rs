use super::counting_scan::CountingScan;
use super::stalled_read::StallFirstVersionedRead;
use super::*;
use jiff::civil::date;
use jojobot_domain::memory::testing::{InMemoryMemory, contract};
use jojobot_domain::memory::types::{Field, ValueType};

fn alpha() -> EntityId {
    EntityId::person("person:alpha")
}

fn beta() -> EntityId {
    EntityId::person("person:beta")
}

async fn seeded(store: &Folded, id: &EntityId, name: &str) {
    store
        .add_entity(NewEntity::new(id.clone(), name, "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
}

/// **What is kept beside the documents by key follows every kind of write.**
/// After each write the values held by key, the places kept for a date or a
/// number, the items of a list and the records worked out from a claim are
/// what a build over the documents makes: a write takes off what the thing
/// held and puts on what it holds, and leaves an empty entry nowhere.
#[tokio::test]
async fn what_is_kept_by_key_follows_every_kind_of_write() {
    use jojobot_domain::memory::FactPatch;
    let folded = Folded::new(Arc::new(InMemoryMemory::booted()));
    folded.rebuild().await.expect("rebuild ok");
    for declared in [
        DeclaredType::new(
            "kept",
            vec![
                Field::new("due", ValueType::Date),
                Field::new("points", ValueType::Number),
                Field::listing("tags", ValueType::Text),
            ],
        ),
        // One key two types read two ways keeps a place for each.
        DeclaredType::new("kept-too", vec![Field::new("points", ValueType::Date)]),
    ] {
        folded.declare_type(declared, None).await.expect("declared");
    }
    let check = |after: &'static str| {
        let folded = &folded;
        async move {
            let checked = folded
                .held(|image| image.upkeep_is_what_a_build_would_make())
                .expect("the image is held");
            if let Err(differs) = checked {
                panic!("after {after}: {differs}");
            }
        }
    };
    let (gamma, delta) = (
        EntityId::person("person:gamma"),
        EntityId::person("person:kappa"),
    );
    for (id, name) in [
        (alpha(), "Alpha"),
        (beta(), "Beta"),
        (gamma.clone(), "Gamma"),
    ] {
        seeded(&folded, &id, name).await;
    }
    check("three things are added").await;

    let set = |id: &EntityId, what: &str, held: &[(&str, &str)]| {
        contract::state(NewFact {
            fields: held
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
            ..NewFact::about(id.clone(), what, date(2026, 9, 1))
        })
    };
    let first = folded
        .capture(set(
            &alpha(),
            "alpha holds",
            &[
                ("due", "2026-09-14"),
                ("points", "3"),
                ("tags", "kitchen, garden"),
            ],
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    check("a capture").await;
    let beta_holds = folded
        .capture(set(
            &beta(),
            "beta holds",
            &[
                ("due", "2026-09-14"),
                ("points", "lots"),
                ("tags", "kitchen"),
            ],
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    check("a capture that ties and holds a word where a number goes").await;
    let mut built_on = set(&gamma, "gamma stands on alpha", &[("tags", "")]);
    built_on.derived_from = Some(FactAddress::new(alpha(), first.id.clone()));
    folded
        .capture(built_on)
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    check("a capture worked out from a claim").await;

    for (after, held) in [
        (
            "an edit that moves a date and trims a list",
            [("due", "2026-10-01"), ("points", "3"), ("tags", "kitchen")].as_slice(),
        ),
        (
            "an edit that adds an item to a list",
            [
                ("due", "2026-10-01"),
                ("points", "3"),
                ("tags", "kitchen, shed"),
            ]
            .as_slice(),
        ),
        (
            "an edit that takes an item off a list",
            [("due", "2026-10-01"), ("points", "3"), ("tags", "shed")].as_slice(),
        ),
    ] {
        folded
            .update_fact(
                &first.address(),
                contract::state_patch(FactPatch {
                    fields: held
                        .iter()
                        .map(|(key, value)| (key.to_string(), value.to_string()))
                        .collect(),
                    ..FactPatch::default()
                }),
                &EntityId("bot:sigma".into()),
            )
            .await
            .expect("update ok")
            .written()
            .expect("not blocked");
        check(after).await;
    }

    folded
        .retract(
            &beta_holds.address(),
            Some("not so"),
            date(2026, 9, 9),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect("retract ok");
    check("a retraction").await;

    folded
        .archive_entity(&gamma, "set aside", None)
        .await
        .expect("archive ok");
    check("an archive").await;
    folded
        .restore_entity(&gamma, None)
        .await
        .expect("restore ok");
    check("a restore").await;

    folded
        .set_prose(&alpha(), "alpha, in words", None)
        .await
        .expect("prose ok");
    check("a page").await;

    let notes = EntityId("document:contract-notes".into());
    seeded(&folded, &notes, "Contract Notes").await;
    for section in ["# Heading", "The words under it."] {
        folded
            .capture(NewFact::about(notes.clone(), section, date(2026, 9, 9)))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
    }
    check("a document put as sections").await;

    folded
        .rename_entity(
            &gamma,
            &delta,
            None,
            date(2026, 9, 10),
            jojobot_domain::memory::Override::none(),
            None,
        )
        .await
        .expect("rename ok")
        .written()
        .expect("not blocked");
    check("a rename").await;

    folded
        .merge(
            &delta,
            &alpha(),
            Some("the same"),
            date(2026, 9, 11),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect("merge ok");
    check("a merge").await;

    folded
        .declare_type(
            DeclaredType::new(
                "kept-three",
                vec![Field::new("shed_count", ValueType::Number)],
            ),
            None,
        )
        .await
        .expect("declared");
    check("a declaration").await;
}

/// **The boot path.** A store already holding a write before `Folded`
/// ever wraps it is folded correctly once `rebuild` runs — the cache is
/// not only ever filled by write-through.
#[tokio::test]
async fn rebuild_folds_what_the_store_already_held() {
    let inner = Arc::new(InMemoryMemory::booted());
    inner
        .add_entity(NewEntity::new(alpha(), "Alpha", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
    inner
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: BTreeMap::from([("due_on".to_string(), "2026-09-14".to_string())]),
            ..NewFact::about(
                alpha(),
                "was here before the fold wrapped it",
                date(2026, 9, 1),
            )
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    let folded = Folded::new(inner);
    let before_rebuild = folded.fields(&alpha()).await.expect("fields ok");
    assert_eq!(
        before_rebuild.get("due_on").map(String::as_str),
        Some("2026-09-14"),
        "a cache miss falls through to the store, so this reads right even before rebuild"
    );

    let folded_count = folded.rebuild().await.expect("rebuild ok");
    assert_eq!(folded_count, 1, "one entity to fold");
    assert_eq!(
        folded.fields(&alpha()).await.expect("fields ok"),
        folded
            .inner
            .fields(&alpha())
            .await
            .expect("scanned fields ok"),
        "the folded answer must equal the scanned answer"
    );
}

/// **A first fill that failed is filled again by the next write.** The store
/// was unreachable when the server booted and recovered a moment later. The
/// next write fills the image, and the read after it comes from the image,
/// makes no call to the store, and carries that write.
#[tokio::test]
async fn a_first_fill_that_failed_is_filled_again_by_the_next_write() {
    use crate::counted::Counted;
    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    counted.set_unreachable(true);
    folded
        .rebuild()
        .await
        .expect_err("the store cannot be reached");
    counted.set_unreachable(false);

    seeded(&folded, &alpha(), "Alpha").await;
    folded
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: BTreeMap::from([("due_on".to_string(), "2026-09-14".to_string())]),
            ..NewFact::about(
                alpha(),
                "written after the store came back",
                date(2026, 9, 1),
            )
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    counted.reset();
    let fields = folded.fields(&alpha()).await.expect("fields ok");
    assert_eq!(
        counted.calls(),
        0,
        "the read reached the store: {:?}",
        counted.seen()
    );
    assert_eq!(
        fields.get("due_on").map(String::as_str),
        Some("2026-09-14"),
        "the image does not carry the write"
    );
}

/// **A capture whose caller stopped waiting after the store committed is in
/// the image.** The write landed and the future that would have followed it
/// was dropped. A read from the image must still answer as the store does.
#[tokio::test]
async fn a_capture_dropped_after_the_store_committed_is_in_the_image() {
    use crate::counted::Counted;
    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("an empty store fills");
    seeded(&folded, &alpha(), "Alpha").await;

    counted.hang_after_next("capture");
    let waited = crate::fold_dropped_tests::cancelled_after_commit(
        counted.hung(),
        folded.capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: BTreeMap::from([("due_on".to_string(), "2026-09-14".to_string())]),
            ..NewFact::about(alpha(), "written, then the caller left", date(2026, 9, 1))
        })),
    )
    .await;
    assert!(waited.is_err(), "the capture is cancelled: {waited:?}");
    counted.release_hung();
    folded.writer().drain().await;
    let in_store = store.fields(&alpha()).await.expect("the store answers");
    assert_eq!(
        in_store.get("due_on").map(String::as_str),
        Some("2026-09-14"),
        "the capture committed in the store"
    );

    let in_image = folded.fields(&alpha()).await.expect("the image answers");
    assert_eq!(in_image, in_store, "the image misses a committed capture");
    let claims = folded.recall(&alpha()).await.expect("recall ok");
    assert!(
        claims.iter().any(|f| f.content.contains("the caller left")),
        "the image does not hold the claim: {claims:?}"
    );
}

/// **A creation whose caller stopped waiting after the store committed is in
/// the image.** The same scope guards every write verb, not only `capture`.
#[tokio::test]
async fn an_add_entity_dropped_after_the_store_committed_is_in_the_image() {
    use crate::counted::Counted;
    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("an empty store fills");

    counted.hang_after_next("add_entity");
    let waited = crate::fold_dropped_tests::cancelled_after_commit(
        counted.hung(),
        folded.add_entity(NewEntity::new(alpha(), "Alpha", "user-named")),
    )
    .await;
    assert!(waited.is_err(), "the creation is cancelled: {waited:?}");
    counted.release_hung();
    folded.writer().drain().await;
    let in_store = store.list_entities(None).await.expect("the store answers");
    assert_eq!(in_store.len(), 1, "the creation committed in the store");

    let in_image = folded.list_entities(None).await.expect("the image answers");
    assert_eq!(in_image, in_store, "the image misses a committed creation");
}

/// **The writes of a claim a survivor cannot serve are answered from the
/// image.** The image names such a claim; its writes are read when the
/// document is filled, so asking for them makes no call to the store. Paired
/// with the answer being the store's own, and non-empty.
#[tokio::test]
async fn the_writes_of_a_claim_a_survivor_cannot_serve_are_read_from_the_image() {
    use crate::counted::Counted;
    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("an empty store fills");
    seeded(&folded, &alpha(), "Alpha").await;
    seeded(&folded, &beta(), "Beta").await;
    folded
        .capture(NewFact::about(beta(), "plays the cornet", date(2026, 9, 1)))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    folded
        .merge(
            &beta(),
            &alpha(),
            Some("filed twice"),
            date(2026, 9, 2),
            &EntityId("bot:sigma".into()),
            Some("run"),
        )
        .await
        .expect("merge ok");
    // A claim lands under the folded key past the fold, as damage does; a
    // restart reads it.
    let stray = store.stray_claim_under(&beta(), "a stray claim");
    folded.rebuild().await.expect("the image fills again");
    assert!(
        folded.unmapped_claims(&alpha()).await.expect("reads").len() == 1,
        "the survivor names the stray claim, so the case reaches the branch"
    );

    counted.reset();
    let from_image = folded
        .claim_history(&stray)
        .await
        .expect("the address reads");
    assert_eq!(
        counted.calls(),
        0,
        "the writes of an unmapped claim reached the store: {:?}",
        counted.seen()
    );
    assert_eq!(
        from_image,
        store.claim_history(&stray).await.expect("the store reads"),
        "the image answers what the store answers"
    );
    assert_eq!(from_image.len(), 1, "the claim has its one write");
}

/// **Write-through keeps the fold current, and reordering after a
/// retraction is where a fold built from arithmetic instead of a re-read
/// would go wrong.** Two writes name the same key; retracting the newer
/// one must bring the older, still-active write back — not merely blank
/// the key, and not still show the retracted value.
#[tokio::test]
async fn capture_and_retract_keep_the_fold_equal_to_the_store() {
    let inner = Arc::new(InMemoryMemory::booted());
    let folded = Folded::new(inner);
    seeded(&folded, &alpha(), "Alpha").await;

    let first = folded
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: BTreeMap::from([("due_on".to_string(), "2026-09-14".to_string())]),
            ..NewFact::about(alpha(), "first due date", date(2026, 9, 1))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    assert_eq!(
        folded
            .fields(&alpha())
            .await
            .expect("fields ok")
            .get("due_on"),
        Some(&"2026-09-14".to_string()),
        "the newest write wins, positive case"
    );

    let second = folded
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: BTreeMap::from([("due_on".to_string(), "2026-09-21".to_string())]),
            ..NewFact::about(alpha(), "moved to next week", date(2026, 9, 8))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    assert_eq!(
        folded
            .fields(&alpha())
            .await
            .expect("fields ok")
            .get("due_on"),
        Some(&"2026-09-21".to_string()),
        "the second write is now newest"
    );
    assert_eq!(
        folded.fields(&alpha()).await.expect("fields ok"),
        folded
            .inner
            .fields(&alpha())
            .await
            .expect("scanned fields ok"),
        "the folded answer must equal the scanned answer"
    );

    folded
        .retract(
            &FactAddress {
                home: alpha(),
                local: second.id.clone(),
            },
            Some("moved back"),
            date(2026, 9, 9),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect("retract ok");

    assert_eq!(
        folded
            .fields(&alpha())
            .await
            .expect("fields ok")
            .get("due_on"),
        Some(&"2026-09-14".to_string()),
        "retracting the newer write must bring the older active one back, not blank the key"
    );
    assert_eq!(
        folded.fields(&alpha()).await.expect("fields ok"),
        folded
            .inner
            .fields(&alpha())
            .await
            .expect("scanned fields ok"),
        "the folded answer must equal the scanned answer"
    );
    let _ = first;
}

/// **A merge moves fields, not only claims — the fold has to follow to
/// both sides.** The folded thing's key must stop reading and the
/// survivor's must start.
#[tokio::test]
async fn merge_moves_the_fold_to_the_survivor() {
    let inner = Arc::new(InMemoryMemory::booted());
    let folded = Folded::new(inner);
    seeded(&folded, &alpha(), "Alpha").await;
    seeded(&folded, &beta(), "Beta").await;

    folded
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: BTreeMap::from([("due_on".to_string(), "2026-09-14".to_string())]),
            ..NewFact::about(alpha(), "alpha's own due date", date(2026, 9, 1))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    folded
        .merge(
            &alpha(),
            &beta(),
            Some("same thing"),
            date(2026, 9, 10),
            &EntityId::person("person:gamma"),
            None,
        )
        .await
        .expect("merge ok");

    assert_eq!(
        folded
            .fields(&alpha())
            .await
            .expect("fields ok")
            .get("due_on"),
        None,
        "the folded handle's own fields must not still show what moved, negative case"
    );
    assert_eq!(
        folded
            .fields(&beta())
            .await
            .expect("fields ok")
            .get("due_on"),
        Some(&"2026-09-14".to_string()),
        "the survivor must now hold what moved, positive case"
    );
    assert_eq!(
        folded.fields(&beta()).await.expect("fields ok"),
        folded
            .inner
            .fields(&beta())
            .await
            .expect("scanned fields ok"),
        "the folded answer must equal the scanned answer"
    );
}

/// **`fields` answers from the cache, not by re-asking the store every
/// time.** A write made straight to the wrapped store — bypassing
/// `Folded`, the shape a second writer would take — is invisible to a
/// `fields` call that hits the cache, exactly the load-bearing
/// single-writer assumption this module's doc warns about. Without this
/// case, `fields` falling straight through to the store on every call —
/// correct, but pointless — would look identical to every other test
/// here passing.
#[tokio::test]
async fn fields_are_served_from_the_cache_not_rederived_on_every_call() {
    let inner = Arc::new(InMemoryMemory::booted());
    let folded = Folded::new(inner);
    folded.rebuild().await.expect("an empty store fills");
    seeded(&folded, &alpha(), "Alpha").await;
    folded
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: BTreeMap::from([("due_on".to_string(), "2026-09-14".to_string())]),
            ..NewFact::about(alpha(), "cached", date(2026, 9, 1))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    // Straight to the wrapped store, never through `Folded::capture` —
    // the cache is never told this happened.
    folded
        .inner
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: BTreeMap::from([("due_on".to_string(), "2026-09-21".to_string())]),
            ..NewFact::about(alpha(), "written around the fold", date(2026, 9, 8))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    assert_eq!(
        folded
            .inner
            .fields(&alpha())
            .await
            .expect("fields ok")
            .get("due_on"),
        Some(&"2026-09-21".to_string()),
        "the store itself already sees the bypassed write"
    );
    assert_eq!(
        folded
            .fields(&alpha())
            .await
            .expect("fields ok")
            .get("due_on"),
        Some(&"2026-09-14".to_string()),
        "the fold still answers with what it cached, not what the store now holds"
    );
}

/// **The behavioural contract holds wrapped in a fold, not only bare.**
/// `Folded` forwards every guard, every read and every write it does not
/// itself refresh straight to the store underneath, so the whole existing
/// suite is the regression test for that claim — a decorator that changed
/// what any of it answers would fail here, with no case written for the
/// fold itself.
/// **The same contract, read through a loaded image.** Every case writes
/// through the fold from an empty store, so each read it makes is one the
/// image answers. Dealt into parts that run side by side, because a read
/// through the fake costs as much as the store is large and a single store
/// holding every case's rows would take minutes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fake_satisfies_the_contract_when_folded_and_loaded() {
    let mut parts = Vec::new();
    for part in 0..contract::PARTS {
        parts.push(tokio::spawn(async move {
            let folded = Folded::new(Arc::new(InMemoryMemory::booted()));
            folded.rebuild().await.expect("an empty store fills");
            contract::run_part(&folded, part, contract::PARTS).await
        }));
    }
    let mut ran = 0;
    for part in parts {
        ran += part.await.expect("a part of the contract held");
    }
    assert_eq!(
        ran,
        contract::case_count(),
        "a case was dropped between parts"
    );
}

/// The fold forwards which keys of a record the store holds off the thing.
#[tokio::test]
async fn the_fold_forwards_the_keys_a_switched_store_holds_off_a_thing() {
    contract::own_fields_stay_while_settings_and_computed_keys_reach(&Folded::new(Arc::new(
        InMemoryMemory::booted(),
    )))
    .await;
}

/// **A store that says how wide it can be read, and counts how wide it was.**
struct Gauged {
    inner: Arc<InMemoryMemory>,
    width: usize,
    in_flight: std::sync::atomic::AtomicUsize,
    peak: std::sync::atomic::AtomicUsize,
}

#[async_trait]
impl Memory for Gauged {
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        self.inner.add_entity(new).await
    }
    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        self.inner.list_entities(kind).await
    }
    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        self.inner.former_handles().await
    }
    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.inner.update_entity(handle, patch).await
    }
    async fn archive_entity(
        &self,
        id: &EntityId,
        reason: &str,
        session: Option<&str>,
    ) -> Result<Entity, MemoryError> {
        self.inner.archive_entity(id, reason, session).await
    }
    async fn restore_entity(
        &self,
        id: &EntityId,
        session: Option<&str>,
    ) -> Result<(Entity, Archived), MemoryError> {
        self.inner.restore_entity(id, session).await
    }
    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: Date,
        resemblance: jojobot_domain::memory::Override<'_>,
        session: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.inner
            .rename_entity(from, to, parent, date, resemblance, session)
            .await
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        self.inner.capture(fact).await
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.inner.recall(subject).await
    }
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        self.inner.history(entity, key).await
    }
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        self.inner.claim_history(address).await
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        self.inner.update_fact(address, patch, caller).await
    }
    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        self.inner.fields(entity).await
    }
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Retraction, MemoryError> {
        self.inner
            .retract(address, reason, date, caller, session)
            .await
    }
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Merge, MemoryError> {
        self.inner
            .merge(folded, survivor, reason, date, caller, session)
            .await
    }
    async fn set_prose(
        &self,
        entity: &EntityId,
        prose: &str,
        session: Option<&str>,
    ) -> Result<String, MemoryError> {
        self.inner.set_prose(entity, prose, session).await
    }
    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        self.inner.scan().await
    }
    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<DocScan>, MemoryError> {
        self.inner.scan_entity(entity).await
    }
    async fn declare_type(
        &self,
        declared: DeclaredType,
        by_session: Option<&str>,
    ) -> Result<DeclaredType, MemoryError> {
        self.inner.declare_type(declared, by_session).await
    }
    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        self.inner.declared_types().await
    }
    async fn declare_kind(
        &self,
        token: &str,
        origin: types::Origin,
        fields: Vec<types::Field>,
    ) -> Result<bool, MemoryError> {
        self.inner.declare_kind(token, origin, fields).await
    }
    async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError> {
        self.inner.declared_kinds().await
    }
    fn read_width(&self) -> usize {
        self.width
    }
    async fn fields_versioned(
        &self,
        entity: &EntityId,
    ) -> Result<(BTreeMap<String, String>, u64), MemoryError> {
        use std::sync::atomic::Ordering::SeqCst;
        let now = self.in_flight.fetch_add(1, SeqCst) + 1;
        self.peak.fetch_max(now, SeqCst);
        // Long enough for every loader the fold starts to be in here at once.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let answer = self.inner.fields_versioned(entity).await;
        self.in_flight.fetch_sub(1, SeqCst);
        answer
    }
    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        self.inner.reclaim_kind(token).await
    }
}

/// **The fold's startup read never has more things in flight than the store
/// says it can answer at once**, and does reach that many. The width is the
/// store's own answer, so a store with a smaller pool is read more narrowly
/// and the fold holds no number of its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_startup_read_never_has_more_in_flight_than_the_store_answers() {
    for width in [2usize, 3] {
        let inner = Arc::new(InMemoryMemory::booted());
        for handle in [
            "alpha", "kappa", "omega", "sigma", "lambda", "rho", "tau", "psi",
        ]
        .into_iter()
        {
            inner
                .add_entity(NewEntity::new(
                    EntityId::person(handle),
                    handle.to_uppercase(),
                    "user-named",
                ))
                .await
                .expect("add ok")
                .written()
                .expect("not blocked");
        }
        let gauged = Arc::new(Gauged {
            inner,
            width,
            in_flight: Default::default(),
            peak: Default::default(),
        });
        let folded = Folded::new(gauged.clone());
        folded.rebuild().await.expect("the image fills");
        let peak = gauged.peak.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            peak, width,
            "eight things were read with at most {width} in flight, and that many at once"
        );
    }
}

/// **A store whose `field_ages` answer is taken and then held back**, so a
/// test can let a write land between a reader's store read and the moment
/// that reader installs what it read.
struct AgesBehindAGate {
    inner: Arc<InMemoryMemory>,
    /// The next `field_ages` call reads the store, tells the first of these
    /// it has, and waits for the second before it answers.
    gate: std::sync::Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
}

#[async_trait]
impl Memory for AgesBehindAGate {
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        self.inner.add_entity(new).await
    }
    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        self.inner.list_entities(kind).await
    }
    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        self.inner.former_handles().await
    }
    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.inner.update_entity(handle, patch).await
    }
    async fn archive_entity(
        &self,
        id: &EntityId,
        reason: &str,
        session: Option<&str>,
    ) -> Result<Entity, MemoryError> {
        self.inner.archive_entity(id, reason, session).await
    }
    async fn restore_entity(
        &self,
        id: &EntityId,
        session: Option<&str>,
    ) -> Result<(Entity, Archived), MemoryError> {
        self.inner.restore_entity(id, session).await
    }
    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: Date,
        resemblance: jojobot_domain::memory::Override<'_>,
        session: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.inner
            .rename_entity(from, to, parent, date, resemblance, session)
            .await
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        self.inner.capture(fact).await
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.inner.recall(subject).await
    }
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        self.inner.history(entity, key).await
    }
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        self.inner.claim_history(address).await
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        self.inner.update_fact(address, patch, caller).await
    }
    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        self.inner.fields(entity).await
    }
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Retraction, MemoryError> {
        self.inner
            .retract(address, reason, date, caller, session)
            .await
    }
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Merge, MemoryError> {
        self.inner
            .merge(folded, survivor, reason, date, caller, session)
            .await
    }
    async fn set_prose(
        &self,
        entity: &EntityId,
        prose: &str,
        session: Option<&str>,
    ) -> Result<String, MemoryError> {
        self.inner.set_prose(entity, prose, session).await
    }
    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        self.inner.scan().await
    }
    /// **The store read happens, then the call waits.** Whatever the store
    /// answers is already taken when the gate holds, so a write that lands
    /// while it waits is one the answer cannot include.
    async fn field_ages(&self, entity: &EntityId) -> Result<FieldAges, MemoryError> {
        let answered = self.inner.field_ages(entity).await?;
        let gate = self.gate.lock().expect("gate lock").take();
        if let Some((reached, release)) = gate {
            reached.notify_one();
            release.notified().await;
        }
        Ok(answered)
    }
    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<DocScan>, MemoryError> {
        Ok(self
            .inner
            .scan()
            .await?
            .into_iter()
            .find(|d| d.entity.as_ref().is_some_and(|e| &e.id == entity)))
    }
    async fn declare_type(
        &self,
        declared: DeclaredType,
        by_session: Option<&str>,
    ) -> Result<DeclaredType, MemoryError> {
        self.inner.declare_type(declared, by_session).await
    }
    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        self.inner.declared_types().await
    }
    async fn declare_kind(
        &self,
        token: &str,
        origin: types::Origin,
        fields: Vec<types::Field>,
    ) -> Result<bool, MemoryError> {
        self.inner.declare_kind(token, origin, fields).await
    }
    async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError> {
        self.inner.declared_kinds().await
    }
    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        self.inner.reclaim_kind(token).await
    }
}

/// **A reader that read the store before a write cannot install what it
/// read after the write has cleared the cache.** The write drops every cached
/// age when it finishes, and a reader that was between its store read and its
/// insert would put the pre-write ages back, to be served as current until the
/// next write. The double holds the reader exactly there, the write passes
/// through, and the reader is let go. A reader that comes after must be told
/// the ages the store holds now, not the ones the late reader carried.
#[tokio::test]
async fn a_reader_that_read_before_a_write_does_not_cache_the_pre_write_ages() {
    let inner = Arc::new(InMemoryMemory::booted());
    let gated = Arc::new(AgesBehindAGate {
        inner: inner.clone(),
        gate: std::sync::Mutex::new(None),
    });
    let folded = Arc::new(Folded::new(gated.clone()));
    seeded(&folded, &alpha(), "Alpha").await;
    let write = |colour: &str, content: &str| {
        jojobot_domain::memory::testing::contract::state(NewFact {
            fields: BTreeMap::from([("colour".to_string(), colour.to_string())]),
            ..NewFact::about(alpha(), content.to_string(), date(2026, 9, 1))
        })
    };
    folded
        .capture(write("green", "first colour"))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    let reached = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    *gated.gate.lock().expect("gate lock") = Some((reached.clone(), release.clone()));
    let reader = tokio::spawn({
        let folded = folded.clone();
        async move { folded.field_ages(&alpha()).await }
    });
    // The reader has read the store and not yet installed what it read.
    reached.notified().await;
    folded
        .capture(write("blue", "repainted"))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    release.notify_one();
    let late = reader
        .await
        .expect("the reader finished")
        .expect("ages read");

    let served = folded.field_ages(&alpha()).await.expect("ages read");
    let held = inner.field_ages(&alpha()).await.expect("ages read");
    assert_eq!(
        served, held,
        "the ages served after the write are the ones the store holds now"
    );
    assert!(
        served.keys["colour"] > late.keys["colour"],
        "the late reader carried the pre-write age and it was served: \
         late {late:?}, served {served:?}"
    );
}

/// **A held image says whether a page holds prose, and the store is not
/// asked.** One person has a page and one has none; both are answered from
/// the image after the boot fills it.
#[tokio::test]
async fn has_prose_is_answered_from_the_image() {
    let counted = Arc::new(crate::counted::Counted::new(Arc::new(
        InMemoryMemory::booted(),
    )));
    let folded = Folded::new(counted.clone());
    let milhouse = EntityId::person("person:milhouse");
    let nelson = EntityId::person("person:nelson");
    for (id, name) in [(&milhouse, "Milhouse"), (&nelson, "Nelson")] {
        counted
            .add_entity(NewEntity::new(id.clone(), name, "user-named"))
            .await
            .expect("add ok")
            .written()
            .expect("not blocked");
    }
    counted
        .set_prose(&milhouse, "Milhouse lives next door.", None)
        .await
        .expect("the page is set");
    folded.rebuild().await.expect("the image fills");
    counted.reset();

    assert!(folded.has_prose(&milhouse).await.expect("has_prose ok"));
    assert!(!folded.has_prose(&nelson).await.expect("has_prose ok"));
    assert_eq!(
        counted.calls(),
        0,
        "the store is not asked: {:?}",
        counted.seen()
    );
}

/// 🚨 **One write's own store work must not grow with how much is
/// already stored.** Before `Folded::scan_entity` forwarded, it fell to
/// [`Memory::scan_entity`]'s own default: read every entity's every fact
/// through `scan`, then keep one row and discard the rest — a cost that
/// grows with the whole store regardless of which entity was asked
/// about. The count that proves it: how many times the whole-corpus
/// `scan` ran to answer one `scan_entity` call, asked once against a
/// small corpus and once against a large one. A count that does not move
/// is the proof, and here it does not even have to move off zero.
#[tokio::test]
async fn scan_entity_does_not_pay_for_the_whole_corpus() {
    async fn scan_calls_for_one_entity(handles: &[&str]) -> usize {
        let inner = Arc::new(InMemoryMemory::booted());
        for handle in handles {
            let new = NewEntity::new(
                EntityId::person(*handle),
                format!("Person {handle}"),
                "user-named",
            );
            match inner.add_entity(new.clone()).await.expect("add ok") {
                Guarded::Written(_) => {}
                Guarded::Blocked {
                    attempted,
                    candidates,
                } => {
                    let token =
                        jojobot_domain::memory::guard::override_token(&attempted, &candidates);
                    inner
                        .add_entity(NewEntity {
                            override_token: Some(token),
                            override_reason: Some("a fixture sharing a name".into()),
                            ..new
                        })
                        .await
                        .expect("add ok")
                        .written()
                        .expect("the override clears the near-miss");
                }
            }
        }
        let scan_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = Arc::new(CountingScan {
            inner,
            scan_calls: scan_calls.clone(),
        });
        let folded = Folded::new(counted);
        folded
            .scan_entity(&EntityId::person("person:alpha"))
            .await
            .expect("scan_entity ok");
        scan_calls.load(std::sync::atomic::Ordering::SeqCst)
    }

    const SMALL: &[&str] = &["person:alpha", "person:beta"];
    const LARGE: &[&str] = &[
        "person:alpha",
        "person:beta",
        "person:gamma",
        "person:kappa",
        "person:delta",
        "person:epsilon",
        "person:zeta",
        "person:eta",
        "person:theta",
        "person:iota",
    ];
    let small = scan_calls_for_one_entity(SMALL).await;
    let large = scan_calls_for_one_entity(LARGE).await;
    assert_eq!(
        small,
        large,
        "one write's store work must not depend on how many things are already stored: \
         {small} whole-corpus scans at {} entities, {large} at {}",
        SMALL.len(),
        LARGE.len(),
    );
    assert_eq!(
        small, 0,
        "scan_entity must not read the whole corpus to answer about one entity: {small} calls",
    );
}

/// **Two writes reach the image in the order they reached the store.**
///
/// A writes `due_on = A`; its re-read stalls. While it is stalled, B tries
/// to write `due_on = B`. B must wait for A's write and A's re-read to
/// finish: if it ran through, A's older snapshot could land after B's and
/// the image would serve the earlier value for ever, with the store
/// holding the later one. Only after A is released does B write, so the
/// store and the image both end at `B`.
#[tokio::test]
async fn a_second_write_waits_for_the_first_ones_reread_so_the_image_ends_at_the_later_value() {
    let inner = Arc::new(InMemoryMemory::booted());
    inner
        .add_entity(NewEntity::new(alpha(), "Alpha", "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");

    let stalled = Arc::new(tokio::sync::Notify::new());
    let released = Arc::new(tokio::sync::Notify::new());
    let double = Arc::new(StallFirstVersionedRead {
        inner: inner.clone(),
        written: std::sync::atomic::AtomicU64::new(0),
        stalled: stalled.clone(),
        released: released.clone(),
        first: std::sync::atomic::AtomicBool::new(false),
    });
    let folded = Arc::new(Folded::new(double.clone()));
    folded.rebuild().await.expect("the image fills");
    double
        .first
        .store(true, std::sync::atomic::Ordering::SeqCst);

    let a_folded = folded.clone();
    let a_task = tokio::spawn(async move {
        a_folded
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "A".to_string())]),
                ..NewFact::about(alpha(), "A writes first", date(2026, 9, 1))
            }))
            .await
    });

    // A's write has landed and its re-read is holding a snapshot open.
    stalled.notified().await;

    let b_folded = folded.clone();
    let b_task = tokio::spawn(async move {
        b_folded
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "B".to_string())]),
                ..NewFact::about(
                    alpha(),
                    "B writes second, while A is stalled",
                    date(2026, 9, 2),
                )
            }))
            .await
    });
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    assert!(
        !b_task.is_finished(),
        "B ran through while A's re-read was still open"
    );
    assert_eq!(
        inner
            .fields(&alpha())
            .await
            .expect("fields ok")
            .get("due_on"),
        Some(&"A".to_string()),
        "B has not reached the store while A holds the gate"
    );

    released.notify_one();
    a_task
        .await
        .expect("the task did not panic")
        .expect("capture ok")
        .written()
        .expect("not blocked");
    b_task
        .await
        .expect("the task did not panic")
        .expect("capture ok")
        .written()
        .expect("not blocked");

    assert_eq!(
        folded
            .fields(&alpha())
            .await
            .expect("fields ok")
            .get("due_on"),
        Some(&"B".to_string()),
        "the image ends at the later write"
    );
    assert_eq!(
        inner
            .fields(&alpha())
            .await
            .expect("fields ok")
            .get("due_on"),
        Some(&"B".to_string()),
        "and so does the store"
    );
}

/// **Declaring a fold changes what already-cached writes answer, not
/// only writes made afterwards.** `folded_fields` interprets a thing's
/// existing writes against the CURRENT declarations every time it runs;
/// a cache that answered from a snapshot taken under the old
/// declaration would go on answering wrongly for ever, since nothing
/// ever touches this entity's own key again to trigger a normal
/// refresh.
///
/// The worked example: write, write, read (newest-write-wins, since
/// nothing is declared), declare `laps` a counter, read again (the SAME
/// two writes now sum).
#[tokio::test]
async fn declaring_a_counter_changes_what_the_same_two_writes_already_fold_to() {
    let inner = Arc::new(InMemoryMemory::booted());
    let folded = Folded::new(inner);
    seeded(&folded, &alpha(), "Alpha").await;

    folded
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: BTreeMap::from([("laps".to_string(), "1".to_string())]),
            ..NewFact::about(alpha(), "lap one", date(2026, 9, 1))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    folded
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: BTreeMap::from([("laps".to_string(), "1".to_string())]),
            ..NewFact::about(alpha(), "lap two", date(2026, 9, 2))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    assert_eq!(
        folded
            .fields(&alpha())
            .await
            .expect("fields ok")
            .get("laps"),
        Some(&"1".to_string()),
        "undeclared, laps folds newest-write-wins: the second write alone",
    );

    folded
        .declare_type(
            types::DeclaredType::new("running", vec![types::Field::summing("laps")]),
            None,
        )
        .await
        .expect("declare ok");

    assert_eq!(
        folded
            .fields(&alpha())
            .await
            .expect("fields ok")
            .get("laps"),
        Some(&"2".to_string()),
        "declared a counter, the same two writes now sum to two",
    );
}

/// **The store resolves a stale handle through its own rename history;
/// the cache answered first, and answered wrong.** Read once under a
/// handle, rename, write again under the new one, then ask by the OLD
/// handle again: the answer must be what the thing holds NOW, not a
/// photograph of what it held the moment before the rename — with no
/// error and no notice, since the miss this relies on is silent by
/// design.
#[tokio::test]
async fn a_rename_drops_the_old_handles_entry_so_it_answers_current_data() {
    let inner = Arc::new(InMemoryMemory::booted());
    let folded = Folded::new(inner);
    seeded(&folded, &alpha(), "Alpha").await;

    folded
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: BTreeMap::from([("due_on".to_string(), "before".to_string())]),
            ..NewFact::about(alpha(), "before the rename", date(2026, 9, 1))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    // Read under the old handle before the rename — the same read a
    // caller holding it would make, and what populates the cache entry
    // this fix has to drop.
    assert_eq!(
        folded
            .fields(&alpha())
            .await
            .expect("fields ok")
            .get("due_on"),
        Some(&"before".to_string()),
    );

    let gamma = EntityId::person("person:gamma");
    folded
        .rename_entity(
            &alpha(),
            &gamma,
            None,
            date(2026, 9, 2),
            jojobot_domain::memory::Override::none(),
            None,
        )
        .await
        .expect("rename ok")
        .written()
        .expect("not blocked");

    // A write under the CURRENT handle, after the rename.
    folded
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: BTreeMap::from([("due_on".to_string(), "after".to_string())]),
            ..NewFact::about(gamma.clone(), "after the rename", date(2026, 9, 3))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

    assert_eq!(
        folded
            .fields(&alpha())
            .await
            .expect("fields ok")
            .get("due_on"),
        Some(&"after".to_string()),
        "the old handle must resolve through history to CURRENT data, not the snapshot the \
         cache took before the rename",
    );
}

/// **A rename leaves no cached copy of a field that names the old handle.**
/// A field naming a thing is served under the handle the thing answers to
/// now, and the fold caches what it served, so the copy cached for the thing
/// that holds the field still says the old spelling after the rename. The
/// fold drops it, which is why a read of a thing's fields needs nothing
/// further to resolve a reference.
#[tokio::test]
async fn a_rename_drops_the_cached_fields_that_name_the_old_handle() {
    let inner = Arc::new(InMemoryMemory::booted());
    let folded = Folded::new(inner.clone());
    folded
        .declare_type(
            DeclaredType::new(
                "fold-pointing",
                vec![Field::new("friend", ValueType::Reference)],
            ),
            None,
        )
        .await
        .expect("the type is declared");
    let gamma = EntityId::person("person:gamma");
    let holder = EntityId::person("person:kenny");
    for (id, name) in [(&alpha(), "Alpha"), (&holder, "Kenny")] {
        seeded(&folded, id, name).await;
    }
    folded
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: BTreeMap::from([("friend".to_string(), alpha().to_string())]),
            ..NewFact::about(holder.clone(), "has a friend", date(2026, 9, 1))
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");
    // The holder is read once, so it has a cached copy to be dropped.
    assert_eq!(
        folded
            .fields(&holder)
            .await
            .expect("fields ok")
            .get("friend"),
        Some(&alpha().to_string()),
    );

    folded
        .rename_entity(
            &alpha(),
            &gamma,
            None,
            date(2026, 9, 2),
            jojobot_domain::memory::Override::none(),
            None,
        )
        .await
        .expect("rename ok")
        .written()
        .expect("not blocked");

    assert_eq!(
        folded
            .fields(&holder)
            .await
            .expect("fields ok")
            .get("friend"),
        Some(&gamma.to_string()),
        "the holder's cached copy still names the handle the friend wore before the rename"
    );
}
