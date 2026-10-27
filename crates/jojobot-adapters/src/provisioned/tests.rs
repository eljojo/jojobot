use super::overlay_contract::{shipped_record, supplied, the_operators_own_view};
use super::*;
use jojobot_domain::memory::owned::Provision;
use jojobot_domain::memory::testing::InMemoryMemory;

const CORE: &str = "You answer in one line unless asked otherwise.";

async fn provisioned() -> (Provisioned<InMemoryMemory>, EntityId) {
    let bot = EntityId("bot:gamma".into());
    let store = InMemoryMemory::booted();
    store
        .add_entity(NewEntity::new(bot.clone(), "Gamma", "jojobot"))
        .await
        .expect("the identity is created")
        .written()
        .expect("an empty board blocks nothing");
    let over = Provisioned::new(
        store,
        Provisions::new(vec![Provision::prose(bot.clone(), CORE)]),
    );
    (over, bot)
}

/// **Composing prose and naming echoed defaults read no stored row.** The
/// fold above this layer forwards both, and they are no reads of the store:
/// the first joins what the build ships to the half the instance wrote, the
/// second compares the caller's fields with what the build ships. The store
/// beneath is aimed at a port nothing listens on, so any read of it fails,
/// and both still answer. **Paired**: a read of the store's rows fails on the
/// same store, so the answers above were not given by a store that was there.
#[tokio::test]
async fn composing_prose_and_naming_echoed_defaults_need_no_store() {
    use crate::dolt::memory::DoltMemory;
    use crate::fold::Folded;
    let nowhere = sqlx::mysql::MySqlPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy("mysql://root@127.0.0.1:1/nothing-is-there")
        .expect("a pool that will never answer");
    let bot = EntityId("bot:gamma".into());
    let other = EntityId("bot:delta".into());
    let over = Folded::new(std::sync::Arc::new(Provisioned::new(
        DoltMemory::open(nowhere),
        Provisions::new(vec![
            Provision::prose(bot.clone(), CORE),
            Provision::field(bot.clone(), "one_liner", "The disposable implementer."),
        ]),
    )));

    let composed = over
        .composed_prose(&bot, "its own line")
        .await
        .expect("the prose the build ships is joined to the instance's, no store needed");
    assert!(
        composed.contains(CORE) && composed.contains("its own line"),
        "both halves are joined: {composed}"
    );
    assert_eq!(
        over.composed_prose(&other, " its own line ")
            .await
            .expect("a handle the build ships nothing for answers as the store's default does"),
        "its own line"
    );
    let sent = BTreeMap::from([(
        "one_liner".to_string(),
        "The disposable implementer.".to_string(),
    )]);
    assert_eq!(
        over.echoed_defaults(&bot, &sent).await,
        vec!["one_liner".to_string()],
        "the key equal to the build's default is named, no store needed"
    );
    assert!(
        over.echoed_defaults(&other, &sent).await.is_empty(),
        "a handle the build ships no default for names none"
    );
    assert!(
        over.list_entities(None).await.is_err(),
        "the store cannot be reached, so a read of its rows fails: the answers above did not \
         need it"
    );
}

/// The layer forwards which keys of a record the store holds off the thing.
#[tokio::test]
async fn the_layer_forwards_the_keys_a_switched_store_holds_off_a_thing() {
    let over = Provisioned::new(InMemoryMemory::booted(), Provisions::new(vec![]));
    jojobot_domain::memory::testing::contract::own_fields_stay_while_settings_and_computed_keys_reach(&over).await;
}

/// The layer forwards the writes of a key that stays on its record, with the
/// moment of each.
#[tokio::test]
async fn the_layer_forwards_the_writes_that_stay_on_their_record() {
    let over = Provisioned::new(InMemoryMemory::booted(), Provisions::new(vec![]));
    jojobot_domain::memory::testing::contract::a_key_that_stays_on_its_record_is_in_the_trails_with_its_moment(&over).await;
}

/// The prose a reader gets for that entity, through the read every reader
/// above this layer takes it from.
async fn read(store: &Provisioned<InMemoryMemory>, bot: &EntityId) -> String {
    store
        .scan_entity(bot)
        .await
        .expect("the scan reads")
        .expect("the entity is there")
        .prose
}

/// **A read carries what the build supplies, and the operator's own text
/// under it.**
///
/// Both halves out of one read: the build's alone passes on a layer that
/// throws the operator's text away, and the operator's alone passes on a
/// layer that resolves nothing.
#[tokio::test]
async fn a_read_carries_what_the_build_supplies_and_what_the_operator_wrote() {
    let (store, bot) = provisioned().await;
    store
        .set_prose(&bot, "Gamma also files the weekly note.", None)
        .await
        .expect("the operator's own layer lands");

    let read = read(&store, &bot).await;
    assert!(read.contains(CORE), "the build's half is there: {read}");
    assert!(
        read.contains("files the weekly note"),
        "…and the operator's is too: {read}",
    );
    assert!(
        read.find(CORE) < read.find("files the weekly note"),
        "…with the build's half first, because the second narrows it: {read}",
    );
}

/// **An entity the build supplies nothing for is untouched** — which is
/// every entity in the store but one.
///
/// The positive the case above rests on: without it, a layer that pasted
/// the same text onto everything would pass.
#[tokio::test]
async fn an_entity_the_build_supplies_nothing_for_reads_exactly_what_was_stored() {
    let (store, _) = provisioned().await;
    let other = EntityId("bot:delta".into());
    store
        .add_entity(NewEntity::new(other.clone(), "Delta", "jojobot"))
        .await
        .expect("a second identity")
        .written()
        .expect("an empty board blocks nothing");
    store
        .set_prose(&other, "Delta writes its own charter.", None)
        .await
        .expect("prose lands");

    assert_eq!(read(&store, &other).await, "Delta writes its own charter.");
}

/// **A bot nobody has written for still answers with what the build
/// supplies**, and with nothing bolted onto it.
#[tokio::test]
async fn an_unwritten_entity_answers_with_the_build_alone() {
    let (store, bot) = provisioned().await;
    assert_eq!(read(&store, &bot).await, CORE);
}

