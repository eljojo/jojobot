//! **The import reader keeps every row of a real store, and leaves what it
//! cannot place as a finding.** A store written through the port in every shape
//! the import names (a rename, a merge, an archive, a claim rewritten, a key that
//! points at another thing, a declaration) is read once. Each table's row count
//! must equal the store's, the rows must map through the import with no error, and
//! the legacy shapes an older build left must read as they were stored.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::log::claims::FieldValue;
use jojobot_domain::log::claims::import;
use jojobot_domain::memory::WriteClass;
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use jojobot_domain::memory::{
    Edge, EdgeShape, EntityId, FactAddress, FactPatch, Guarded, Memory, NewEntity, NewFact,
    Override, Provenance, guard,
};

async fn thing(memory: &dyn Memory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    let new = NewEntity::new(id.clone(), name, "fixture");
    match memory.add_entity(new.clone()).await.expect("add") {
        // Things of one family share words, and the guard asks to be told apart.
        Guarded::Blocked {
            attempted,
            candidates,
        } => {
            memory
                .add_entity(NewEntity {
                    override_token: Some(guard::override_token(&attempted, &candidates)),
                    override_reason: Some("a fixture of the same family".into()),
                    ..new
                })
                .await
                .expect("add again")
                .written()
                .expect("not blocked twice");
        }
        other => {
            other.written().expect("written");
        }
    }
    id
}

async fn say(
    memory: &dyn Memory,
    about: &EntityId,
    what: &str,
    fields: &[(&str, &str)],
    day: i8,
) -> FactAddress {
    let mut fact = NewFact::about(about.clone(), what, date(2026, 9, day));
    fact.provenance = Provenance::Inference;
    fact.fields = fields
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    fact.sets = fact.fields.keys().cloned().collect();
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("not blocked")
        .address()
}

async fn stored(pool: &sqlx::MySqlPool, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .expect("a count")
}

