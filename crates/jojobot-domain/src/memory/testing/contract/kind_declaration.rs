use super::*;

/// **A kind declaration answers whether the store holds anything different
/// afterwards.** A fold that keeps a copy of the store reads it again only when
/// the answer is yes, so the answer has to be exact in both directions: a repeat
/// that changes nothing says no, and anything that moves a row, a key, a key's
/// type or its owner says yes.
///
/// **Every line pairs a yes with the no that follows it**, so a store that
/// always says yes and a store that always says no each fail a different line.
pub async fn a_kind_declaration_says_whether_it_changed_the_store<M: Memory>(store: &M) {
    use crate::memory::types::Field;

    let keys = |names: &[&str]| -> Vec<Field> {
        names
            .iter()
            .map(|name| Field::new(name, ValueType::Text))
            .collect()
    };

    // A kind the build ships, new to the store, then again as it was.
    assert!(
        store
            .declare_kind("nu", Origin::Shipped, keys(&["a", "b"]))
            .await
            .expect("a kind is declared"),
        "a kind the store did not hold changed it",
    );
    assert!(
        !store
            .declare_kind("nu", Origin::Shipped, keys(&["a", "b"]))
            .await
            .expect("the same again"),
        "the same declaration again changed nothing",
    );
    // Naming no keys takes none away and changes nothing.
    assert!(
        !store
            .declare_kind("nu", Origin::Shipped, Vec::new())
            .await
            .expect("a start names no keys"),
        "a declaration with no keys changed the store",
    );
    // A key added, a key's type changed and a key's order changed each change it.
    assert!(
        store
            .declare_kind("nu", Origin::Shipped, keys(&["a", "b", "c"]))
            .await
            .expect("a key is added"),
        "a new key changed nothing",
    );
    assert!(
        store
            .declare_kind(
                "nu",
                Origin::Shipped,
                vec![
                    Field::new("a", ValueType::Number),
                    Field::new("b", ValueType::Text),
                    Field::new("c", ValueType::Text),
                ],
            )
            .await
            .expect("a key's type changes"),
        "a key's new type changed nothing",
    );
    assert!(
        store
            .declare_kind("nu", Origin::Shipped, keys(&["b", "a", "c"]))
            .await
            .expect("the order changes"),
        "a new order changed nothing",
    );
    // A caller's key on a kind the build ships (rule 566), then again.
    assert!(
        store
            .declare_kind("nu", Origin::Declared, keys(&["d"]))
            .await
            .expect("a caller adds a key"),
        "a caller's key changed nothing",
    );
    assert!(
        !store
            .declare_kind("nu", Origin::Declared, keys(&["d"]))
            .await
            .expect("the same key again"),
        "the same key again changed the store",
    );
    // The build's start over the kind with a caller's key changes nothing.
    assert!(
        !store
            .declare_kind("nu", Origin::Shipped, keys(&["b", "a", "c"]))
            .await
            .expect("the start again"),
        "the start changed the store because a caller's key was there",
    );
    // **Only a key's owner changes**: the build starts to ship the key a caller
    // added, with the same name and type. Nothing else moves, and the repeat is no.
    assert!(
        store
            .declare_kind("nu", Origin::Shipped, keys(&["b", "a", "c", "d"]))
            .await
            .expect("the build ships the caller's key"),
        "a key that changed owner alone changed nothing",
    );
    assert!(
        !store
            .declare_kind("nu", Origin::Shipped, keys(&["b", "a", "c", "d"]))
            .await
            .expect("the same again"),
        "the same owner again changed the store",
    );
    // A kind of the caller's own, and the same again.
    assert!(
        store
            .declare_kind("xi", Origin::Declared, keys(&["e"]))
            .await
            .expect("a caller declares a kind"),
        "a caller's new kind changed nothing",
    );
    assert!(
        !store
            .declare_kind("xi", Origin::Declared, keys(&["e"]))
            .await
            .expect("the same kind again"),
        "the same kind again changed the store",
    );
}