/// **THE READ-MODIFY-WRITE TRAP.** A caller reads the resolved value, adds
/// a line, sends it back — and the build's own words would become this
/// instance's, where they stop moving when the software does.
///
/// **Both halves in one read of the store**: the echoed write is refused,
/// and the caller's own text lands. Without the second, the case passes on
/// a layer that refuses every write there is.
#[tokio::test]
async fn sending_a_read_back_is_refused_and_the_callers_own_text_lands() {
    let (store, bot) = provisioned().await;
    let round_trip = format!(
        "{}\n\nAnd Gamma files the weekly note.",
        read(&store, &bot).await
    );

    let refused = store
        .set_prose(&bot, &round_trip, None)
        .await
        .expect_err("a write carrying the build's own half is refused");
    assert!(
        matches!(refused, MemoryError::RepeatsShipped),
        "the refusal says what is wrong rather than failing the store: {refused:?}",
    );

    store
        .set_prose(&bot, "Gamma files the weekly note.", None)
        .await
        .expect("the same intent, sent as what is being added, lands");

    let read = read(&store, &bot).await;
    assert!(
        read.contains("files the weekly note"),
        "the caller's own text is stored: {read}",
    );
    assert_eq!(
        read.matches(CORE).count(),
        1,
        "…and the build's half is there exactly once, not stored a second time: {read}",
    );
}

/// **An operator's extension survives an upgrade that changes what the
/// build supplies** — paired, in one read, with the build's half having
/// actually changed.
///
/// Without the pairing this passes on a build where nothing upgrades: the
/// extension surviving is only interesting if the half beside it moved.
#[tokio::test]
async fn an_extension_survives_an_upgrade_that_changes_what_the_build_ships() {
    let (store, bot) = provisioned().await;
    store
        .set_prose(&bot, "Gamma files the weekly note.", None)
        .await
        .expect("the operator writes their own layer");

    // The upgrade: a later build, over the same store, supplying different
    // text at the same address. Nothing migrates and nothing runs.
    const IMPROVED: &str = "You answer in one line, and you say when you are unsure.";
    let upgraded = Provisioned::new(
        store.inner,
        Provisions::new(vec![Provision::prose(bot.clone(), IMPROVED)]),
    );

    let read = read(&upgraded, &bot).await;
    assert!(
        read.contains(IMPROVED),
        "the later build's half reached an instance that was already running: {read}",
    );
    assert!(
        !read.contains(CORE),
        "…and the older build's half is gone rather than accumulated: {read}",
    );
    assert!(
        read.contains("files the weekly note"),
        "…while what the operator wrote is exactly where they left it: {read}",
    );
}

/// **Changing a supplied record's source is refused the way creating one is,
/// and the refusal says the record is there.** (Its name and aliases are the
/// operator's to set: see `the_operator_names_a_supplied_record_and_every_read_shows_it`.)
///
/// `add_entity` on a supplied handle comes back blocked, naming the
/// collision. `update_entity` reached the inner store, which holds no row
/// for that handle and answered that the handle names nothing — so one path
/// resolved the record, one blocked on it, and a third could not see it.
/// **A caller was told a thing does not exist while every read returns it**,
/// and the candidates offered came from an index the record is not in.
///
/// **Both halves in one read.** The supplied record is refused with the
/// collision named, and the identical call on a STORED entity still
/// renames it — without the second, a decorator that blocked every rename
/// would pass the first.
#[tokio::test]
async fn changing_the_source_of_a_supplied_record_is_blocked_and_a_stored_one_still_renames() {
    let store = InMemoryMemory::booted();
    store
        .add_entity(jojobot_domain::memory::NewEntity::new(
            EntityId("view:my-week".into()),
            "The week",
            "the operator",
        ))
        .await
        .expect("the operator's own view is created")
        .written()
        .expect("an empty board blocks nothing");
    let over = Provisioned::new(store, Provisions::new(vec![shipped_record("loops")]));

    let renamed = over
        .update_entity(
            &EntityId("view:loops".into()),
            jojobot_domain::memory::EntityPatch {
                source: Some("the operator".into()),
                ..Default::default()
            },
        )
        .await;

    match renamed {
        Ok(jojobot_domain::memory::Guarded::Blocked {
            attempted,
            candidates,
        }) => {
            assert_eq!(attempted, EntityId("view:loops".into()));
            assert!(
                candidates
                    .iter()
                    .any(|c| c.handle == EntityId("view:loops".into())),
                "the refusal names the record it collided with: {candidates:?}",
            );
        }
        other => panic!(
            "a supplied record is there, so changing its source is a refusal that names \
             it — not an answer that it does not exist: {other:?}"
        ),
    }

    let stored = over
        .update_entity(
            &EntityId("view:my-week".into()),
            jojobot_domain::memory::EntityPatch {
                name: Some("The week ahead".into()),
                ..Default::default()
            },
        )
        .await
        .expect("the operator's own record is theirs to rename");
    assert!(
        matches!(stored, jojobot_domain::memory::Guarded::Written(ref e) if e.name == "The week ahead"),
        "…and a stored record still renames exactly as it did: {stored:?}",
    );
}

/// **An edit that writes a claim and names nothing of the record itself is a
/// capture on a supplied record**, and a source beside it refuses the whole
/// call.
///
/// Three reads in one case. The claim lands and the supplied record is what
/// comes back, with what the build ships still under what was set; the same
/// call with a source refuses and leaves the claim unwritten, so the pair is
/// whole or not at all; and a claim about another record than the one edited
/// is the malformed call it is on a stored record. Without the second, a
/// layer that ignored the patch and wrote the claim would pass the first.
#[tokio::test]
async fn an_edit_that_only_sets_lands_on_a_supplied_record_and_one_that_changes_its_source_does_not()
 {
    let supplied = Provisions::new(vec![shipped_record("loops")]);
    let over = Provisioned::new(InMemoryMemory::booted().knowing(supplied.clone()), supplied);
    let id = EntityId("view:loops".into());
    let claim = |content: &str, key: &str| {
        let mut fact = NewFact::about(id.clone(), content, Date::constant(2026, 10, 12));
        fact.fields
            .insert(key.to_string(), "kept on top".to_string());
        fact.sets.insert(key.to_string());
        fact
    };

    let landed = over
        .update_entity_with_claim(&id, EntityPatch::default(), claim("a note", "note"))
        .await
        .expect("a sets-only edit of a supplied record answers");
    match landed {
        Guarded::Written((entity, _)) => assert_eq!(entity.id, id),
        other => panic!("a sets-only edit lands on a supplied record: {other:?}"),
    }
    let held = over.fields(&id).await.expect("fields read");
    assert_eq!(
        held.get("note").map(String::as_str),
        Some("kept on top"),
        "{held:?}"
    );
    assert_eq!(
        held.get("selects").map(String::as_str),
        Some("rhythm"),
        "what the build ships is still under it: {held:?}",
    );

    let renamed = over
        .update_entity_with_claim(
            &id,
            EntityPatch {
                source: Some("the operator".into()),
                ..Default::default()
            },
            claim("another note", "other"),
        )
        .await
        .expect("the refusal is an answer");
    assert!(
        matches!(renamed, Guarded::Blocked { ref attempted, .. } if attempted == &id),
        "a source beside the sets refuses the call: {renamed:?}",
    );
    let held = over.fields(&id).await.expect("fields read");
    assert!(
        !held.contains_key("other"),
        "a refused edit writes none of its claim: {held:?}",
    );

    let elsewhere = over
        .update_entity_with_claim(
            &id,
            EntityPatch::default(),
            NewFact::about(
                EntityId("view:ready".into()),
                "about another record",
                Date::constant(2026, 10, 12),
            ),
        )
        .await;
    assert!(
        matches!(elsewhere, Err(MemoryError::InvalidFact(_))),
        "a claim about another record is a malformed call: {elsewhere:?}",
    );
}

