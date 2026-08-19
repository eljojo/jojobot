//! **A kind's keys are a schema's keys, against the real store.**
//!
//! A kind is a schema plus identity. The schema half means a kind that names
//! keys answers the type questions like any declaration; the identity half
//! means the two questions can return **different sets for the same word**.
//! That difference is the ruling, and this is where it is shown.
//!
//! **A binary of its own, and the reason is the set.** The kinds a process
//! parses against are process-wide — that is what makes a parse a memory read
//! — and declaring a kind means re-reading them from the store that took the
//! declaration. A process driving several stores at once would have one case
//! loading a set from a store another case is still seeding, so the model
//! holds only where one store owns the process. Production is that; a shared
//! test binary is not.

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::free_port;
use jojobot_domain::memory::types::{DeclaredType, Field, Origin, ValueType};
use jojobot_domain::memory::{
    Boot, EntityId, EntityKind, FactPatch, Memory, NewEntity, NewFact, Provenance, graph, kinds,
};

#[tokio::test]
async fn a_kinds_keys_are_a_schema_and_the_two_questions_differ() {
    let (mut server, store, _turn) = a_store("schema").await;

    // **Declared by a caller**, because the shipped ten name no keys and adding
    // one to them is not this slice's to do.
    store
        .declare_kind(
            "stall",
            Origin::Declared,
            vec![Field::required("pitch", ValueType::Text)],
        )
        .await
        .expect("a caller declares a kind of their own");
    // A declaration writes a row; the set is a copy this process took. Re-read
    // it, or the very next handle carrying that kind cannot be parsed.
    kinds::reload(&store).await.expect("the set is re-read");

    let stall = EntityKind::from_token("stall").expect("the kind was just declared");
    let mine = EntityId::new(stall, "corner-stall");
    let other = EntityId::new(EntityKind::THING, "handcart");
    for (id, what) in [(&mine, "the stall on the corner"), (&other, "a cart")] {
        store
            .add_entity(NewEntity {
                boot: Boot::default(),
                ..NewEntity::new(id.clone(), what, "a test")
            })
            .await
            .expect("the entity is written")
            .written()
            .expect("nothing resembles it");
        store
            .capture(NewFact {
                provenance: Provenance::Testimony,
                fields: [("pitch".to_string(), "somewhere".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(id.clone(), what, jiff::civil::date(2026, 5, 2))
            })
            .await
            .expect("the record is written")
            .written()
            .expect("nothing blocked it");
    }

    // **The kind comes back in the roster of declarations**, which is the schema
    // half being visible rather than leaking.
    let as_a_schema = store
        .declared_types()
        .await
        .expect("the roster reads")
        .into_iter()
        .find(|declared| declared.name == "stall")
        .expect("a kind with keys IS a schema, and the roster says so");

    let by_kind = selected(
        &store,
        graph::Selection {
            kind: Some(stall),
            ..graph::Selection::default()
        },
    )
    .await;
    assert!(
        by_kind.contains(&mine) && !by_kind.contains(&other),
        "asking by KIND answers with the things whose identity it is: {by_kind:?}",
    );

    let by_schema = selected(
        &store,
        graph::Selection {
            answers_type: Some(as_a_schema),
            ..graph::Selection::default()
        },
    )
    .await;
    assert!(
        by_schema.contains(&mine) && by_schema.contains(&other),
        "asking by SCHEMA answers with everything carrying its keys, whatever it is: {by_schema:?}",
    );

    server.stop().await;
}

/// The entities a selection answers with.
async fn selected<M: Memory>(store: &M, select: graph::Selection) -> Vec<EntityId> {
    graph::walk(
        store,
        &graph::GraphQuery {
            select,
            include: graph::Include {
                facts: false,
                prose: false,
            },
            ..graph::GraphQuery::default()
        },
    )
    .await
    .expect("a selection")
    .into_iter()
    .map(|o| o.entity.id)
    .collect()
}

/// **Adding a key to a kind refuses no write that worked before it.**
///
/// This is the property that makes schema versioning unnecessary, and it is
/// the one that has to hold rather than the one that is easy. A thing that was
/// written before a key existed lacks it, so it falls BELOW the floor and stops
/// being protected — the safe direction. The unsafe direction would be for it
/// to start failing checks it never met.
///
/// **If the property were false this would print `BreaksFit`, naming the kind
/// and a key nobody had ever written on that thing** — a refusal on an edit to
/// a record that predates the key entirely.
#[tokio::test]
async fn a_key_added_to_a_kind_refuses_no_write_that_worked_before() {
    let (mut server, store, _turn) = a_store("versioning").await;

    store
        .declare_kind(
            "kiosk",
            Origin::Declared,
            vec![Field::required("pitch", ValueType::Text)],
        )
        .await
        .expect("a caller declares a kind");
    kinds::reload(&store).await.expect("the set is re-read");
    let kiosk = EntityKind::from_token("kiosk").expect("the kind was just declared");

    let old = EntityId::new(kiosk, "the-one-that-predates-it");
    added(&store, &old, "the kiosk by the door").await;
    let record = captured(&store, &old, "somewhere", None).await;

    // The kind grows a key the thing has never carried.
    store
        .declare_kind(
            "kiosk",
            Origin::Declared,
            vec![
                Field::required("pitch", ValueType::Text),
                Field::required("opens_at", ValueType::Text),
            ],
        )
        .await
        .expect("the kind gains a key");

    // …and an ordinary edit to the old record still lands.
    store
        .update_fact(
            &record,
            FactPatch {
                content: Some("the kiosk by the side door".to_string()),
                ..FactPatch::default()
            },
        )
        .await
        .expect("a write that worked before the key existed still works")
        .written()
        .expect("nothing blocked it");

    // **The control, without which this case cannot fail.** A green above says
    // the edit was allowed; it does not say anything was ever guarding. So the
    // same kind, on a thing that DOES hold every key it names, must still
    // refuse a write that takes one away — otherwise a build with the floor
    // switched off entirely passes this case unchanged.
    let fitting = EntityId::new(kiosk, "holds-them-all");
    added(&store, &fitting, "the complete one").await;
    let whole = store
        .capture(NewFact {
            provenance: Provenance::Testimony,
            fields: [
                ("pitch".to_string(), "the corner".to_string()),
                ("opens_at".to_string(), "early".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(
                fitting.clone(),
                "a complete record",
                jiff::civil::date(2026, 5, 4),
            )
        })
        .await
        .expect("the record is written")
        .written()
        .expect("nothing blocked it");
    store
        .update_fact(
            &whole.address(),
            FactPatch {
                clear_fields: vec!["opens_at".to_string()],
                ..FactPatch::default()
            },
        )
        .await
        .expect_err("the floor is on: a thing that fits is still protected");

    server.stop().await;
}

/// **A thing holding every key its kind names is protected, and one that does
/// not is never refused.**
///
/// Both halves in one case, because either alone reads as the other rule. The
/// floor protects a fit a thing HAS; it never demands one a thing does not.
#[tokio::test]
async fn the_floor_protects_a_thing_that_fits_and_leaves_one_that_does_not() {
    let (mut server, store, _turn) = a_store("floor").await;

    store
        .declare_kind(
            "pitch-stall",
            Origin::Declared,
            vec![Field::required("pitch", ValueType::Text)],
        )
        .await
        .expect("a caller declares a kind");
    kinds::reload(&store).await.expect("the set is re-read");
    let stall = EntityKind::from_token("pitch-stall").expect("the kind was just declared");

    // One that holds the key its kind names…
    let fitting = EntityId::new(stall, "holds-its-key");
    added(&store, &fitting, "the one that fits").await;
    let held = captured(&store, &fitting, "the corner", None).await;

    let refused = store
        .update_fact(
            &held,
            FactPatch {
                clear_fields: vec!["pitch".to_string()],
                ..FactPatch::default()
            },
        )
        .await
        .expect_err("taking away the key its kind names is refused");
    let said = refused.to_string();
    assert!(
        said.contains("pitch-stall") && said.contains("pitch"),
        "the refusal names the kind and the key, which is what a caller needs: {said}",
    );

    // …and one that never held it is not refused anything.
    let bare = EntityId::new(stall, "holds-nothing");
    added(&store, &bare, "the one that does not fit").await;
    let loose = captured(&store, &bare, "", Some("colour")).await;
    store
        .update_fact(
            &loose,
            FactPatch {
                clear_fields: vec!["colour".to_string()],
                ..FactPatch::default()
            },
        )
        .await
        .expect("a thing that fits nothing has nothing to protect")
        .written()
        .expect("nothing blocked it");

    server.stop().await;
}

/// **A thing whose kind's keys it lacks is still that kind.**
///
/// It is free today because nothing derives a thing's kind from its fields —
/// the kind is in the handle. **A property that is free because nothing
/// threatens it still needs a red bar waiting** for whoever adds a derivation
/// later and reasons that an incomplete thing should stop counting.
#[tokio::test]
async fn identity_does_not_lapse_when_a_thing_lacks_its_kinds_keys() {
    let (mut server, store, _turn) = a_store("identity").await;

    store
        .declare_kind(
            "barrow",
            Origin::Declared,
            vec![Field::required("pitch", ValueType::Text)],
        )
        .await
        .expect("a caller declares a kind");
    kinds::reload(&store).await.expect("the set is re-read");
    let barrow = EntityKind::from_token("barrow").expect("the kind was just declared");

    let bare = EntityId::new(barrow, "carries-none-of-them");
    added(&store, &bare, "the empty one").await;

    let listed = store
        .list_entities(Some(barrow))
        .await
        .expect("a listing of that kind");
    assert!(
        listed.iter().any(|e| e.id == bare),
        "a thing holding none of its kind's keys is still that kind: {listed:?}",
    );
    assert_eq!(
        listed
            .iter()
            .find(|e| e.id == bare)
            .map(|e| e.kind.as_token()),
        Some("barrow"),
        "…and it reads back as that kind rather than as whatever a fallback would say",
    );

    server.stop().await;
}

/// **One store for this whole binary, and that is the file's premise rather
/// than a convenience.**
///
/// The kinds a process parses against are process-wide, and declaring a kind
/// means re-reading them from the store that took it. Give two cases two
/// stores and one case's re-read replaces the other's set mid-write — a
/// validated handle then comes back with no kind at all. **The model holds
/// where one store owns the process**, which is what production is, so the
/// cases share one and each names kinds of its own.
///
/// It is never stopped: the cases run concurrently and any one of them calling
/// `stop` would take the store out from under the others. The scratch
/// directory goes when the process does.
async fn a_store(what: &str) -> (Dolt, DoltMemory, tokio::sync::MutexGuard<'static, ()>) {
    // **The cases take turns, and the store is each case's own.**
    //
    // Two things force this shape and they pull in opposite directions. The
    // kinds a process parses against are process-wide, and declaring a kind
    // re-reads them from the store that took it — so two cases running at once
    // would have one replacing the other's set mid-write, and a validated
    // handle would come back carrying no kind. That argues for one store.
    //
    // But a store cannot be shared across these cases: each is its own tokio
    // test, the pool belongs to the runtime that opened it, and the first
    // runtime to finish takes the pool down with it. Every later case then
    // fails to reach a store that looks perfectly alive.
    //
    // So: a store per case, and a turn so only one of them owns the process's
    // kind set at a time.
    static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let turn = TURN.lock().await;

    let path =
        std::env::temp_dir().join(format!("jojobot-kind-schema-{}-{what}", std::process::id()));
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let server = Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    migrate::run(server.pool()).await.expect("the schema");
    migrate::seed_kinds(server.pool())
        .await
        .expect("the kinds are seeded");
    let store = DoltMemory::open(server.pool().clone());
    (server, store, turn)
}

/// An entity the guard is expected to wave through.
async fn added(store: &DoltMemory, id: &EntityId, name: &str) {
    store
        .add_entity(NewEntity {
            boot: Boot::default(),
            ..NewEntity::new(id.clone(), name, "a test")
        })
        .await
        .expect("the entity is written")
        .written()
        .expect("nothing resembles it");
}

/// A record carrying `pitch`, or another key when one is named.
async fn captured(
    store: &DoltMemory,
    id: &EntityId,
    value: &str,
    other_key: Option<&str>,
) -> jojobot_domain::memory::FactAddress {
    let key = other_key.unwrap_or("pitch");
    let fact = store
        .capture(NewFact {
            provenance: Provenance::Testimony,
            fields: [(key.to_string(), value.to_string())].into_iter().collect(),
            ..NewFact::about(id.clone(), "a record", jiff::civil::date(2026, 5, 2))
        })
        .await
        .expect("the record is written")
        .written()
        .expect("nothing blocked it");
    fact.address()
}

/// **A kind and a schema share one row-space, so it has to say which is which.**
///
/// A kind's keys live where a schema's keys live — that is the model. What the
/// model does not say is that either may quietly take the other's rows, and
/// both could: the name is the whole key, so a declaration on one side
/// replaced the other side's keys with no refusal and no trace.
///
/// **The worst instance needed no new verb at all.** The seed declares every
/// shipped kind on every boot, naming no keys, so a caller who had declared an
/// ordinary schema called `project` lost it the next time the server started.
#[tokio::test]
async fn a_boot_does_not_destroy_a_schema_named_like_a_shipped_kind() {
    let (mut server, store, _turn) = a_store("boot-collision").await;

    store
        .declare_type(DeclaredType::new(
            "project",
            vec![Field::required("budget", ValueType::Text)],
        ))
        .await
        .expect("a caller declares a schema of their own");

    // The boot's own step, run again exactly as a restart runs it.
    migrate::seed_kinds(server.pool())
        .await
        .expect("the kinds are seeded again");

    let held = store
        .declared_types()
        .await
        .expect("the roster reads")
        .into_iter()
        .find(|declared| declared.name == "project");
    assert_eq!(
        held.map(|declared| declared
            .fields
            .iter()
            .map(|f| f.key.clone())
            .collect::<Vec<_>>()),
        Some(vec!["budget".to_string()]),
        "a restart must not take away a schema somebody declared",
    );

    server.stop().await;
}

/// **Neither side may take the other's keys**, and the refusal says which
/// thing already holds the name.
#[tokio::test]
async fn a_kind_and_a_schema_cannot_take_each_others_keys() {
    let (mut server, store, _turn) = a_store("namespace").await;

    store
        .declare_kind(
            "cartwright",
            Origin::Declared,
            vec![Field::required("pitch", ValueType::Text)],
        )
        .await
        .expect("a caller declares a kind");

    let refused = store
        .declare_type(DeclaredType::new(
            "cartwright",
            vec![Field::required("price", ValueType::Text)],
        ))
        .await
        .expect_err("a schema cannot reshape a kind");
    assert!(
        refused.to_string().contains("cartwright"),
        "the refusal names what already holds it: {refused}",
    );

    // …and the kind's keys are where they were.
    let held = store
        .declared_types()
        .await
        .expect("the roster reads")
        .into_iter()
        .find(|declared| declared.name == "cartwright")
        .expect("the kind is still declared");
    assert_eq!(
        held.fields
            .iter()
            .map(|f| f.key.as_str())
            .collect::<Vec<_>>(),
        vec!["pitch"],
        "the kind kept its keys: {held:?}",
    );

    // The other direction: a schema's name is not a kind's to take.
    store
        .declare_type(DeclaredType::new(
            "barrow-boy",
            vec![Field::required("price", ValueType::Text)],
        ))
        .await
        .expect("a caller declares a schema");
    let refused = store
        .declare_kind(
            "barrow-boy",
            Origin::Declared,
            vec![Field::required("pitch", ValueType::Text)],
        )
        .await
        .expect_err("a kind cannot reshape a schema somebody declared");
    assert!(
        refused.to_string().contains("barrow-boy"),
        "…and that refusal names it too: {refused}",
    );

    server.stop().await;
}

/// **A required key is a floor; an optional key is checked and never demanded.**
///
/// Two independent properties, and the whole slice is that they stopped being
/// one. Fitting used to be every key a declaration named, so declaring what a
/// thing MAY carry was impossible without demanding all of it — which is how a
/// schema ends up shaped by what the type system can carry rather than by what
/// somebody keeps.
///
/// **Three beats, and the third is the one that matters.** A missing optional
/// key is served; a missing required key is refused; and an optional key
/// holding something it does not hold is refused all the same.
///
/// ⚠️ **A build where "optional" means "not checked at all" passes the first
/// two and fails the third**, which is why the third is here: the two easy
/// halves are satisfied by simply removing optional keys from every check.
#[tokio::test]
async fn a_required_key_is_a_floor_and_an_optional_one_is_checked_but_never_demanded() {
    let (mut server, store, _turn) = a_store("required").await;

    store
        .declare_kind(
            "market-stall",
            Origin::Declared,
            vec![
                Field::required("pitch", ValueType::Text),
                // Welcome, never demanded — and still a date when it is there.
                Field::new("awning", ValueType::Date),
            ],
        )
        .await
        .expect("a caller declares a kind");
    kinds::reload(&store).await.expect("the set is re-read");
    let stall = EntityKind::from_token("market-stall").expect("the kind was just declared");

    let corner = EntityId::new(stall, "corner-pitch");
    added(&store, &corner, "the pitch on the corner").await;
    let record = store
        .capture(NewFact {
            provenance: Provenance::Testimony,
            fields: [
                ("pitch".to_string(), "the corner".to_string()),
                ("awning".to_string(), "2026-05-02".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(corner.clone(), "a record", jiff::civil::date(2026, 5, 2))
        })
        .await
        .expect("the record is written")
        .written()
        .expect("nothing blocked it");

    // ① The optional key goes, and nothing is refused: a thing without it is
    // not a thing with something missing.
    store
        .update_fact(
            &record.address(),
            FactPatch {
                clear_fields: vec!["awning".to_string()],
                ..FactPatch::default()
            },
        )
        .await
        .expect("an optional key is never demanded")
        .written()
        .expect("nothing blocked it");

    // ② The required key does not: it is what makes this thing one of these.
    let refused = store
        .update_fact(
            &record.address(),
            FactPatch {
                clear_fields: vec!["pitch".to_string()],
                ..FactPatch::default()
            },
        )
        .await
        .expect_err("the required key has to survive the write");
    let said = refused.to_string();
    assert!(
        said.contains("market-stall") && said.contains("pitch"),
        "the refusal names the kind and the key it would cost: {said}",
    );

    // ③ **And what the optional key HOLDS is checked whenever it is set.** The
    // key is welcome to be absent and is held to its declaration the moment it
    // is there — the two questions are independent, and this is the one a
    // build that simply skips optional keys gets wrong.
    let wrong = store
        .update_fact(
            &record.address(),
            FactPatch {
                fields: [("awning".to_string(), "sometime in may".to_string())]
                    .into_iter()
                    .collect(),
                ..FactPatch::default()
            },
        )
        .await
        .expect_err("an optional key still holds what it was declared to hold");
    let said = wrong.to_string();
    assert!(
        said.contains("awning") && said.contains("date"),
        "the refusal names the key and what it wanted: {said}",
    );

    server.stop().await;
}

/// **A key may hold zero or more, and a span is one value rather than two
/// keys.**
///
/// Both are here because both were missing at once and the same schema paid
/// for it: a span written as two date keys reads as a thing with a key missing
/// when only one end is known, and a key that can hold one person drops
/// everybody else who was there.
///
/// **Zero is the beat that makes it a list.** A key holding none of something
/// is a thing that had nobody along, which is different from a key nobody
/// filled in — and a build treating an empty value as an absent key cannot say
/// the difference.
#[tokio::test]
async fn a_list_key_holds_none_one_or_many_and_a_span_is_one_value() {
    let (mut server, store, _turn) = a_store("shapes").await;

    store
        .declare_kind(
            "outing",
            Origin::Declared,
            vec![
                Field::required("away", ValueType::DateRange),
                Field {
                    points_at: Some(EntityKind::PERSON),
                    ..Field::listing("came_with", ValueType::Reference)
                },
            ],
        )
        .await
        .expect("a caller declares a kind");
    kinds::reload(&store).await.expect("the set is re-read");
    let outing = EntityKind::from_token("outing").expect("the kind was just declared");

    for who in [
        EntityId::person("bart"),
        EntityId::person("milhouse"),
        EntityId::new(EntityKind::THING, "red-bike"),
    ] {
        added(&store, &who, who.slug()).await;
    }
    let day = EntityId::new(outing, "the-day-out");
    added(&store, &day, "the day out").await;

    // **The span is one value.** Two dates, one key, and the pair cannot come
    // apart the way two keys can.
    let record = store
        .capture(NewFact {
            provenance: Provenance::Testimony,
            fields: [
                ("away".to_string(), "2026-04-18/2026-04-25".to_string()),
                ("came_with".to_string(), String::new()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(day.clone(), "a record", jiff::civil::date(2026, 4, 18))
        })
        .await
        .expect("the record is written")
        .written()
        .expect("nothing blocked it");
    assert_eq!(
        record.fields.get("came_with").map(String::as_str),
        Some(""),
        "a list with nothing in it is a thing that had nobody along: {:?}",
        record.fields,
    );

    // One, then many, through the same key. The handles are built rather than
    // written out, so no fixture name enters this source as a literal.
    let bart = EntityId::person("bart").to_string();
    let milhouse = EntityId::person("milhouse").to_string();
    let both = format!("{bart}, {milhouse}");
    for wrote in [bart.as_str(), both.as_str()] {
        let expected = wrote;
        let edited = store
            .update_fact(
                &record.address(),
                FactPatch {
                    fields: [("came_with".to_string(), wrote.to_string())]
                        .into_iter()
                        .collect(),
                    ..FactPatch::default()
                },
            )
            .await
            .expect("a list key takes as many as it is given")
            .written()
            .expect("nothing blocked it");
        assert_eq!(
            edited.fields.get("came_with").map(String::as_str),
            Some(expected),
            "the items come back as they were written",
        );
    }

    // **Every item is held to what the key points at**, or a list would be the
    // way past the narrowing rather than a use of it.
    let refused = store
        .update_fact(
            &record.address(),
            FactPatch {
                fields: [(
                    "came_with".to_string(),
                    format!("{bart}, {}", EntityId::new(EntityKind::THING, "red-bike")),
                )]
                .into_iter()
                .collect(),
                ..FactPatch::default()
            },
        )
        .await
        .expect_err("an item of the wrong kind is not a person who came along");
    assert!(
        refused.to_string().contains("came_with"),
        "the refusal names the key: {refused}",
    );

    // A span that ends before it starts is two dates in the wrong slots.
    let backwards = store
        .update_fact(
            &record.address(),
            FactPatch {
                fields: [("away".to_string(), "2026-04-25/2026-04-18".to_string())]
                    .into_iter()
                    .collect(),
                ..FactPatch::default()
            },
        )
        .await
        .expect_err("a span has to run forwards");
    assert!(
        backwards.to_string().contains("away"),
        "the refusal names the key: {backwards}",
    );

    server.stop().await;
}

/// **The name `rhythm` is the KIND's, and the kind carries the keys.**
///
/// A shipped type held that name, and a kind's keys and a type's keys share one
/// row-space under it — so the kind could name nothing while the type was
/// there, and every `rhythm:` entity was held to a type nobody was offered.
/// Taking the name back is what lets a loop be described by what it IS.
///
/// **Two beats, and the second is the model change earning its keep.** A loop
/// that holds its name and the day it last ran may not lose either. A loop that
/// holds only its name is legal, complete and refused nothing — because the
/// required set is two keys and everything else is what somebody had to hand.
#[tokio::test]
async fn the_rhythm_kind_owns_its_name_and_asks_for_two_keys() {
    let (mut server, store, _turn) = a_store("rhythm-kind").await;

    // The seed writes the kind's keys, and reading them back off the store is
    // what says the name is the kind's: while the type held it, this
    // declaration was refused.
    let held = store
        .declared_types()
        .await
        .expect("the roster reads")
        .into_iter()
        .find(|t| t.name == "rhythm")
        .expect("the shipped kind carries its keys");
    assert_eq!(
        held.fields
            .iter()
            .map(|f| (f.key.as_str(), f.required))
            .collect::<Vec<_>>(),
        vec![
            ("name", true),
            ("last_ran", true),
            ("note", false),
            ("cadence", false),
            ("outcome", false),
        ],
        "the required set is the name and the day it last ran: {held:?}",
    );

    // A loop is filed under whatever it is a loop ON.
    let plant = EntityId::new(EntityKind::THING, "handcart");
    added(&store, &plant, "the thing the loop is on").await;
    let ran = EntityId::new(EntityKind::RHYTHM, "corner-stall");
    store
        .add_entity(NewEntity {
            boot: Boot::default(),
            parent: Some(plant.clone()),
            ..NewEntity::new(ran.clone(), "the loop that has run", "a test")
        })
        .await
        .expect("the entity is written")
        .written()
        .expect("nothing resembles it");

    let record = store
        .capture(NewFact {
            provenance: Provenance::Testimony,
            fields: [
                ("name".to_string(), "the loop that has run".to_string()),
                ("last_ran".to_string(), "2026-05-02".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(ran.clone(), "a record", jiff::civil::date(2026, 5, 2))
        })
        .await
        .expect("the record is written")
        .written()
        .expect("nothing blocked it");

    let refused = store
        .update_fact(
            &record.address(),
            FactPatch {
                clear_fields: vec!["last_ran".to_string()],
                ..FactPatch::default()
            },
        )
        .await
        .expect_err("a loop that has run may not lose the day it ran");
    assert!(
        refused.to_string().contains("rhythm") && refused.to_string().contains("last_ran"),
        "the refusal names the kind and the key: {refused}",
    );

    // **A loop with only a name is legal.** The optional keys are welcome and
    // never demanded, so nothing here is refused and nothing is missing.
    let bare = EntityId::new(EntityKind::RHYTHM, "holds-nothing");
    store
        .add_entity(NewEntity {
            boot: Boot::default(),
            parent: Some(plant.clone()),
            ..NewEntity::new(bare.clone(), "the loop nobody has dated", "a test")
        })
        .await
        .expect("the entity is written")
        .written()
        .expect("nothing resembles it");
    store
        .capture(NewFact {
            provenance: Provenance::Testimony,
            fields: [("name".to_string(), "water the plant".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(bare.clone(), "a record", jiff::civil::date(2026, 5, 2))
        })
        .await
        .expect("a loop with only a name is a loop")
        .written()
        .expect("nothing blocked it");

    server.stop().await;
}
