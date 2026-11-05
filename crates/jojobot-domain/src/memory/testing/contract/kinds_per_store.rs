use super::*;

/// **Each store answers for the kinds it declares itself.** Two stores in one
/// process declare different kinds. In the same read each accepts its own kind
/// and refuses the other's, and the refusal names only its own kinds. A store
/// that answered from a set shared by the process would accept both, or
/// neither.
///
/// It takes two stores because one store cannot show the difference: the case
/// is about what a store does when another is open beside it.
pub async fn two_stores_each_see_only_the_kinds_they_declare<A: Memory, B: Memory>(
    first: &A,
    second: &B,
) {
    use crate::memory::kinds::NotAKind;

    first
        .declare_kind("sloth", Origin::Declared, Vec::new())
        .await
        .expect("the first store declares its kind");
    second
        .declare_kind("walrus", Origin::Declared, Vec::new())
        .await
        .expect("the second store declares its kind");

    assert_eq!(
        first.kind_declared("sloth").await.expect("the first reads"),
        Ok(()),
        "a store accepts the kind it declared"
    );
    assert_eq!(
        second
            .kind_declared("walrus")
            .await
            .expect("the second reads"),
        Ok(()),
        "a store accepts the kind it declared"
    );
    for (store_says, own, other) in [
        (first.kind_declared("walrus").await, "sloth", "walrus"),
        (second.kind_declared("sloth").await, "walrus", "sloth"),
    ] {
        let Err(NotAKind::NotDeclared { token, known }) = store_says.expect("the store reads")
        else {
            panic!("a store refuses a kind only the other declares: {other}");
        };
        assert_eq!(token, other);
        assert!(
            known.iter().any(|kind| kind == own),
            "the refusal names the store's own kind: {known:?}"
        );
        assert!(
            !known.iter().any(|kind| kind == other),
            "the refusal does not name the other store's kind: {known:?}"
        );
    }
}

/// **A kind is declared under a name that could be a handle's kind**, the same
/// shape parsing accepts, so a store never holds a kind no handle can wear.
pub async fn a_kind_declaration_is_checked_for_shape<M: Memory>(store: &M) {
    for token in ["", "two words", "person:homer", "a/b"] {
        let refused = store
            .declare_kind(token, Origin::Declared, Vec::new())
            .await;
        assert!(
            matches!(refused, Err(MemoryError::InvalidType(_))),
            "{token:?} is not a kind's name: {refused:?}"
        );
    }
    let held = store.declared_kinds().await.expect("the store reads");
    assert!(
        held.iter().all(|(held, _)| held != "two words"),
        "a refused declaration leaves no kind behind"
    );
    store
        .declare_kind("otter", Origin::Declared, Vec::new())
        .await
        .expect("a well formed name is declared");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::testing::InMemoryMemory;

    #[tokio::test]
    async fn a_fake_checks_a_kind_declaration_for_shape() {
        a_kind_declaration_is_checked_for_shape(&InMemoryMemory::new()).await;
    }

    #[tokio::test]
    async fn two_fakes_each_see_only_the_kinds_they_declare() {
        two_stores_each_see_only_the_kinds_they_declare(
            &InMemoryMemory::new(),
            &InMemoryMemory::new(),
        )
        .await;
    }
}