/// **Two supplied records and an operator-owned thing, for the overlay cases.**
async fn with_two_supplied_and_a_stored_thing() -> Provisioned<InMemoryMemory> {
    let store = InMemoryMemory::booted().knowing(supplied());
    the_operators_own_view(&store).await;
    Provisioned::new(store, supplied())
}

/// **The operator names a supplied record and gives it more names, and every
/// read shows it.** One update, then the reads that name a record (the
/// listing, the document, the scan the index ingests and the fields), the
/// screen a new creation faces, and the claim that carries the overlay.
/// Paired with the record before the update, which is the build's own.
#[tokio::test]
async fn the_operator_names_a_supplied_record_and_every_read_shows_it() {
    let over = with_two_supplied_and_a_stored_thing().await;
    overlay_contract::the_operator_names_a_supplied_record_and_every_read_shows_it(&over).await;
}

/// **The operator's name faces the screen a rename does, and a refusal writes
/// nothing.** A name another thing answers to is blocked with the thing named,
/// and the record keeps the name it had. Source and crm are refused.
#[tokio::test]
async fn the_operators_name_faces_the_screen_and_source_and_crm_stay_the_builds() {
    let over = with_two_supplied_and_a_stored_thing().await;
    let id = EntityId("view:loops".into());
    let taken = over
        .update_entity(
            &id,
            EntityPatch {
                name: Some("The week".into()),
                ..Default::default()
            },
        )
        .await
        .expect("a refusal is an answer");
    let Guarded::Blocked { candidates, .. } = taken else {
        panic!("a name another thing answers to is blocked: {taken:?}");
    };
    assert!(
        candidates
            .iter()
            .any(|c| c.handle.as_str() == "view:my-week")
    );
    let after = over.list_entities(None).await.expect("lists");
    assert_eq!(
        after.iter().find(|e| e.id == id).map(|e| e.name.as_str()),
        Some("The loops view"),
        "a refused name leaves the record as it was"
    );
    assert!(over.recall(&id).await.expect("recall").is_empty());

    for patch in [
        EntityPatch {
            source: Some("the operator".into()),
            ..Default::default()
        },
        EntityPatch {
            crm: Some("card-1".into()),
            ..Default::default()
        },
    ] {
        let refused = over.update_entity(&id, patch).await.expect("answers");
        assert!(
            matches!(refused, Guarded::Blocked { .. }),
            "source and crm stay the build's: {refused:?}"
        );
    }
}

/// **The overlay's keys are written by the update that sets a name, and by
/// nothing else.** A capture that names one is refused, because a write that
/// set them directly would skip the screen.
#[tokio::test]
async fn a_caller_cannot_write_the_overlay_keys_around_the_screen() {
    let over = with_two_supplied_and_a_stored_thing().await;
    let id = EntityId("view:loops".into());
    let mut fact = NewFact::about(id.clone(), "a name", Date::constant(2026, 10, 12));
    fact.fields.insert(
        jojobot_domain::memory::owned::OVERLAY_NAME.to_string(),
        "Sneaked".to_string(),
    );
    fact.sets
        .insert(jojobot_domain::memory::owned::OVERLAY_NAME.to_string());
    let refused = over.capture(fact).await;
    assert!(
        matches!(refused, Err(MemoryError::InvalidFact(_))),
        "a capture setting the overlay's key is refused: {refused:?}"
    );
    let listed = over.list_entities(None).await.expect("lists");
    assert_eq!(
        listed.iter().find(|e| e.id == id).map(|e| e.name.as_str()),
        Some("The loops view")
    );
}