#[tokio::test]
async fn every_stored_row_is_read_and_the_legacy_shapes_read_as_stored() {
    let scratch = Scratch::new("import-rows");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "importrows")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");

    let homer = thing(&memory, "person:homer", "Homer").await;
    let marge = thing(&memory, "person:marge", "Marge").await;
    let bart = thing(&memory, "person:bart", "Bart").await;
    let lisa = thing(&memory, "person:lisa", "Lisa").await;
    let nelson = thing(&memory, "person:nelson", "Nelson").await;
    let krusty = thing(&memory, "person:krusty", "Krusty").await;

    let eats = say(&memory, &homer, "eats donuts", &[("mood", "happy")], 1).await;
    // A key whose value names another thing writes a link.
    say(
        &memory,
        &homer,
        "is married",
        &[("spouse", "person:marge")],
        2,
    )
    .await;
    // A claim rewritten has two writes.
    let keeps = say(&memory, &marge, "keeps the house", &[("hair", "blue")], 1).await;
    memory
        .update_fact(
            &keeps,
            FactPatch {
                content: Some("keeps the house tidy".into()),
                provenance: Some(Provenance::Inference),
                ..Default::default()
            },
            &EntityId("bot:sigma".into()),
        )
        .await
        .expect("the claim is rewritten")
        .written()
        .expect("not blocked");
    // A claim with an edge, a reference and a lineage.
    let mut fact = NewFact::about(marge.clone(), "is married to Homer", date(2026, 9, 3));
    fact.provenance = Provenance::Testimony;
    fact.edge = Some(Edge::new(EdgeShape::Connection, homer.clone()));
    fact.derived_from = Some(eats);
    fact.refs = vec![homer.clone()];
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
    say(&memory, &bart, "skates", &[("mood", "mischievous")], 1).await;
    say(
        &memory,
        &lisa,
        "plays the saxophone",
        &[("instrument", "saxophone")],
        1,
    )
    .await;
    say(&memory, &krusty, "hosts a show", &[], 1).await;

    // A merge: Lisa is folded into Bart.
    memory
        .merge(
            &lisa,
            &bart,
            Some("filed twice"),
            date(2026, 9, 3),
            &EntityId("bot:sigma".into()),
            Some("run-a"),
        )
        .await
        .expect("the fold lands");
    // A rename: Nelson now wears another handle.
    let renamed = EntityId("person:ralph".into());
    let rename = |token: Option<String>| {
        let (memory, nelson, renamed) = (&memory, &nelson, &renamed);
        async move {
            memory
                .rename_entity(
                    nelson,
                    renamed,
                    None,
                    date(2026, 9, 4),
                    Override::new(
                        token.as_deref(),
                        token.as_ref().map(|_| "a fixture of one family"),
                    ),
                    None,
                )
                .await
                .expect("the rename answers")
        }
    };
    if let Guarded::Blocked {
        attempted,
        candidates,
    } = rename(None).await
    {
        rename(Some(guard::override_token(&attempted, &candidates)))
            .await
            .written()
            .expect("the rename lands");
    }
    // An archive.
    memory
        .archive_entity(&krusty, "off the air", None)
        .await
        .expect("the archive lands");
    // A declaration, whose account `type_write` keeps.
    memory
        .declare_type(
            DeclaredType::new("sketch", vec![Field::new("mood", ValueType::Text)]),
            None,
        )
        .await
        .expect("the declaration lands");

    let read = memory.import_rows().await.expect("the reader reads");
    assert!(
        read.findings.is_empty(),
        "a store written through the port holds nothing the reader cannot place: {:?}",
        read.findings
    );
    let rows = &read.rows;

    // ── every table's count equals the store's ─────────────────────────────
    let counts = [
        ("entity", rows.entities.len() as i64),
        (
            "entity_alias",
            rows.entities
                .iter()
                .map(|row| row.aliases.len() as i64)
                .sum(),
        ),
        ("entity_former_handle", rows.former_handles.len() as i64),
        ("entity_write", rows.entity_writes.len() as i64),
        ("fact", rows.facts.len() as i64),
        (
            "fact_write",
            rows.facts.iter().map(|row| row.writes.len() as i64).sum(),
        ),
        ("field_write", rows.field_writes.len() as i64),
        (
            "field_link",
            rows.field_writes
                .iter()
                .map(|row| row.targets.len() as i64)
                .sum(),
        ),
        (
            "merge_event",
            rows.merges
                .iter()
                .map(|row| 1 + row.claims.len() as i64)
                .sum(),
        ),
        ("type_write", rows.type_writes.len() as i64),
        ("fact_event_metadata", rows.metadata.len() as i64),
    ];
    for (table, read_count) in counts {
        assert_eq!(
            read_count,
            stored(&pool, table).await,
            "{table}: no stored row is lost"
        );
    }
    for table in [
        "entity",
        "fact",
        "fact_write",
        "entity_write",
        "field_write",
        "field_link",
        "merge_event",
        "type_write",
    ] {
        assert!(
            stored(&pool, table).await > 0,
            "the store must hold {table} rows"
        );
    }
    import::import(rows).expect("the rows map through the import with no error");

    // ── a rename keeps the permanent key, and the old handle points at it ──
    let ralph = rows
        .entities
        .iter()
        .find(|row| row.handle == "person:ralph")
        .expect("the renamed thing is read under its new handle");
    let badge = ralph
        .badge
        .clone()
        .expect("a thing made through the port has a badge");
    assert_eq!(ralph.key.0, badge, "the key is the badge, not the handle");
    assert!(
        rows.former_handles
            .iter()
            .any(|row| row.former == "person:nelson" && row.object.0 == badge),
        "the handle it left points at the same key"
    );

    // ── a claim rewritten keeps both writes, in the stored order ───────────
    let rewritten = rows
        .facts
        .iter()
        .find(|row| row.head.content == "keeps the house tidy")
        .expect("the rewritten claim is read");
    assert_eq!(
        rewritten
            .writes
            .iter()
            .map(|write| write.ordinal)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(rewritten.writes[0].claim.content, "keeps the house");

    // ── the legacy shapes an older build left, as they were stored ─────────
    // A version with no name holds no snapshot, and a stamp nobody can read is no
    // moment. Filed under a handle somebody wears, it is that thing's first write.
    sqlx::query(
        "INSERT INTO entity_write (entity, ordinal, written_at) VALUES ('person:homer', 99, 'unreadable')",
    )
    .execute(&pool)
    .await
    .expect("an old version row");
    // A row filed under a handle nobody ever held is a finding and keeps its spelling.
    sqlx::query(
        "INSERT INTO entity_write (entity, ordinal, written_at) VALUES ('person:milhouse', 1, '2026-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("an orphan version row");
    // A key written with no value is a clear, and its class is the one stored.
    sqlx::query(
        "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id, write_class) \
         SELECT badge, 'mood', 50, NULL, 'f1', 'sets' FROM entity WHERE id = 'person:homer'",
    )
    .execute(&pool)
    .await
    .expect("a cleared key");
    let read = memory.import_rows().await.expect("the reader reads again");
    let cleared = read
        .rows
        .field_writes
        .iter()
        .find(|row| row.ordinal == 50)
        .expect("the cleared key is read");
    assert_eq!(cleared.write, FieldValue::Clear);
    assert_eq!(cleared.class, WriteClass::Sets);
    let homer_key = read
        .rows
        .entities
        .iter()
        .find(|row| row.handle == "person:homer")
        .expect("homer is read");
    let old = read
        .rows
        .entity_writes
        .iter()
        .find(|row| row.ordinal == 99)
        .expect("the old version row is read");
    assert_eq!(
        (old.thing.clone(), old.snapshot.is_none(), old.written_at),
        (homer_key.key.clone(), true, None)
    );
    assert_eq!(
        homer_key.taken_in, None,
        "a thing whose first write nobody stamped has no first moment, never a later one"
    );
    let marge_row = read
        .rows
        .entities
        .iter()
        .find(|row| row.handle == "person:marge")
        .expect("marge is read");
    assert!(
        marge_row.taken_in.is_some(),
        "a stamped first write is the first moment"
    );
    assert_eq!(read.findings.len(), 1, "{:?}", read.findings);
    assert_eq!(read.findings[0].table, "entity_write");
    assert!(
        read.rows
            .entity_writes
            .iter()
            .any(|row| row.filed_under == "person:milhouse" && row.thing.0 == "person:milhouse"),
        "an orphan keeps its stored spelling"
    );
    server.stop().await;
}
