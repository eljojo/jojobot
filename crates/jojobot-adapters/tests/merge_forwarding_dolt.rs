//! **Every path that follows a merge follows the same ones** (card 2428).
//!
//! A merge made before events were kept leaves only a forwarding row, and
//! `index()` reads that row as the merge. These cases take the same shape from
//! the other paths, each paired with a merge that kept its events in the same
//! store.

use std::path::PathBuf;

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::memory::owned::{Provision, Provisions};
use jojobot_domain::memory::{
    EntityId, EntityKind, FactAddress, FactPatch, Memory, MemoryError, NewEntity, NewFact,
    Override, Provenance,
};
use sqlx::MySqlPool;

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A store of its own: the server, its pool and the memory over it.
async fn store_of(name: &str) -> (Scratch, Dolt, MySqlPool, DoltMemory) {
    let dir = std::env::temp_dir().join(format!("jojobot-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let scratch = Scratch(dir.clone());
    let server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, name)
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    (scratch, server, pool, memory)
}

async fn thing(
    memory: &dyn Memory,
    handle: &str,
    name: &str,
    parent: Option<&EntityId>,
) -> EntityId {
    let id = EntityId(handle.to_string());
    memory
        .add_entity(NewEntity {
            parent: parent.cloned(),
            ..NewEntity::new(id.clone(), name, "fixture")
        })
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    id
}

async fn say(
    memory: &dyn Memory,
    about: &EntityId,
    what: &str,
    fields: &[(&str, &str)],
) -> FactAddress {
    let mut fact = NewFact::about(about.clone(), what, date(2026, 9, 1));
    fact.provenance = Provenance::Testimony;
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

async fn merge(memory: &dyn Memory, folded: &EntityId, survivor: &EntityId) {
    memory
        .merge(
            folded,
            survivor,
            Some("filed twice"),
            date(2026, 9, 2),
            &EntityId("bot:sigma".into()),
            Some("run-a"),
        )
        .await
        .expect("the fold lands");
}

/// The merge made "before events were kept": its event rows are taken out,
/// the forwarding row stays.
async fn forget_the_events_of(pool: &MySqlPool, folded: &str) {
    sqlx::query("DELETE FROM merge_event WHERE folded = (SELECT badge FROM entity WHERE id = ?)")
        .bind(folded)
        .execute(pool)
        .await
        .expect("the event is taken out");
}

/// **The children a fold serves under the survivor are the same whether the
/// merge kept an event or left a forwarding row.** A numbered collection counts
/// the number a child holds under a folded parent as one of its own, so the next
/// number and the refusal of a taken one are the questions that tell.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_children_of_a_fold_without_an_event_are_served_under_its_survivor() {
    let (_scratch, mut server, pool, memory) = store_of("forwarded-children").await;

    // The merge made before events were kept.
    let old_survivor = thing(&memory, "person:krusty", "Krusty", None).await;
    say(
        &memory,
        &old_survivor,
        "numbers its members",
        &[("numbers_by", "number")],
    )
    .await;
    let old_folded = thing(&memory, "person:skinner", "Skinner", None).await;
    let old_child = thing(&memory, "person:bart", "Bart", Some(&old_folded)).await;
    say(&memory, &old_child, "holds three", &[("number", "3")]).await;
    merge(&memory, &old_folded, &old_survivor).await;
    forget_the_events_of(&pool, "person:skinner").await;

    // The merge made with its event kept.
    let new_survivor = thing(&memory, "person:burns", "Burns", None).await;
    say(
        &memory,
        &new_survivor,
        "numbers its members",
        &[("numbers_by", "number")],
    )
    .await;
    let new_folded = thing(&memory, "person:homer", "Homer", None).await;
    let new_child = thing(&memory, "person:lisa", "Lisa", Some(&new_folded)).await;
    say(&memory, &new_child, "holds seven", &[("number", "7")]).await;
    merge(&memory, &new_folded, &new_survivor).await;

    let next = |survivor: EntityId| {
        let memory = &memory;
        async move {
            memory
                .numbering_of(&survivor)
                .await
                .expect("reads")
                .expect("numbered")
                .next_number
        }
    };
    assert_eq!(
        next(new_survivor.clone()).await,
        8,
        "the control: a child of a fold with an event is a member"
    );
    assert_eq!(
        next(old_survivor.clone()).await,
        4,
        "a child of a fold with only a forwarding row is a member too"
    );

    let taken = memory
        .capture({
            let mut fact = NewFact::about(
                thing(&memory, "person:apu", "Apu", Some(&old_survivor)).await,
                "asks for three",
                date(2026, 9, 3),
            );
            fact.fields = [("number".to_string(), "3".to_string())].into();
            fact.sets = fact.fields.keys().cloned().collect();
            fact
        })
        .await;
    assert!(
        matches!(&taken, Err(MemoryError::NumberTaken { holder, .. }) if holder == "person:bart"),
        "a number held under a folded parent stays held: {taken:?}"
    );
    server.stop().await;
}

/// A view the build supplies: no row, so a write on it is filed under its text.
fn supplied_view(id: &EntityId, name: &str) -> Provision {
    Provision::record(
        jojobot_domain::memory::Entity {
            folded_names: Vec::new(),
            id: id.clone(),
            kind: EntityKind::VIEW,
            name: name.into(),
            aliases: Vec::new(),
            source: "jojobot".into(),
            crm: None,
            parent: None,
            boot: Default::default(),
            merged_into: None,
            badge: None,
            archived: None,
        },
        std::collections::BTreeMap::from([("selects".to_string(), "rhythm".to_string())]),
    )
}

async fn rename(memory: &dyn Memory, from: &EntityId, to: &str) -> EntityId {
    let to = EntityId(to.to_string());
    memory
        .rename_entity(from, &to, None, date(2026, 9, 4), Override::none(), None)
        .await
        .expect("rename_entity ok")
        .written()
        .expect("not blocked");
    to
}

/// **A write's references and marks are kept as text that holds storage keys**,
/// so the adoption of a handle the build stopped supplying moves them with every
/// other column that holds the handle. Read back after a rename, the write
/// serves the handle the thing wears now; one that kept the old text names a
/// handle nothing answers to. Paired with a thing that was never adopted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_adoption_moves_the_references_and_marks_a_write_kept_onto_the_badge() {
    let (_scratch, mut server, pool, _memory) = store_of("adopted-references").await;
    let retired = EntityId("view:contract-retired".into());
    let writer = DoltMemory::open(pool.clone())
        .knowing(Provisions::new(vec![supplied_view(&retired, "Retired")]));
    let caller = EntityId("bot:sigma".into());

    // The build supplies the view and the operator writes on it: a claim filed
    // under the handle's own text, and a claim of another thing that points at it
    // and stands for it.
    let on_view = say(&writer, &retired, "answers a question", &[]).await;
    let author = thing(&writer, "person:krusty", "Krusty", None).await;
    let mut pointing = NewFact::about(author.clone(), "points at the view", date(2026, 9, 1));
    pointing.provenance = Provenance::Testimony;
    pointing.refs = vec![retired.clone()];
    let pointing = writer
        .capture(pointing)
        .await
        .expect("capture")
        .written()
        .expect("not blocked")
        .address();
    writer
        .update_fact(
            &pointing,
            FactPatch {
                stands_for: Some(vec![on_view.clone()]),
                ..Default::default()
            },
            &caller,
        )
        .await
        .expect("update_fact ok")
        .written()
        .expect("not blocked");

    // The control is a thing that was never supplied, pointed at the same way.
    let plain = thing(&writer, "person:homer", "Homer", None).await;
    let on_plain = say(&writer, &plain, "answers too", &[]).await;
    let mut pointing_plain = NewFact::about(author.clone(), "points at homer", date(2026, 9, 1));
    pointing_plain.provenance = Provenance::Testimony;
    pointing_plain.refs = vec![plain.clone()];
    let pointing_plain = writer
        .capture(pointing_plain)
        .await
        .expect("capture")
        .written()
        .expect("not blocked")
        .address();
    writer
        .update_fact(
            &pointing_plain,
            FactPatch {
                stands_for: Some(vec![on_plain.clone()]),
                ..Default::default()
            },
            &caller,
        )
        .await
        .expect("update_fact ok")
        .written()
        .expect("not blocked");

    // A later build stops supplying the view; the boot adopts its handle.
    writer
        .record_supplied_handles(&[])
        .await
        .expect("the supplied handles are recorded");
    let later = DoltMemory::open(pool.clone());
    let report = later
        .adopt_handles_without_a_row()
        .await
        .expect("the pass runs");
    assert_eq!(report.adopted, vec![retired.clone()]);
    later
        .resolve_stale_pointer_columns()
        .await
        .expect("the pointers resolve");

    // Both things take a new handle; the writes that point at them serve it.
    let adopted_now = rename(&later, &retired, "view:contract-renamed").await;
    let plain_now = rename(&later, &plain, "person:lisa").await;
    for (pointing, thing_now, claim_now) in [
        (&pointing_plain, &plain_now, on_plain.home.clone()),
        (&pointing, &adopted_now, retired.clone()),
    ] {
        let writes = later.claim_history(pointing).await.expect("history reads");
        let last = writes.last().expect("the claim has writes");
        assert_eq!(
            last.refs.as_deref(),
            Some(std::slice::from_ref(thing_now)),
            "the write's references serve the handle the thing wears now (was {claim_now})"
        );
        assert_eq!(
            last.stands_for
                .as_deref()
                .map(|marks| marks.iter().map(|m| m.home.clone()).collect::<Vec<_>>()),
            Some(vec![thing_now.clone()]),
            "the write's marks serve the handle the thing wears now"
        );
    }
    server.stop().await;
}
