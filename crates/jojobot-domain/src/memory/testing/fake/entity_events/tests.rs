use super::*;
use crate::log::ObjectId;
use crate::memory::{EntityPatch, NewEntity, NewFact, Override};
use jiff::civil::date;

fn handle(text: &str) -> EntityId {
    EntityId(text.to_string())
}

/// What the claims State holds for `spelling` now: the object and its body.
fn held(
    fake: &InMemoryMemory,
    spelling: &str,
) -> Option<(ObjectId, crate::log::claims::ObjectBody)> {
    let state = fake.claims_state();
    let object = state.resolve(spelling)?.clone();
    let body = state.object(&object)?.clone();
    Some((object, body))
}

/// **Each entity writer of the fake leaves its change in the claims State**, as
/// the domain's event choice makes it: a creation holds the object under its
/// handle, an edit rewrites it, an archive and a restore flip it, a prose write
/// leaves it held, and a rename moves the handle and retires the old spelling.
#[tokio::test]
async fn each_entity_writer_of_the_fake_leaves_its_change_in_the_claims_state() {
    crate::memory::kinds::load_shipped();
    let fake = InMemoryMemory::seeded();
    let homer = handle("person:homer");

    fake.add_entity(NewEntity::new(homer.clone(), "Homer", "tests"))
        .await
        .expect("the creation is not refused");
    let (object, body) = held(&fake, "person:homer").expect("a creation is held");
    assert_eq!(body.name, "Homer");
    assert!(!body.archived);

    fake.update_entity(
        &homer,
        EntityPatch {
            name: Some("Homer J".into()),
            ..EntityPatch::default()
        },
    )
    .await
    .expect("the edit is not refused");
    assert_eq!(held(&fake, "person:homer").expect("held").1.name, "Homer J");

    fake.archive_entity(&homer, "a test", None)
        .await
        .expect("the archive lands");
    assert!(held(&fake, "person:homer").expect("held").1.archived);
    fake.restore_entity(&homer, None)
        .await
        .expect("the restore lands");
    assert!(!held(&fake, "person:homer").expect("held").1.archived);

    fake.set_prose(&homer, "A line of prose.", None)
        .await
        .expect("the prose lands");
    assert!(held(&fake, "person:homer").is_some());

    let marge = handle("person:marge");
    fake.rename_entity(
        &homer,
        &marge,
        None,
        date(2026, 8, 2),
        Override::none(),
        None,
    )
    .await
    .expect("the rename lands");
    let (renamed, _) = held(&fake, "person:marge").expect("the new handle is held");
    assert_eq!(renamed, object, "a rename keeps the object");
    assert!(
        held(&fake, "person:homer").is_none(),
        "the old spelling is retired"
    );
}

/// **A creation or an edit whose claim is refused is taken back from the claims
/// State as well**, as the rows are: the State never holds an object the store
/// does not.
#[tokio::test]
async fn an_act_whose_claim_is_refused_leaves_the_claims_state_as_it_was() {
    crate::memory::kinds::load_shipped();
    let fake = InMemoryMemory::seeded();
    let lisa = handle("person:lisa");

    // An empty claim is malformed, so the capture inside the act refuses.
    let refused = fake
        .add_entity_with_first_claim(
            NewEntity::new(lisa.clone(), "Lisa", "tests"),
            NewFact::about(lisa.clone(), "", date(2026, 8, 2)),
        )
        .await;
    assert!(refused.is_err(), "the empty claim refuses the act");
    assert!(
        held(&fake, "person:lisa").is_none(),
        "the refused creation left an object in the State"
    );

    fake.add_entity(NewEntity::new(lisa.clone(), "Lisa", "tests"))
        .await
        .expect("the creation is not refused");
    let refused = fake
        .update_entity_with_claim(
            &lisa,
            EntityPatch {
                name: Some("Lisa S".into()),
                ..EntityPatch::default()
            },
            NewFact::about(lisa.clone(), "", date(2026, 8, 2)),
        )
        .await;
    assert!(refused.is_err(), "the empty claim refuses the act");
    assert_eq!(
        held(&fake, "person:lisa").expect("held").1.name,
        "Lisa",
        "the refused edit left its name in the State"
    );
}

/// **A type declaration reaches the claims State only when it differs from the
/// declaration the State holds for the name**: the first is one event, the same
/// again adds none, and a changed one adds one. The same rule as the Dolt writer.
#[tokio::test]
async fn a_type_declaration_emits_only_when_it_differs_from_the_state() {
    use crate::memory::types::{DeclaredType, Field, ValueType};

    let fake = InMemoryMemory::seeded();
    let declared = |keys: &[&str]| {
        DeclaredType::new(
            "pledge",
            keys.iter()
                .map(|key| Field::new(key, ValueType::Text))
                .collect(),
        )
    };
    fake.declare_type(declared(&["witness"]), None)
        .await
        .expect("declared");
    fake.declare_type(declared(&["witness"]), None)
        .await
        .expect("declared again");
    fake.declare_type(declared(&["witness", "date"]), None)
        .await
        .expect("changed");

    let held = fake
        .claims_state()
        .declarations_of(&ObjectId("pledge".into()))
        .to_vec();
    assert_eq!(
        held,
        vec![
            declared(&["witness"]).normalized(),
            declared(&["witness", "date"]).normalized()
        ],
        "one event for the first declaration, none for the repeat, one for the change"
    );
}
