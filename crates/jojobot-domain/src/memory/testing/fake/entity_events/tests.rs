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
    assert_eq!(
        held(&fake, "person:homer")
            .expect("held")
            .1
            .metadata
            .get("prose")
            .map(String::as_str),
        Some("A line of prose."),
        "the prose write is in the State"
    );

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
    // **The prose is the thing's, so a rename keeps it** under the new handle, as
    // the real store's row keeps its column, and so does a write made after.
    let prose_of = |fake: &InMemoryMemory, spelling: &str| {
        held(fake, spelling)
            .expect("held")
            .1
            .metadata
            .get("prose")
            .cloned()
    };
    assert_eq!(
        prose_of(&fake, "person:marge").as_deref(),
        Some("A line of prose."),
        "the rename dropped the prose from the State"
    );
    fake.update_entity(
        &marge,
        EntityPatch {
            name: Some("Marge B".into()),
            ..EntityPatch::default()
        },
    )
    .await
    .expect("the edit after the rename lands");
    assert_eq!(
        prose_of(&fake, "person:marge").as_deref(),
        Some("A line of prose."),
        "a write after the rename dropped the prose from the State"
    );

    // **Prose written under the new handle moves with the next rename**, and a
    // write after that still finds it: the handle it was written under is neither
    // the one the thing was made under nor the one it wears now.
    fake.set_prose(&marge, "Another line.", None)
        .await
        .expect("the prose lands under the new handle");
    let lisa = handle("person:lisa");
    fake.rename_entity(
        &marge,
        &lisa,
        None,
        date(2026, 8, 3),
        Override::none(),
        None,
    )
    .await
    .expect("the second rename lands");
    assert_eq!(
        prose_of(&fake, "person:lisa").as_deref(),
        Some("Another line."),
        "the second rename dropped the prose written after the first"
    );
    fake.update_entity(
        &lisa,
        EntityPatch {
            name: Some("Lisa S".into()),
            ..EntityPatch::default()
        },
    )
    .await
    .expect("the edit after the second rename lands");
    assert_eq!(
        prose_of(&fake, "person:lisa").as_deref(),
        Some("Another line."),
        "a write after the second rename dropped the prose"
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

/// **After a reclaim the claims State holds no declaration for the kind, as the
/// rows hold none**, and a reclaim of a kind with nothing left to take changes
/// nothing. The State holds the declaration because declaring the kind put it
/// there, so the case runs the whole flow a store runs.
#[tokio::test]
async fn a_reclaim_leaves_the_state_as_the_rows_hold_the_type() {
    use crate::memory::types::{Field, Origin, ValueType};

    crate::memory::kinds::load_shipped();
    let fake = InMemoryMemory::seeded();
    let fields = vec![Field::new("witness", ValueType::Text)];
    fake.declare_kind("pledge", Origin::Shipped, fields)
        .await
        .expect("the kind is declared");
    let kind = ObjectId("pledge".into());
    assert!(
        fake.claims_state().declared(&kind).is_some(),
        "the State holds the declaration before the reclaim"
    );

    fake.reclaim_kind("pledge")
        .await
        .expect("the reclaim lands");
    assert!(
        fake.claims_state().declared(&kind).is_none(),
        "the rows hold no declaration after a reclaim, so the State holds none"
    );

    fake.reclaim_kind("pledge")
        .await
        .expect("a second reclaim lands");
    assert!(
        fake.claims_state().declared(&kind).is_none(),
        "a reclaim with nothing left to take changes nothing"
    );
}

/// **A kind's declaration reaches the claims State only when it differs from the
/// one the State holds for the name**, as a type's does: the first declaration
/// is one event, the same again adds none, and changed keys add one. The
/// declaration is what the rows hold for the kind, normalized as the Dolt writer
/// normalizes it.
#[tokio::test]
async fn a_kind_declaration_emits_only_when_it_differs_from_the_state() {
    use crate::memory::types::{DeclaredType, Field, Origin, ValueType};

    crate::memory::kinds::load_shipped();
    let fake = InMemoryMemory::seeded();
    let keys = |names: &[&str]| -> Vec<Field> {
        names
            .iter()
            .map(|name| Field::new(name, ValueType::Text))
            .collect()
    };
    let declared = |names: &[&str]| DeclaredType {
        name: "pledge".to_string(),
        fields: keys(names),
        origin: Origin::Shipped,
        must: None,
    };
    fake.declare_kind("pledge", Origin::Shipped, keys(&["witness"]))
        .await
        .expect("declared");
    fake.declare_kind("pledge", Origin::Shipped, keys(&["witness"]))
        .await
        .expect("declared again");
    fake.declare_kind("pledge", Origin::Shipped, keys(&["witness", "date"]))
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

/// **The declaration the State holds for a kind is the one the rows hold after the
/// write**, with every key the kind retains, and not the keys the call brought.
/// A caller adding a key to a shipped kind leaves the shipped key in the rows; a
/// call naming no keys takes none away.
#[tokio::test]
async fn a_kind_declaration_in_the_state_keeps_every_key_the_rows_keep() {
    use crate::memory::types::{Field, Origin, ValueType};

    crate::memory::kinds::load_shipped();
    let fake = InMemoryMemory::seeded();
    let keys = |names: &[&str]| -> Vec<Field> {
        names
            .iter()
            .map(|name| Field::new(name, ValueType::Text))
            .collect()
    };
    let kind = ObjectId("pledge".into());
    let state_keys = |fake: &InMemoryMemory| -> Vec<String> {
        let mut named: Vec<String> = fake
            .claims_state()
            .declared(&kind)
            .expect("the State holds a declaration")
            .fields
            .iter()
            .map(|field| field.key.clone())
            .collect();
        named.sort();
        named
    };

    fake.declare_kind("pledge", Origin::Shipped, keys(&["witness"]))
        .await
        .expect("the shipped key is declared");
    fake.declare_kind("pledge", Origin::Declared, keys(&["date"]))
        .await
        .expect("a caller adds a key");
    assert_eq!(
        state_keys(&fake),
        ["date", "witness"],
        "the State lost the shipped key the rows keep"
    );
    assert_eq!(
        fake.claims_state().declared(&kind).cloned(),
        fake.kind_keys_of("pledge")
            .first()
            .map(crate::memory::types::DeclaredType::normalized),
        "the State and the rows disagree on the declaration"
    );

    fake.declare_kind("pledge", Origin::Shipped, Vec::new())
        .await
        .expect("a call naming no keys lands");
    assert_eq!(
        state_keys(&fake),
        ["date", "witness"],
        "a call naming no keys took keys out of the State"
    );

    // **A kind that names no keys is held with none**, under its row's origin, as
    // the Dolt writer holds it.
    fake.declare_kind("oath", Origin::Shipped, Vec::new())
        .await
        .expect("a kind naming no keys lands");
    assert_eq!(
        fake.claims_state()
            .declared(&ObjectId("oath".into()))
            .cloned(),
        Some(
            crate::memory::types::DeclaredType {
                name: "oath".to_string(),
                fields: Vec::new(),
                origin: Origin::Shipped,
                must: None,
            }
            .normalized()
        ),
        "a kind with no keys is held empty in the State"
    );
}
