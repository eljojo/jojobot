//! **Where a store's own kind set refuses a handle.** The set is the store's,
//! so the refusal does not depend on what any other store in the process holds.

use super::*;
use crate::memory::kinds::KindSet;
use crate::memory::types::Origin;

fn declared(tokens: &[&str]) -> KindSet {
    KindSet::new(
        tokens
            .iter()
            .map(|token| (token.to_string(), Origin::Declared))
            .collect(),
    )
}

fn id(handle: &str) -> EntityId {
    EntityId(handle.to_string())
}

/// What a refusal of the kind half is, with the wording left to the display.
#[derive(Debug, PartialEq)]
enum Refusal {
    Never,
    NotDeclared,
    Shape,
    Fine,
}

fn refusal(answer: Result<(), MemoryError>) -> Refusal {
    match answer {
        Ok(()) => Refusal::Fine,
        Err(MemoryError::KindsNeverLoaded { attempted: Some(_) }) => Refusal::Never,
        Err(MemoryError::InvalidSubject(why)) if why.contains("no kind is named") => {
            Refusal::NotDeclared
        }
        Err(MemoryError::InvalidSubject(_)) => Refusal::Shape,
        Err(other) => panic!("not a subject refusal: {other:?}"),
    }
}

/// **The decision is one table.** Shape is judged before the kind, an empty
/// set is told apart from a typo, and a declared kind passes.
#[test]
fn a_subject_is_judged_by_shape_then_by_the_kinds_the_store_declares() {
    //   (the store's kinds, the handle, the refusal)
    let table = [
        (declared(&["sloth"]), "sloth:slow", Refusal::Fine),
        (declared(&["sloth"]), "walrus:tusky", Refusal::NotDeclared),
        (declared(&[]), "sloth:slow", Refusal::Never),
        (declared(&["sloth"]), "sloth:Not Valid", Refusal::Shape),
        (declared(&[]), "sloth:Not Valid", Refusal::Shape),
    ];
    for (kinds, handle, expected) in table {
        assert_eq!(
            refusal(validate_subject_in(&kinds, &id(handle))),
            expected,
            "{kinds:?} / {handle}"
        );
    }
}

fn claim(subject: &str) -> NewFact {
    NewFact::about(id(subject), "a claim", jiff::civil::date(2026, 8, 1))
}

/// **Every verb family asks the fake's own set.** The process set is filled
/// with the shipped kinds, so a store that still asked it would accept the
/// empty store's `person` and refuse the declared `sloth`.
#[tokio::test]
async fn each_verb_family_asks_the_stores_own_kinds() {
    let _process = InMemoryMemory::booted();
    let never_seeded = InMemoryMemory::default();
    let sloths = InMemoryMemory::default();
    sloths
        .declare_kind("sloth", Origin::Declared, Vec::new())
        .await
        .expect("the store declares its kind");

    // A claim written.
    assert_eq!(
        refusal(never_seeded.capture(claim("person:homer")).await.map(drop)),
        Refusal::Never
    );
    assert_eq!(
        refusal(sloths.capture(claim("walrus:tusky")).await.map(drop)),
        Refusal::NotDeclared
    );
    // An entity written.
    assert_eq!(
        refusal(
            never_seeded
                .archive_entity(&id("person:homer"), "done", None)
                .await
                .map(drop)
        ),
        Refusal::Never
    );
    assert_eq!(
        refusal(
            sloths
                .archive_entity(&id("walrus:tusky"), "done", None)
                .await
                .map(drop)
        ),
        Refusal::NotDeclared
    );
    // A read.
    let address = |handle: &str| FactAddress::new(id(handle), FactId("f1".into()));
    assert_eq!(
        refusal(
            never_seeded
                .built_on(&address("person:homer"))
                .await
                .map(drop)
        ),
        Refusal::Never
    );
    assert_eq!(
        refusal(sloths.built_on(&address("walrus:tusky")).await.map(drop)),
        Refusal::NotDeclared
    );
    // And a declared kind gets past the kind check to the next refusal.
    assert!(matches!(
        sloths.archive_entity(&id("sloth:slow"), "done", None).await,
        Err(MemoryError::UnknownEntity { .. })
    ));
}
