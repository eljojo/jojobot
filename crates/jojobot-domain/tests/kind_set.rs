//! **The kind set, in a process of its own.**
//!
//! The set is process-wide, which is what makes a parse a memory read instead
//! of a store read — and what makes a case that REPLACES it a case that
//! reaches every other test running beside it. These two replace it, so they
//! get a binary to themselves rather than a mutex nobody would remember to
//! take: a unit test beside them would pass alone and fail in a full run, for
//! a reason invisible in its own file.
//!
//! **If you add a case that calls `load`, add it here.**

use jojobot_domain::memory::kinds::{self, NotAKind};
use jojobot_domain::memory::{EntityId, EntityKind};

/// These two share the one set even here, so they take turns.
fn in_turn() -> std::sync::MutexGuard<'static, ()> {
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
    TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// **The set answers from what was loaded, not from what was compiled.**
///
/// This is the case that tells the two silent failures apart.
///
/// A build where the set ships as nothing has two ways to look healthy. One:
/// the set stays empty and everything is refused — loud, and caught by the
/// shipped token below. Two: the lookup quietly falls back to the compiled
/// list, which passes every test written against the ten and changes nothing
/// at all — **caught only by a token that is NOT one of the ten**, because a
/// fallback has no way to know one, and by a shipped token that was not
/// loaded, which is the same discriminator from the other side.
#[test]
fn a_loaded_kind_is_known_and_the_compiled_list_cannot_answer_for_it() {
    let _turn = in_turn();
    kinds::load(["person", "gadget"]);

    assert!(
        kinds::known("gadget"),
        "a kind the store holds is known, and no compiled list contains this one",
    );
    assert!(
        kinds::known("person"),
        "…and a shipped kind loaded beside it is too",
    );
    assert!(
        !kinds::known("pet"),
        "a shipped kind that was NOT loaded is unknown, which is what says this answer \
         comes from the set rather than from the list it replaces",
    );

    // **Through the parse a handle really takes**, not only the set's own
    // question. A fallback to the compiled list can live in `from_token`
    // rather than in the set, and an assertion that never calls it would not
    // notice: `pet` is compiled and NOT loaded here, so a parse that answers
    // for it is answering from the list this change replaces.
    assert_eq!(
        EntityId::new(EntityKind::PERSON, "alpha").kind(),
        Some(EntityKind::PERSON),
        "a handle whose kind is loaded parses",
    );
    assert_eq!(
        EntityId("gadget:one".to_string())
            .kind()
            .map(|k| k.as_token()),
        Some("gadget"),
        "…and so does one whose kind only the store knows",
    );
    assert_eq!(
        EntityId("pet:santas-little-helper".to_string()).kind(),
        None,
        "a kind that is compiled but not loaded does not parse, which is where a \
         fallback would show",
    );

    kinds::load_shipped();
}

/// **An unloaded set refuses in its own words.**
///
/// Two failures with two repairs, and they must not arrive wearing one
/// sentence: a process that seeded nothing refuses `person` exactly as it
/// refuses a typo, and a reader told "unknown kind" about the most ordinary
/// handle in the system goes hunting for a misspelling that is not there.
#[test]
fn an_unloaded_set_and_an_undeclared_token_are_different_refusals() {
    let _turn = in_turn();
    kinds::load::<[&str; 0], &str>([]);

    assert_eq!(
        kinds::resolve("person"),
        Err(NotAKind::SetNeverLoaded),
        "nothing seeded this process, and that is not the handle's fault",
    );

    kinds::load_shipped();
    let refused = kinds::resolve("gadget").expect_err("no kind is named gadget");
    match refused {
        NotAKind::NotDeclared { token, known } => {
            assert_eq!(token, "gadget");
            assert!(
                known.contains(&"person".to_string()),
                "the refusal carries the kinds that ARE here: {known:?}",
            );
        }
        other => panic!("a loaded set names what it does not hold: {other:?}"),
    }
}