/// **Folding a supplied record away, or into it, is refused before the
/// store is ever asked — the fourth sibling merge was missing.**
///
/// `add_entity`, `update_entity` and `rename_entity` all short-circuit on
/// what the build supplies; `merge` forwarded straight through. Checked
/// on both sides, because a supplied handle is equally untouchable as
/// the duplicate and as the survivor.
///
/// **Paired with a genuinely stored fold, which still lands** — without
/// it, a decorator that refused every fold would pass the first two
/// assertions identically.
#[tokio::test]
async fn merging_a_supplied_record_is_refused_and_a_stored_pair_still_merges() {
    let store = InMemoryMemory::booted();
    for id in ["thing:merge-spare", "thing:merge-kept"] {
        store
            .add_entity(jojobot_domain::memory::NewEntity::new(
                EntityId(id.into()),
                id.split_once(':').map_or(id, |(_, slug)| slug),
                "the operator",
            ))
            .await
            .expect("the operator's own thing is created")
            .written()
            .expect("an empty board blocks nothing");
    }
    let over = Provisioned::new(store, Provisions::new(vec![shipped_record("loops")]));

    let as_folded = over
        .merge(
            &EntityId("view:loops".into()),
            &EntityId("thing:merge-kept".into()),
            None,
            Date::constant(2026, 6, 13),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect_err("folding a supplied record away must be refused");
    assert!(
        matches!(&as_folded, MemoryError::SuppliedHandle { attempted } if attempted == "view:loops"),
        "the refusal says the supplied handle exists and has no row to move: {as_folded:?}",
    );

    let as_survivor = over
        .merge(
            &EntityId("thing:merge-spare".into()),
            &EntityId("view:loops".into()),
            None,
            Date::constant(2026, 6, 13),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect_err("folding a stored thing into a supplied record must be refused");
    assert!(
        matches!(&as_survivor, MemoryError::SuppliedHandle { attempted } if attempted == "view:loops"),
        "the refusal says the same when the supplied handle is the survivor: {as_survivor:?}",
    );

    over.merge(
        &EntityId("thing:merge-spare".into()),
        &EntityId("thing:merge-kept".into()),
        None,
        Date::constant(2026, 6, 13),
        &EntityId("bot:sigma".into()),
        None,
    )
    .await
    .expect("a fold of two stored things is still the decorator's to pass through");
}

/// **A record the build ships whole, with the keys a view's build inputs
/// live under** — what a stored row at the same handle must never read
/// back.
fn shipped_view_with_inputs(slug: &str) -> jojobot_domain::memory::owned::Provision {
    let id = EntityId::new(EntityKind::VIEW, slug);
    jojobot_domain::memory::owned::Provision::record(
        Entity {
            folded_names: Vec::new(),
            id: id.clone(),
            kind: EntityKind::VIEW,
            name: format!("The {slug} view"),
            aliases: Vec::new(),
            source: "jojobot".into(),
            crm: None,
            parent: Some(EntityId("topic:all-rules".into())),
            boot: Default::default(),
            merged_into: None,
            badge: None,
            archived: None,
        },
        BTreeMap::from([
            ("selects".to_string(), "rhythm".to_string()),
            ("inputs".to_string(), "project".to_string()),
            ("order".to_string(), "position".to_string()),
            ("shows_keys".to_string(), "status,owner".to_string()),
        ]),
    )
}

/// **The operator's row at a handle the build also ships**: a stored
/// `view:loops` holding only `selects` and `where`, over a build that
/// ships that handle with inputs, an order and a key list. Every store
/// the layer wraps can hold such a row (rule 508).
async fn stored_over_supplied() -> Provisioned<InMemoryMemory> {
    let store = InMemoryMemory::booted();
    let board = EntityId("view:loops".into());
    store
        .add_entity(NewEntity::new(board.clone(), "Mine", "user-named"))
        .await
        .expect("the operator's row is created")
        .written()
        .expect("nothing shipped it yet");
    store
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: [
                ("selects".to_string(), "work".to_string()),
                ("where".to_string(), "status = now".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(
                board,
                "what the operator selects",
                Date::constant(2026, 6, 2),
            )
        }))
        .await
        .expect("the operator's keys land")
        .written()
        .expect("nothing blocks them");
    for spare in ["view:due", "view:plan"] {
        store
            .add_entity(NewEntity::new(EntityId(spare.into()), spare, "user-named"))
            .await
            .expect("the operator's other row is created")
            .written()
            .expect("nothing blocks it");
    }
    Provisioned::new(
        store,
        Provisions::new(vec![shipped_view_with_inputs("loops")]),
    )
}

/// **A stored row at a supplied handle reads back only the keys it
/// holds.** Paired with a supplied record that has no stored row, which
/// keeps every default: without the second, a layer that dropped the
/// build's keys everywhere would pass the first.
#[tokio::test]
async fn a_stored_row_at_a_supplied_handle_reads_back_only_its_own_keys() {
    let over = stored_over_supplied().await;
    let board = EntityId("view:loops".into());

    let keys = |m: BTreeMap<String, String>| m.keys().cloned().collect::<Vec<_>>();
    let read = over.fields(&board).await.expect("fields read");
    assert_eq!(keys(read.clone()), ["selects", "where"], "{read:?}");
    assert_eq!(read.get("selects").map(String::as_str), Some("work"));
    let (versioned, _) = over.fields_versioned(&board).await.expect("fields read");
    assert_eq!(keys(versioned), ["selects", "where"]);

    let echoed = over
        .echoed_defaults(
            &board,
            &BTreeMap::from([("inputs".to_string(), "project".to_string())]),
        )
        .await;
    assert!(
        echoed.is_empty(),
        "no default sits under a stored row, so none can be echoed: {echoed:?}"
    );

    let supplies = Provisions::new(vec![shipped_view_with_inputs("loops")]);
    let no_row = Provisioned::new(InMemoryMemory::booted().knowing(supplies.clone()), supplies);
    let supplied = no_row.fields(&board).await.expect("fields read");
    assert_eq!(
        keys(supplied),
        ["inputs", "order", "selects", "shows_keys"],
        "a supplied record with no stored row keeps every default",
    );
}

/// **A stored row at a supplied handle takes the writes a stored row
/// takes** — an edit, a rename, an archive and its restore — while the
/// same verbs on a supplied record with no stored row are still refused.
#[tokio::test]
async fn a_stored_row_at_a_supplied_handle_takes_edit_rename_archive_and_restore() {
    let over = stored_over_supplied().await;
    let board = EntityId("view:loops".into());

    let edited = over
        .update_entity(
            &board,
            EntityPatch {
                name: Some("Mine, edited".into()),
                ..Default::default()
            },
        )
        .await
        .expect("the edit reaches the store");
    assert!(
        matches!(edited, Guarded::Written(ref e) if e.name == "Mine, edited"),
        "{edited:?}"
    );

    let archived = over
        .archive_entity(&board, "retired", None)
        .await
        .expect("the archive reaches the store");
    assert!(archived.archived.is_some(), "{archived:?}");
    let (restored, _) = over
        .restore_entity(&board, None)
        .await
        .expect("the restore reaches the store");
    assert!(restored.archived.is_none(), "{restored:?}");

    let renamed = over
        .rename_entity(
            &board,
            &EntityId("view:urgent-things".into()),
            None,
            Date::constant(2026, 6, 13),
            jojobot_domain::memory::Override::none(),
            None,
        )
        .await
        .expect("the rename reaches the store");
    assert!(
        matches!(renamed, Guarded::Written(ref e) if e.id == EntityId("view:urgent-things".into())),
        "{renamed:?}"
    );

    let still_supplied = Provisioned::new(
        InMemoryMemory::booted(),
        Provisions::new(vec![shipped_view_with_inputs("loops")]),
    );
    assert!(matches!(
        still_supplied.archive_entity(&board, "x", None).await,
        Err(MemoryError::SuppliedHandle { .. })
    ));
    assert!(matches!(
        still_supplied.restore_entity(&board, None).await,
        Err(MemoryError::SuppliedHandle { .. })
    ));
}

/// **A stored row at a supplied handle merges as either side**, and an
/// edit that carries a claim and a new name reaches the store too.
#[tokio::test]
async fn a_stored_row_at_a_supplied_handle_merges_and_takes_a_named_edit_with_a_claim() {
    let over = stored_over_supplied().await;
    let board = EntityId("view:loops".into());
    let caller = EntityId("bot:sigma".into());

    over.merge(
        &EntityId("view:due".into()),
        &board,
        None,
        Date::constant(2026, 6, 13),
        &caller,
        None,
    )
    .await
    .expect("the stored row is a survivor like any other");
    over.merge(
        &board,
        &EntityId("view:plan".into()),
        None,
        Date::constant(2026, 6, 13),
        &caller,
        None,
    )
    .await
    .expect("the stored row folds away like any other");

    let over = stored_over_supplied().await;
    let edited = over
        .update_entity_with_claim(
            &board,
            EntityPatch {
                name: Some("Mine again".into()),
                ..Default::default()
            },
            NewFact::about(
                board.clone(),
                "renamed by hand",
                Date::constant(2026, 6, 14),
            ),
        )
        .await
        .expect("the paired edit reaches the store");
    assert!(
        matches!(edited, Guarded::Written((ref e, _)) if e.name == "Mine again"),
        "{edited:?}"
    );
}

/// **A shipped default for one key on a row the store already holds**,
/// under what the operator wrote — same rule prose narrows by, on a
/// single field instead of a paragraph.
///
/// The missing shape `Provision::record` cannot fill: that one is for an
/// address the store holds nothing of, and `bot:assistant` (and any
/// other real identity) always has a row. This is the mechanism
/// `orientation/charter.rs`'s comment says does not exist yet.
///
/// **Both halves in one read**: the default sits where the operator
/// wrote nothing, and it stops sitting there — replaced, not
/// accumulated — the moment the operator writes their own value for the
/// same key. Without the second half, a decorator that just always
/// showed the default would pass the first half alone.
#[tokio::test]
async fn a_supplied_field_default_sits_under_what_the_operator_wrote() {
    let store = InMemoryMemory::booted();
    let bot = EntityId("bot:gamma".into());
    store
        .add_entity(NewEntity::new(bot.clone(), "Gamma", "jojobot"))
        .await
        .expect("the identity is created")
        .written()
        .expect("an empty board blocks nothing");
    let over = Provisioned::new(
        store,
        Provisions::new(vec![Provision::field(
            bot.clone(),
            "one_liner",
            "The disposable implementer.",
        )]),
    );

    let before = over.fields(&bot).await.expect("fields read");
    assert_eq!(
        before.get("one_liner").map(String::as_str),
        Some("The disposable implementer."),
        "the shipped default answers when the operator wrote nothing: {before:?}",
    );

    over.inner
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: [(
                "one_liner".to_string(),
                "Files the weekly note.".to_string(),
            )]
            .into_iter()
            .collect(),
            ..NewFact::about(
                bot.clone(),
                "gamma's own one-liner",
                Date::constant(2026, 4, 18),
            )
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("nothing collides");

    let after = over.fields(&bot).await.expect("fields read");
    assert_eq!(
        after.get("one_liner").map(String::as_str),
        Some("Files the weekly note."),
        "the operator's own write replaces the default rather than sitting beside it: {after:?}",
    );
}

/// The fields a thing's one scan and the listing of every scan carry.
async fn scanned_fields(
    over: &Provisioned<InMemoryMemory>,
    bot: &EntityId,
) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let one = over
        .scan_entity(bot)
        .await
        .expect("the entity scans")
        .expect("the thing is there");
    let listed = over
        .scan()
        .await
        .expect("everything scans")
        .into_iter()
        .find(|doc| doc.entity.as_ref().is_some_and(|e| &e.id == bot))
        .expect("the thing is listed");
    (one.fields, listed.fields)
}

