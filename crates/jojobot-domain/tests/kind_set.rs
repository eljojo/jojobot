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
use jojobot_domain::memory::{EntityId, EntityKind, validate_subject};

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

/// **The seed writes before it reads, and that order is what leaves a process
/// able to read a handle.**
///
/// A seed that read first would load whatever was there before it wrote —
/// nothing, on an instance that never had the rows — so the rows would end up
/// right and no handle would resolve until the next boot. The rows alone
/// cannot show that: this empties the set first, so what the seed loads is the
/// only thing answering.
///
/// It is here rather than in the shared contract for the reason this whole
/// file exists: emptying the set reaches every test running beside it.
/// It drives the future on a runtime of its own rather than being an async
/// test, because the turn above is a plain lock and holding one across an await
/// is the shape that deadlocks a runtime.
#[test]
fn the_seed_writes_before_it_reads() {
    let _turn = in_turn();
    let store = jojobot_domain::memory::testing::InMemoryMemory::default();
    kinds::load::<[&str; 0], &str>([]);

    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime for this one case")
        .block_on(kinds::seed(&store))
        .expect("the kinds are seeded");

    assert_eq!(
        EntityId::person("kind-set-reader").kind(),
        Some(EntityKind::PERSON),
        "after the seed this process reads a handle, which it can only do if the seed \
         loaded what it had just written",
    );

    kinds::load_shipped();
}

/// **Standing a store up is not booting a process, and only one of the two
/// fills the set.**
///
/// The double's constructor used to load the set as a side effect of being
/// built, so every case in a binary got the set from whichever case happened to
/// build a fake first. That is the hazard this whole file exists for, one layer
/// down: a case could be green because of another case.
///
/// **Both halves, and the negative alone proves nothing.** Standing a store up
/// and finding the set still empty is satisfied by a build where nothing ever
/// loads it; booting one and finding a handle parses is satisfied by the old
/// build that loaded on construction. Together they say which step did it.
///
/// It drives the future on a runtime of its own rather than being an async
/// test, for the reason the seed case above does: the turn is a plain lock and
/// holding one across an await is the shape that deadlocks a runtime.
#[test]
fn a_store_stood_up_loads_nothing_and_a_booted_one_loads_what_it_holds() {
    let _turn = in_turn();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime for this one case");

    kinds::load::<[&str; 0], &str>([]);
    let _stood_up = jojobot_domain::memory::testing::InMemoryMemory::new();
    assert!(
        !kinds::known("person"),
        "building a store must not fill the set this process parses against — a \
         constructor that does it makes every case beside it depend on the order \
         they ran",
    );

    let _booted = jojobot_domain::memory::testing::InMemoryMemory::booted();
    assert!(
        kinds::known("person"),
        "booting one does fill it, which is the step a case asks for by asking for a \
         booted store",
    );
    assert_eq!(
        EntityId::person("kind-set-reader").kind(),
        Some(EntityKind::PERSON),
        "and the set is filled well enough to parse a handle, which is what a case \
         needs it for",
    );

    // **From the store's answer, not from the constant.** A kind this store
    // holds and the shipped list does not is known once the store boots, which
    // no build reading `SHIPPED` could answer for. It is asked of a store the
    // case wrote to BEFORE the boot, because a store built and booted in one
    // call holds exactly the constant and can never tell the two apart.
    let holds_a_gadget = jojobot_domain::memory::testing::InMemoryMemory::new();
    runtime
        .block_on(jojobot_domain::memory::Memory::declare_kind(
            &holds_a_gadget,
            "gadget",
            jojobot_domain::memory::types::Origin::Declared,
            Vec::new(),
        ))
        .expect("a caller declares a kind");
    kinds::load::<[&str; 0], &str>([]);
    holds_a_gadget.boot();
    assert!(
        kinds::known("gadget"),
        "what is loaded is what the store answers, so a kind nobody compiled is \
         parsed too",
    );
    assert!(
        kinds::known("person"),
        "…and the shipped rows the same store holds are loaded beside it",
    );

    kinds::load_shipped();
}

