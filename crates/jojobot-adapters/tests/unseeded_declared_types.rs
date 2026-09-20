//! **What a caller meets reading a declared type's schema on a process that
//! loaded no kinds.**
//!
//! A type may declare a reference-typed key that names the kind it points
//! at (`reference:place`). Reading that key back off a stored row asks the
//! kind set the same way parsing a handle does, and an unloaded process
//! fails every kind the same way a typo would. `gather_types` used to
//! answer that by silently retyping the key to plain text — a caller
//! reading the schema would see a value type the type was never declared
//! with, and nothing would say why.
//!
//! **A test binary of its own, because the set is process-wide.** Any case
//! running beside this one would load it, and this one would then be
//! asserting about a state it is not in.

use std::path::PathBuf;

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::free_port;
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use jojobot_domain::memory::{EntityKind, Memory, kinds};

/// A directory of this run's own, removed when it is done.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn a_declared_type_with_a_reference_field_refuses_to_read_back_on_an_unloaded_process() {
    let path = std::env::temp_dir().join(format!(
        "jojobot-unseeded-declared-types-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let mut server = Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    migrate::run(server.pool()).await.expect("the schema");
    let store = DoltMemory::open(server.pool().clone());

    // A type declared while the set is loaded, the way a caller always
    // declares one — the set is only ever emptied afterwards, on a
    // restart that could not reach the store.
    kinds::seed(&store).await.expect("the kinds are declared");
    store
        .declare_type(DeclaredType::new(
            "trip-fixture",
            vec![Field::pointing_at("destination", EntityKind::PLACE)],
        ))
        .await
        .expect("a caller declares a type");

    // The loaded column, as the control: the field reads back exactly as
    // declared.
    let loaded = store
        .declared_types()
        .await
        .expect("a loaded process reads its own declarations");
    let field = loaded
        .iter()
        .find(|t| t.name == "trip-fixture")
        .and_then(|t| t.field("destination"))
        .expect("the type and its field are both there");
    assert_eq!(
        field.holds,
        ValueType::Reference,
        "on a loaded process the field reads back as the reference it was declared as",
    );

    kinds::load::<[&str; 0], &str>([]);

    let refused = store
        .declared_types()
        .await
        .expect_err("a process that cannot resolve a kind cannot read a schema naming one");
    let said = refused.to_string();
    assert!(
        said.contains("never loaded"),
        "the refusal names the failure a caller can act on — nothing seeded this process: {said}",
    );

    server.stop().await;
    drop(scratch);
    kinds::load_shipped();
}