/// **A scan carries the shipped default the way a read of the fields does.**
/// The graph takes a thing's fields from its scan, so a default that only the
/// field read folds in goes missing from every answer the graph gives: the
/// shipped one-liner and manager of a bot the store holds vanished together.
/// The default answers where the operator wrote nothing, in both scans, and
/// the operator's own write replaces it.
#[tokio::test]
async fn a_scan_carries_a_supplied_field_default_under_what_the_operator_wrote() {
    let store = InMemoryMemory::booted();
    let bot = EntityId("bot:gamma".into());
    store
        .add_entity(NewEntity::new(bot.clone(), "Gamma", "jojobot"))
        .await
        .expect("the identity is created")
        .written()
        .expect("an empty board blocks nothing");
    let over = Provisioned::new(
        store,
        Provisions::new(vec![Provision::field(
            bot.clone(),
            "one_liner",
            "The disposable implementer.",
        )]),
    );
    let (one, listed) = scanned_fields(&over, &bot).await;
    for fields in [&one, &listed] {
        assert_eq!(
            fields.get("one_liner").map(String::as_str),
            Some("The disposable implementer."),
            "the shipped default is in the scan: {fields:?}",
        );
    }

    over.inner
        .capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: [(
                "one_liner".to_string(),
                "Files the weekly note.".to_string(),
            )]
            .into_iter()
            .collect(),
            ..NewFact::about(
                bot.clone(),
                "gamma's own one-liner",
                Date::constant(2026, 4, 18),
            )
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("nothing collides");
    let (one, listed) = scanned_fields(&over, &bot).await;
    for fields in [&one, &listed] {
        assert_eq!(
            fields.get("one_liner").map(String::as_str),
            Some("Files the weekly note."),
            "the operator's write replaces the default in the scan: {fields:?}",
        );
    }
}