/// **A refusal says which of the three things is wrong, and none of them
/// recites a hardcoded list of kinds.**
///
/// The set is data, so a sentence naming ten nouns goes stale the first time an
/// instance declares an eleventh — and a caller reading it about a kind their
/// own store holds is being told something false.
///
/// The three answers have three repairs, which is why one sentence cannot
/// serve them: fix the handle's shape, boot a process that seeded, or declare
/// the kind. **Only the last one names kinds, and it names the ones this
/// process actually loaded** — that is the way forward rule 68 asks for, and it
/// is a read of the set rather than a list somebody wrote down.
///
/// It lives here because it empties the set, which reaches every test beside it.
#[test]
fn a_refusal_says_which_thing_is_wrong_and_recites_no_written_list() {
    let _turn = in_turn();
    kinds::load_shipped();

    let shape = validate_subject(&EntityId("not a handle".into()))
        .expect_err("a handle that is no handle is refused")
        .to_string();
    assert!(
        shape.contains("kind:slug"),
        "a malformed handle is told the grammar: {shape}",
    );

    let undeclared = validate_subject(&EntityId("gadget:one".into()))
        .expect_err("a kind nobody declared is refused")
        .to_string();
    assert!(
        undeclared.contains("gadget") && undeclared.contains("person"),
        "the refusal names the token sent and the kinds this process holds: {undeclared}",
    );

    kinds::load::<[&str; 0], &str>([]);
    // The handle here carries a prefix that is no shipped kind, so a kind
    // token appearing in this refusal came from the sentence rather than from
    // the caller's own words.
    let unseeded = validate_subject(&EntityId("gadget:one".into()))
        .expect_err("an unseeded process cannot read a handle")
        .to_string();
    kinds::load_shipped();
    assert!(
        !unseeded.contains("no kind is named"),
        "an unseeded process does not blame the handle: {unseeded}",
    );

    // **Whole words, not substrings.** `nothing` carries `thing` inside it, and
    // a check that counted that would fail on a sentence naming no kind at all.
    for said in [&shape, &unseeded] {
        let named: Vec<&str> = EntityKind::ALL
            .into_iter()
            .map(|kind| kind.as_token())
            .filter(|token| {
                said.split(|c: char| !c.is_ascii_alphanumeric())
                    .any(|word| word == *token)
            })
            .collect();
        assert!(
            named.is_empty(),
            "this refusal knows nothing about which kinds exist, so it must name none, and it \
             names {named:?}: {said}",
        );
    }
}

/// **An unloaded set makes a value type give a WRONG ANSWER, not an error.**
///
/// `ValueType::Reference` decides whether a value is a handle by asking it for
/// its kind, and an unloaded set answers "no kind" — so the type reports that a
/// perfectly well-formed handle is not a reference. Nothing panics and nothing
/// refuses: the check just returns the wrong thing, and a reader hitting it
/// concludes the value type is broken.
///
/// **That is worse than the failures beside it and it is why this is pinned
/// separately from the seeding.** A crash says where to look; a wrong answer
/// about the logic under test sends the reader to the wrong file.
#[test]
fn a_value_type_answers_about_a_handle_only_when_the_set_is_loaded() {
    let _turn = in_turn();
    use jojobot_domain::memory::types::ValueType;

    kinds::load_shipped();
    assert!(
        ValueType::Reference.holds("place:moes"),
        "on a loaded process a handle IS a reference",
    );

    kinds::load::<[&str; 0], &str>([]);
    assert!(
        !ValueType::Reference.holds("place:moes"),
        "on an unloaded one the same handle reads as no reference at all — the answer is \
         wrong rather than absent, which is what this case exists to record",
    );
    kinds::load_shipped();
}