/// **The three answers an echoed default has to keep apart, in one read
/// of the store**: a written value equal to today's default is named and
/// then frozen at exactly what was written, through an upgrade that
/// changes the default; a key nobody ever wrote keeps following the
/// default wherever it moves; and a real customisation — a value that
/// never matched any default — is never named and is unaffected either
/// way.
///
/// All three from one setup, because each covers how the others pass on
/// a build that treats every field alike: only the first would pass on a
/// build that names every key; only the second on a build that freezes
/// everything at capture time; only the third on a build that names
/// every write, customisation included.
#[tokio::test]
async fn an_echoed_default_is_named_and_frozen_while_an_unwritten_key_still_follows_it() {
    let store = InMemoryMemory::booted();
    let bot = EntityId("bot:gamma".into());
    store
        .add_entity(NewEntity::new(bot.clone(), "Gamma", "jojobot"))
        .await
        .expect("the identity is created")
        .written()
        .expect("an empty board blocks nothing");
    let over = Provisioned::new(
        store,
        Provisions::new(vec![
            Provision::field(bot.clone(), "one_liner", "The disposable implementer."),
            Provision::field(bot.clone(), "boot", "on-demand"),
            Provision::field(bot.clone(), "role", "dev"),
        ]),
    );

    // The caller sends one_liner equal to today's default (the echo,
    // whether deliberate or an accidental round trip) and boot as a real
    // customisation that happens to differ from today's default.
    let sent = BTreeMap::from([
        (
            "one_liner".to_string(),
            "The disposable implementer.".to_string(),
        ),
        ("boot".to_string(), "always-on".to_string()),
    ]);
    let echoed = over.echoed_defaults(&bot, &sent).await;
    assert_eq!(
        echoed,
        vec!["one_liner".to_string()],
        "only the key that equals today's default is named, never the customisation: \
         {echoed:?}",
    );

    over.capture(jojobot_domain::memory::testing::contract::state(NewFact {
        fields: sent,
        ..NewFact::about(
            bot.clone(),
            "gamma's own fields, one of them a deliberate match on today's default",
            Date::constant(2026, 4, 18),
        )
    }))
    .await
    .expect("capture ok")
    .written()
    .expect("nothing collides");

    // The upgrade: a later build, over the same store, shipping different
    // defaults for all three keys. Nothing migrates and nothing runs.
    let upgraded = Provisioned::new(
        over.inner,
        Provisions::new(vec![
            Provision::field(bot.clone(), "one_liner", "An upgraded one-liner."),
            Provision::field(bot.clone(), "boot", "always-on-by-default"),
            Provision::field(bot.clone(), "role", "senior-dev"),
        ]),
    );

    let after = upgraded.fields(&bot).await.expect("fields read");
    assert_eq!(
        after.get("one_liner").map(String::as_str),
        Some("The disposable implementer."),
        "the echoed value stays exactly what was written, through an upgrade that changed \
         the default: {after:?}",
    );
    assert_eq!(
        after.get("boot").map(String::as_str),
        Some("always-on"),
        "the real customisation is unaffected by the upgrade: {after:?}",
    );
    assert_eq!(
        after.get("role").map(String::as_str),
        Some("senior-dev"),
        "a key nobody ever wrote keeps following the default, wherever it moves: {after:?}",
    );

    // The comparison is live, not a memory taken at capture time: sending
    // the ONE echoed value again after the upgrade no longer names it,
    // because it no longer equals what this build ships today.
    let sent_again = BTreeMap::from([(
        "one_liner".to_string(),
        "The disposable implementer.".to_string(),
    )]);
    let echoed_after_upgrade = upgraded.echoed_defaults(&bot, &sent_again).await;
    assert!(
        echoed_after_upgrade.is_empty(),
        "a value that no longer matches any current default must not be named: \
         {echoed_after_upgrade:?}",
    );
}

/// **The same echo, over a whole shipped record's own fields — not only a
/// per-key [`Provision::field`] default.** A `Supplies::Record` carries
/// its own fields map, folded in at [`fields`](Provisioned::fields)'s own
/// starting point exactly as a `Field` default is; `echoed_defaults`
/// shares that same fold now, through `unwritten_defaults`, so it must
/// see this shape too.
///
/// Paired: an echo of the record's own value is named, and a
/// customisation of the same key is not — without the second half this
/// passes on a check that names every write to a supplied record's key.
#[tokio::test]
async fn an_echo_of_a_shipped_records_own_field_is_named_and_a_customisation_is_not() {
    let record = shipped_record("my-week");
    let id = EntityId("view:my-week".into());
    let store = InMemoryMemory::booted();
    let over = Provisioned::new(store, Provisions::new(vec![record]));

    let echoing = BTreeMap::from([("selects".to_string(), "rhythm".to_string())]);
    let echoed = over.echoed_defaults(&id, &echoing).await;
    assert_eq!(
        echoed,
        vec!["selects".to_string()],
        "a value equal to the shipped record's own field must be named: {echoed:?}",
    );

    let customised = BTreeMap::from([("selects".to_string(), "person".to_string())]);
    let not_echoed = over.echoed_defaults(&id, &customised).await;
    assert!(
        not_echoed.is_empty(),
        "a value that differs from the shipped record's own field must not be named: \
         {not_echoed:?}",
    );
}

/// **A record the build ships answers every read a stored one answers, and
/// the store holds nothing of it.**
///
/// Four reads because those are the four the graph query goes through: a
/// supplied record that answered three of them would be a thing you could
/// list and not read. **And the store is asked directly at the end** —
/// without that, a layer that quietly wrote the record down would pass, and
/// the instance would be frozen on the build that wrote it.
/// 🚨 **A claim may point at a record the build ships.**
///
/// A read resolves what the build supplies and the write guard read only
/// the stored rows, so the two halves disagreed about what exists: `recall`
/// answered for `view:loops` and a claim drawing an edge at it was refused
/// as an entity nobody knows. **And the way out the refusal offered made it
/// worse** — a caller doing what it said would mint a stored row for a name
/// the build already owns, which is the collision the guard exists to
/// prevent.
///
/// ⚠️ **The pair is that the edge LANDS and resolves**, not merely that the
/// refusal is gone: a build that waved the write through and dropped the
/// edge would pass a check for the absence of a block.
#[tokio::test]
async fn a_claim_may_point_at_a_record_the_build_ships() {
    let store = InMemoryMemory::booted();
    let supplied = Provisions::new(vec![shipped_record("loops")]);
    let over = Provisioned::new(store.knowing(supplied.clone()), supplied);
    over.add_entity(jojobot_domain::memory::NewEntity::new(
        EntityId::person("person:milhouse"),
        "Milhouse",
        "the operator",
    ))
    .await
    .expect("add_entity ok")
    .written()
    .expect("an empty board blocks nothing");

    let written = over
        .capture(NewFact {
            edge: Some(jojobot_domain::memory::Edge {
                shape: jojobot_domain::memory::EdgeShape::About,
                object: EntityId("view:loops".into()),
            }),
            ..NewFact::about(
                EntityId::person("person:milhouse"),
                "asked for that view twice this week",
                Date::constant(2026, 4, 18),
            )
        })
        .await
        .expect("capture ok")
        .written()
        .expect("a claim may point at a record the build ships");

    let edge = written
        .edge
        .expect("the edge landed rather than being dropped on the way in");
    assert_eq!(edge.object, EntityId("view:loops".into()));
    assert_eq!(edge.shape, jojobot_domain::memory::EdgeShape::About);
}

#[tokio::test]
async fn a_record_the_build_ships_answers_like_a_stored_one_and_is_stored_nowhere() {
    // **Both halves told the same set**, as the binary wires it: the store
    // below has to see the supplied record when its own gate asks what
    // exists, and a fixture telling only the layer above is a fixture
    // poorer than the deployment it stands for.
    let supplied = Provisions::new(vec![shipped_record("loops")]);
    let over = Provisioned::new(InMemoryMemory::booted().knowing(supplied.clone()), supplied);
    let id = EntityId("view:loops".into());

    assert!(
        over.list_entities(Some(EntityKind::VIEW))
            .await
            .expect("the listing reads")
            .iter()
            .any(|e| e.id == id),
        "a supplied record is in the listing its kind answers",
    );
    assert_eq!(
        over.fields(&id)
            .await
            .expect("the keys read")
            .get("selects"),
        Some(&"rhythm".to_string()),
        "…and it carries the keys the build gave it",
    );
    assert_eq!(
        over.scan_entity(&id)
            .await
            .expect("the scan reads")
            .expect("the record is there")
            .entity
            .map(|e| e.name),
        Some("The loops view".to_string()),
        "…and it scans like a document",
    );
    assert!(
        over.recall(&id)
            .await
            .expect("recall is not a miss")
            .is_empty(),
        "…and recalling it is an empty page rather than an absence",
    );

    // **The store holds nothing of it.** The claim the whole design rests
    // on, asked of the store itself rather than through the layer that
    // would answer for it either way.
    assert!(
        !over
            .inner
            .list_entities(None)
            .await
            .expect("the store reads")
            .iter()
            .any(|e| e.id == id),
        "the build's record was written down, so this instance is frozen on this build",
    );
}

/// **A stored thing's scan answers alike a supplied one's, through the one
/// layer that resolves both.**
///
/// [`a_record_the_build_ships_answers_like_a_stored_one_and_is_stored_nowhere`]
/// names the claim in its own title and proves only the supplied half;
/// this is the stored half beside it, in one call, so the two cannot
/// drift the way the listing question already had. `scan_entity` derives
/// from `list_entities`, so this is asked here — the one place it
/// actually resolves what the build supplies — rather than of the bare
/// store, which answers rows alone by design.
#[tokio::test]
async fn a_stored_things_scan_answers_like_a_supplied_ones() {
    let supplied = Provisions::new(vec![shipped_record("loops")]);
    let over = Provisioned::new(InMemoryMemory::booted().knowing(supplied.clone()), supplied);

    let stored_id = EntityId("thing:contract-scan-stored".into());
    over.add_entity(NewEntity::new(
        stored_id.clone(),
        "Scan Stored",
        "user-named",
    ))
    .await
    .expect("add_entity should succeed")
    .written()
    .expect("nothing collides with it");
    assert_eq!(
        over.scan_entity(&stored_id)
            .await
            .expect("the scan reads")
            .expect("the record is there")
            .entity
            .map(|e| e.name),
        Some("Scan Stored".to_string()),
        "a stored thing must scan through this layer exactly as a supplied one does",
    );

    let supplied_id = EntityId("view:loops".into());
    assert_eq!(
        over.scan_entity(&supplied_id)
            .await
            .expect("the scan reads")
            .expect("the record is there")
            .entity
            .map(|e| e.name),
        Some("The loops view".to_string()),
        "…and the supplied record still scans beside it",
    );
}

/// **A key set on a record is in that record's scanned document, whether the
/// record is stored or the build supplies it.** The index is built from this
/// scan, and a question that asks by a key and no kind is answered from the
/// index. A supplied record must therefore scan as ONE document that holds
/// both the fields the build ships and the keys written onto it.
#[tokio::test]
async fn a_key_set_on_a_supplied_record_scans_beside_the_fields_the_build_ships() {
    let supplied = Provisions::new(vec![shipped_record("loops")]);
    let over = Provisioned::new(InMemoryMemory::booted().knowing(supplied.clone()), supplied);
    let stored_id = EntityId("thing:contract-scan-stored".into());
    over.add_entity(NewEntity::new(
        stored_id.clone(),
        "Scan Stored",
        "user-named",
    ))
    .await
    .expect("add_entity should succeed")
    .written()
    .expect("nothing collides with it");
    let supplied_id = EntityId("view:loops".into());

    for subject in [&stored_id, &supplied_id] {
        let mut fact = NewFact::about(
            subject.clone(),
            "a measurement",
            jiff::civil::date(2026, 10, 12),
        );
        fact.fields
            .insert("answer_99".to_string(), "found me".to_string());
        fact.sets.insert("answer_99".to_string());
        over.capture(fact)
            .await
            .expect("the capture lands")
            .written()
            .expect("nothing blocks it");
    }

    let scanned = over.scan().await.expect("the scan reads");
    for subject in [&stored_id, &supplied_id] {
        let docs: Vec<&search::DocScan> = scanned
            .iter()
            .filter(|doc| {
                doc.entity.as_ref().map(|e| &e.id) == Some(subject)
                    || doc.doc_id == subject.to_string()
            })
            .collect();
        assert_eq!(docs.len(), 1, "{subject} scans as one document: {docs:?}");
        assert_eq!(
            docs[0].entity.as_ref().map(|e| &e.id),
            Some(subject),
            "{subject}'s document names its entity"
        );
        assert_eq!(
            docs[0].fields.get("answer_99").map(String::as_str),
            Some("found me"),
            "{subject}'s document holds the key written onto it: {:?}",
            docs[0].fields
        );
    }
    assert_eq!(
        scanned
            .iter()
            .find(|doc| doc.entity.as_ref().map(|e| &e.id) == Some(&supplied_id))
            .and_then(|doc| doc.fields.get("selects"))
            .map(String::as_str),
        Some("rhythm"),
        "…and the supplied record keeps the fields the build ships"
    );
}

/// The shared contract's listing case (rule 234), asked of a stored row
/// and, over this layer — the one place `list_entities` actually
/// resolves what the build supplies — a record the build ships, in one
/// call. Not a rewrite of
/// [`a_record_the_build_ships_answers_like_a_stored_one_and_is_stored_nowhere`]
/// above, which is the wider proof over every read; this is the doubled
/// contract's own case, so the listing question cannot drift from the
/// stored one the same way the add_entity guard already had.
#[tokio::test]
async fn an_existing_thing_appears_in_its_kinds_listing_stored_and_supplied() {
    let supplied = Provisions::new(vec![shipped_record("loops")]);
    let over = Provisioned::new(InMemoryMemory::booted().knowing(supplied.clone()), supplied);
    jojobot_domain::memory::testing::contract::an_existing_thing_appears_in_its_kinds_listing_stored_and_supplied(
        &InMemoryMemory::booted(),
        &over,
    )
    .await;
}

/// 🚨 **Asking what sits under a supplied record is an answer, not a
/// miss.**
///
/// A read that resolves the record while the read beside it says the handle
/// names nothing is the two halves disagreeing about what exists — the
/// failure rule 234 is about, met a second time at a second read.
///
/// **Three answers in one case, because the middle one is what a rescue
/// gets wrong.** A supplied record nothing sits under answers with none; a
/// supplied record something DOES sit under answers with that child, which
/// an empty rescue would hide; and a handle nobody has is still a miss.
#[tokio::test]
async fn what_sits_under_a_supplied_record_is_an_answer_and_a_missing_handle_is_still_a_miss() {
    let shipped = EntityId("view:loops".into());
    let under = EntityId("view:my-week".into());
    // **Both halves told the same set**, as the binary wires it: the store
    // below has to see the supplied record when its guard asks whether the
    // parent of this write exists.
    let supplied = Provisions::new(vec![shipped_record("loops")]);
    let store = InMemoryMemory::booted().knowing(supplied.clone());
    let over = Provisioned::new(store, supplied);

    assert_eq!(
        over.children(&shipped).await.expect("the read answers"),
        Vec::<EntityId>::new(),
        "a supplied record nothing sits under answers with none rather than a miss",
    );

    // **A child filed under it**, which is what tells a rescue from a read.
    let mut child = NewEntity::new(under.clone(), "My Week", "user-named");
    child.parent = Some(shipped.clone());
    over.add_entity(child)
        .await
        .expect("the operator files one under it")
        .written()
        .expect("nothing collides with it");
    assert_eq!(
        over.children(&shipped).await.expect("the read answers"),
        vec![under],
        "the child under a supplied record came back empty, so the read is a rescue \
         rather than an answer",
    );

    // …and the absence this read exists to report still reports.
    assert!(
        matches!(
            over.children(&EntityId("view:nobody-has-this".into()))
                .await,
            Err(jojobot_domain::memory::MemoryError::UnknownEntity { .. }),
        ),
        "a handle nobody has stopped being a miss",
    );
}

/// **A claim written about a supplied record is in its one document**,
/// over the fake, with the stored half asked beside it.
#[tokio::test]
async fn a_claim_on_a_supplied_record_is_in_its_one_document_over_the_fake() {
    let supplied = Provisions::new(vec![shipped_record("loops")]);
    let over = Provisioned::new(InMemoryMemory::booted().knowing(supplied.clone()), supplied);
    jojobot_domain::memory::testing::contract::a_claim_on_a_supplied_record_is_in_its_one_document(
        &over,
    )
    .await;
}

/// **A stored row for a handle the build later ships stays the one
/// document**, claims and all: the supplied record neither shadows it nor
/// joins it as a second.
#[tokio::test]
async fn a_stored_row_under_a_supplied_handle_stays_one_document_with_its_claims() {
    let shipped = EntityId("view:loops".into());
    let store = InMemoryMemory::booted();
    store
        .add_entity(NewEntity::new(
            shipped.clone(),
            "Stored Loops",
            "user-named",
        ))
        .await
        .expect("the operator wrote it first")
        .written()
        .expect("nothing shipped it yet");
    let claim = store
        .capture(jojobot_domain::memory::testing::contract::state(
            NewFact::about(
                shipped.clone(),
                "a stored zarbleflux habit",
                jiff::civil::date(2026, 6, 2),
            ),
        ))
        .await
        .expect("the claim lands")
        .written()
        .expect("nothing blocks it");
    let over = Provisioned::new(store, Provisions::new(vec![shipped_record("loops")]));

    let whole = over.scan().await.expect("the scan reads");
    let mine: Vec<_> = whole
        .iter()
        .filter(|d| d.entity.as_ref().is_some_and(|e| e.id == shipped))
        .collect();
    assert_eq!(
        mine.len(),
        1,
        "one document, not a stored one and a supplied one"
    );
    assert_eq!(mine[0].title, "Stored Loops", "the stored row wins");
    assert!(
        mine[0].facts.iter().any(|f| f.address() == claim.address()),
        "the stored document keeps its claim: {:?}",
        mine[0].facts,
    );
    let one = over
        .scan_entity(&shipped)
        .await
        .expect("the one-document read answers")
        .expect("the document exists");
    assert_eq!(one.title, "Stored Loops", "scan_entity agrees");
}

/// **A build that stops shipping a record loses it, and what the operator
/// declared under the same kind is untouched.**
///
/// Paired in one read, or the case passes on a build that empties the
/// listing.
#[tokio::test]
async fn a_record_the_build_stops_shipping_goes_and_the_operators_stays() {
    let store = InMemoryMemory::booted();
    let theirs = EntityId("view:my-week".into());
    store
        .add_entity(NewEntity::new(theirs.clone(), "My Week", "user-named"))
        .await
        .expect("the operator declares their own")
        .written()
        .expect("an empty board blocks nothing");

    let dropped = Provisioned::new(store, Provisions::default());
    let listed = dropped
        .list_entities(Some(EntityKind::VIEW))
        .await
        .expect("the listing reads");
    assert!(
        !listed.iter().any(|e| e.id == EntityId("view:loops".into())),
        "the build stopped shipping it, so the instance stops holding it: {listed:?}",
    );
    assert!(
        listed.iter().any(|e| e.id == theirs),
        "…and what the operator declared is exactly where they left it: {listed:?}",
    );
}

/// **A build that stops supplying loses its half, and the operator keeps
/// theirs.** No reclaim runs, because nothing of the build's was ever
/// stored.
#[tokio::test]
async fn a_build_that_stops_supplying_leaves_the_operators_text_alone() {
    let (store, bot) = provisioned().await;
    store
        .set_prose(&bot, "Gamma files the weekly note.", None)
        .await
        .expect("the operator writes their own layer");

    let dropped = Provisioned::new(store.inner, Provisions::default());
    assert_eq!(read(&dropped, &bot).await, "Gamma files the weekly note.");
}
