use super::*;
use crate::memory::Boot;

fn thing(handle: &str, badge: Option<&str>) -> Entity {
    Entity {
        folded_names: Vec::new(),
        id: EntityId(handle.into()),
        kind: EntityId(handle.into()).kind().expect("a test handle"),
        name: handle.into(),
        aliases: Vec::new(),
        source: "the roster".into(),
        crm: None,
        parent: None,
        boot: Boot::OnDemand,
        merged_into: None,
        badge: badge.map(str::to_string),
        archived: None,
    }
}

/// **The words of a claim are screened before a write, the way a capture
/// screens them.** A handle naming something passes, one naming nothing is
/// refused with the handle, an unknown stored mark is refused, and a handle in code
/// is not a mention at all.
#[test]
fn text_is_screened_for_the_mark_and_for_handles_that_name_nothing() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let known = [thing("person:milhouse", Some("k7h2mn"))];
    assert!(refusal_for_text(&["saw @person:milhouse"], &known).is_none());
    assert!(refusal_for_text(&["`@person:ralph` in code"], &known).is_none());
    assert_eq!(
        refusal_for_text(&["plain", "saw @person:ralph"], &known).map(|e| match e {
            super::super::MemoryError::UnknownEntity { attempted, .. } => attempted,
            other => panic!("{other:?}"),
        }),
        Some("person:ralph".to_string()),
    );
    assert!(refusal_for_text(&["copied @#k7h2mn"], &known).is_none());
    assert!(matches!(
        refusal_for_text(&["forged @#dead1"], &known),
        Some(super::super::MemoryError::InvalidFact(_))
    ));
}

/// 🚨 **The stored spelling, pinned as a literal.**
///
/// It is written into rows and nothing outside this process declares it, so
/// a change to it is a change every row written before it disagrees with —
/// and a row cannot fail a test. An assertion built from [`MARK`] would
/// move with the constant and pin nothing.
#[test]
fn a_stored_mention_is_an_at_sign_and_a_hash_and_the_badge() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let known = [thing("person:milhouse", Some("k7h2mn"))];
    assert_eq!(
        resolved("ask @person:milhouse about it", &known),
        Ok("ask @#k7h2mn about it".to_string()),
    );
}

/// **`@` in front of an ordinary word is ordinary text.** The
/// discriminator is the KIND, and the set of kinds is data this process
/// loaded — so a mention is a handle and nothing else has to be escaped.
#[test]
fn text_that_names_no_kind_is_left_alone() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let known = [thing("person:milhouse", Some("k7h2mn"))];
    let plain = "mail me @ home, or @sandwich:cheese, or @person";
    assert_eq!(resolved(plain, &known), Ok(plain.to_string()));
    assert_eq!(rendered(plain, &known), plain);
}

/// **Punctuation after a mention is punctuation.** A handle never ends in a
/// hyphen, and the character that ends a slug ends a badge, so a mention
/// reads the same in both directions.
#[test]
fn a_mention_ends_where_the_sentence_does() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let known = [thing("person:milhouse", Some("k7h2mn"))];
    assert_eq!(
        resolved("saw @person:milhouse, then @person:milhouse.", &known),
        Ok("saw @#k7h2mn, then @#k7h2mn.".to_string()),
    );
    assert_eq!(
        rendered("saw @#k7h2mn, then @#k7h2mn.", &known),
        "saw @person:milhouse, then @person:milhouse.",
    );
}

/// **A record the build supplies wears no badge**, so a mention of one is
/// kept exactly as written: there is no row to rename, and its handle is as
/// permanent as anything here.
#[test]
fn a_mention_of_something_wearing_no_badge_is_kept_as_written() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let known = [thing("view:loops", None)];
    assert_eq!(
        resolved("run @view:loops again", &known),
        Ok("run @view:loops again".to_string()),
    );
    assert_eq!(
        rendered("run @view:loops again", &known),
        "run @view:loops again",
    );
}

/// **A handle in a saved expression is stored as the badge its thing wears,
/// and read back as whatever handle wears it now.** The pair is the point: a
/// rename changes what the saved expression says and nothing is rewritten.
#[test]
fn a_handle_in_a_saved_expression_follows_the_thing_it_names() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let known = [thing("person:milhouse", Some("k7h2mn"))];
    let saved = bound_expression(
        r#"owner = person:milhouse && status in (now, "person:milhouse") && due < 2026-11-01"#,
        &known,
    )
    .expect("a handle that exists binds");
    assert_eq!(
        saved,
        r#"owner = @#k7h2mn && status in (now, "@#k7h2mn") && due < 2026-11-01"#,
    );
    assert_eq!(
        unbound_expression(&saved, &known),
        r#"owner = person:milhouse && status in (now, "person:milhouse") && due < 2026-11-01"#,
    );
    assert_eq!(
        unbound_expression(&saved, &[thing("person:ralph", Some("k7h2mn"))]),
        r#"owner = person:ralph && status in (now, "person:ralph") && due < 2026-11-01"#,
        "the badge answers to the new handle after a rename",
    );
}

/// **A thing's folded fields show a saved expression with the handles it
/// names, not the permanent ids it is stored with.** The write binds the
/// handles of a key declared to hold an expression; the fields door is the
/// read a view's `where` takes, so it unbinds them. A key that holds plain
/// text is served as written. Paired with the stored row, which does hold the
/// binding, so the case cannot pass on a build that never bound anything.
#[tokio::test]
async fn a_thing_serves_a_saved_expression_with_the_handles_it_names() {
    use crate::memory::types::{DeclaredType, Field, ValueType};
    use crate::memory::{Memory, NewEntity, NewFact};
    let inner = std::sync::Arc::new(crate::memory::testing::InMemoryMemory::booted());
    let store = Mentioning::new(inner.clone());
    store
        .declare_type(
            DeclaredType::new("saved", vec![Field::new("line", ValueType::Expression)]),
            None,
        )
        .await
        .expect("a declaration the store keeps");
    let holder = EntityId("thing:delta".into());
    let named = EntityId("thing:gamma".into());
    for (id, name) in [(&holder, "Delta"), (&named, "Gamma")] {
        store
            .add_entity(NewEntity::new(id.clone(), name, "test"))
            .await
            .expect("add ok")
            .written()
            .expect("nothing collides");
    }
    let fields: std::collections::BTreeMap<String, String> = [
        ("line".to_string(), "owner = thing:gamma".to_string()),
        ("note".to_string(), "thing:gamma at home".to_string()),
    ]
    .into_iter()
    .collect();
    store
        .capture(NewFact {
            sets: fields.keys().cloned().collect(),
            fields,
            ..NewFact::about(
                holder.clone(),
                "keeps a line",
                jiff::civil::date(2026, 8, 1),
            )
        })
        .await
        .expect("a line that names a thing that exists is stored")
        .written()
        .expect("nothing blocks it");
    let stored = inner.fields(&holder).await.expect("the stored map");
    assert_ne!(
        stored["line"], "owner = thing:gamma",
        "the store holds the binding, or this proves nothing: {stored:?}"
    );
    let served = store.fields(&holder).await.expect("the served map");
    assert_eq!(served["line"], "owner = thing:gamma");
    assert_eq!(
        served["note"], "thing:gamma at home",
        "a key that holds plain text is served as written"
    );
}

/// **What is not a handle that exists is not bound, and what cannot be
/// bound is refused.** A time is not a handle; a handle of a record the build
/// supplies wears no badge; a handle naming nothing is refused with the
/// nearest; an expression that does not parse is refused by the parser.
#[test]
fn a_saved_expression_binds_only_what_exists() {
    use super::super::MemoryError;
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let known = [
        thing("person:milhouse", Some("k7h2mn")),
        thing("view:loops", None),
    ];
    for kept in [
        "starts = 12:30",
        "view = view:loops",
        "owner = milhouse",
        "done",
    ] {
        assert_eq!(bound_expression(kept, &known).expect("it binds"), kept);
    }
    assert!(
        matches!(
            bound_expression("owner = person:ralph", &known),
            Err(MemoryError::UnknownEntity { attempted, .. }) if attempted == "person:ralph"
        ),
        "a handle that names nothing is refused with candidates"
    );
    assert!(matches!(
        bound_expression("owner = ", &known),
        Err(MemoryError::InvalidQuery(_))
    ));
}

/// **The render follows the row, not the spelling.** One badge, two
/// handles, and the same stored text reads as each in turn — which is the
/// whole of what a mention buys.
#[test]
fn one_stored_mention_reads_as_whichever_handle_wears_the_badge() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let stored = "the survey went out on @#k7h2mn";
    assert_eq!(
        rendered(stored, &[thing("thing:handcart", Some("k7h2mn"))]),
        "the survey went out on @thing:handcart",
    );
    assert_eq!(
        rendered(stored, &[thing("work:handcart", Some("k7h2mn"))]),
        "the survey went out on @work:handcart",
    );
}

/// 🚨 **The two shapes `rendered` cannot make followable are excluded, and
/// a genuine mention beside them is not** — the pairing is the point: a
/// scanner that called nothing followable would pass on the negative
/// halves alone.
#[test]
fn followable_finds_a_genuine_mention_and_leaves_the_two_dead_shapes_out() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let known = [thing("person:milhouse", Some("k7h2mn"))];
    let served = rendered(
        "ask @#k7h2mn about @person:contract-zzz and about @#dead1",
        &known,
    );
    assert_eq!(
        served,
        "ask @person:milhouse about @person:contract-zzz (no such handle) and about @#dead1 (gone)",
    );

    let spans = followable(&served);
    assert_eq!(
        spans,
        vec![(4..20, EntityId("person:milhouse".into()))],
        "found: {spans:?}, in: {served:?}",
    );
}

/// The mention layer forwards which keys of a record the store holds off the
/// thing.
#[tokio::test]
async fn the_mention_layer_forwards_the_keys_a_switched_store_holds_off_a_thing() {
    let store = Mentioning::new(std::sync::Arc::new(
        crate::memory::testing::InMemoryMemory::booted(),
    ));
    crate::memory::testing::contract::own_fields_stay_while_settings_and_computed_keys_reach(
        &store,
    )
    .await;
}

/// The mention layer forwards the writes of a key that stays on its record,
/// with the moment of each.
#[tokio::test]
async fn the_mention_layer_forwards_the_writes_that_stay_on_their_record() {
    let store = Mentioning::new(std::sync::Arc::new(
        crate::memory::testing::InMemoryMemory::booted(),
    ));
    crate::memory::testing::contract::a_key_that_stays_on_its_record_is_in_the_trails_with_its_moment(&store).await;
}

/// **The stored mark is jojobot's, so a caller's text may not carry one.**
///
/// A mark followed by a badge reads back as the handle of whatever wears
/// that badge, so text a caller wrote with one would be a link nothing
/// checked. Claim words, claim details and a thing's prose are all
/// refused, and the same sentence written as a handle is stored. A mark
/// with no badge after it is ordinary text.
#[tokio::test]
async fn a_caller_cannot_write_the_stored_mark_into_text() {
    use crate::memory::{Memory, NewEntity, NewFact};
    let store = Mentioning::new(std::sync::Arc::new(
        crate::memory::testing::InMemoryMemory::booted(),
    ));
    let holder = EntityId("thing:contract-forged-holder".into());
    let target = EntityId("thing:contract-forged-target".into());
    for (id, name) in [(&holder, "Holder"), (&target, "Target")] {
        store
            .add_entity(NewEntity::new(id.clone(), name, "test"))
            .await
            .expect("add ok")
            .written()
            .expect("nothing collides");
    }
    let day = jiff::civil::date(2026, 8, 1);
    let refused = |what: &str, outcome: Result<_, crate::memory::MemoryError>| {
        assert!(
            matches!(outcome, Err(crate::memory::MemoryError::InvalidFact(_))),
            "{what} carrying an unknown stored mark is refused: {:?}",
            outcome.map(|_: ()| ()),
        );
    };
    refused(
        "claim words",
        store
            .capture(NewFact::about(holder.clone(), "waits on @#k7h2mn", day))
            .await
            .map(|_| ()),
    );
    refused(
        "claim details",
        store
            .capture(NewFact {
                details: Some("because of @#k7h2mn".to_string()),
                ..NewFact::about(holder.clone(), "waits", day)
            })
            .await
            .map(|_| ()),
    );
    refused(
        "prose",
        store
            .set_prose(&holder, "see @#k7h2mn", None)
            .await
            .map(|_| ()),
    );
    // **The reason a thought is dropped is text a caller writes**, and a store
    // keeps it as the details of the claim it archives, so it is held to the
    // same rule as the details of any claim.
    refused(
        "a drop reason",
        store
            .capture(NewFact {
                drop: Some(crate::memory::FactAddress::new(
                    holder.clone(),
                    crate::memory::FactId("f1".to_string()),
                )),
                drop_because: Some("replaced by @#k7h2mn".to_string()),
                ..NewFact::about(holder.clone(), "a new thought", day)
            })
            .await
            .map(|_| ()),
    );
    // **A claim written in the same act as a thing's creation or edit is a
    // claim**, so it is held to the same rule.
    refused(
        "a first claim",
        store
            .add_entity_with_first_claim(
                crate::memory::NewEntity::new(
                    EntityId("thing:contract-forged-created".into()),
                    "Forged Created",
                    "test",
                ),
                NewFact::about(
                    EntityId("thing:contract-forged-created".into()),
                    "made with @#k7h2mn",
                    day,
                ),
            )
            .await
            .map(|_| ()),
    );
    refused(
        "a claim written with an edit",
        store
            .update_entity_with_claim(
                &holder,
                crate::memory::EntityPatch::default(),
                NewFact::about(holder.clone(), "edited with @#k7h2mn", day),
            )
            .await
            .map(|_| ()),
    );
    // The positives the refusals are measured against.
    store
        .capture(NewFact::about(
            holder.clone(),
            format!("waits on @{target}, mail me @# today"),
            day,
        ))
        .await
        .expect("a handle is written as a handle")
        .written()
        .expect("the claim lands");
    store
        .set_prose(&holder, "a mark with no badge, @# , is text", None)
        .await
        .expect("prose with a bare mark is stored");
}

/// **The first claim of a new thing is a caller's text too.** Creating a
/// thing with its first claim refuses an unknown stored mark in the claim's words
/// and in its details, exactly as a capture does, and nothing is made. The
/// same call with plain words lands the thing and its claim.
#[tokio::test]
async fn a_first_claim_cannot_carry_the_stored_mark() {
    use crate::memory::{Guarded, Memory, NewEntity, NewFact};
    let store = Mentioning::new(std::sync::Arc::new(
        crate::memory::testing::InMemoryMemory::booted(),
    ));
    let day = jiff::civil::date(2026, 8, 1);
    let listed = |id: &EntityId| {
        let store = &store;
        let id = id.clone();
        async move {
            store
                .list_entities(None)
                .await
                .expect("the roster reads")
                .iter()
                .any(|entity| entity.id == id)
        }
    };
    for (what, id, first) in [
        (
            "claim words",
            EntityId("thing:contract-forged-holder".into()),
            NewFact::about(
                EntityId("thing:contract-forged-holder".into()),
                "waits on @#k7h2mn",
                day,
            ),
        ),
        (
            "claim details",
            EntityId("thing:contract-forged-holder".into()),
            NewFact {
                details: Some("because of @#k7h2mn".to_string()),
                ..NewFact::about(
                    EntityId("thing:contract-forged-holder".into()),
                    "waits",
                    day,
                )
            },
        ),
    ] {
        let outcome = store
            .add_entity_with_first_claim(NewEntity::new(id.clone(), "Forged", "test"), first)
            .await;
        assert!(
            matches!(outcome, Err(crate::memory::MemoryError::InvalidFact(_))),
            "a first claim's {what} carrying an unknown stored mark is refused: {:?}",
            outcome.map(|_| ()),
        );
        assert!(!listed(&id).await, "a refused first claim made {id}");
    }

    let plain = EntityId("thing:contract-forged-holder".into());
    let Guarded::Written((entity, fact)) = store
        .add_entity_with_first_claim(
            NewEntity::new(plain.clone(), "Plain", "test"),
            NewFact::about(plain.clone(), "a mark with no badge, @# , is text", day),
        )
        .await
        .expect("plain words are a first claim")
    else {
        panic!("{plain} was expected to be written");
    };
    assert_eq!(entity.id, plain);
    assert!(fact.content.contains("is text"), "the claim reads back");
    assert!(listed(&plain).await, "the thing was made");
}

/// The handles `named` finds, as strings, so a case reads as a list.
fn names_in(text: &str) -> Vec<String> {
    named(text)
        .into_iter()
        .map(|handle| handle.as_str().to_string())
        .collect()
}

/// **Text inside an inline code span is not a mention** (rule 450). A
/// document that shows sample code holds handles that name nothing, and
/// reading them as links would refuse the whole write. **Paired with the
/// same handle outside code**, or a build that finds nothing anywhere
/// passes the first half.
#[test]
fn a_handle_inside_an_inline_code_span_is_not_a_mention() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    assert!(names_in("write `@person:contract-zzz` to link").is_empty());
    assert!(names_in("write ``@person:contract-zzz`` to link").is_empty());
    assert_eq!(
        names_in("write @person:contract-zzz to link"),
        ["person:contract-zzz"],
    );
    assert_eq!(
        names_in("`@person:milhouse` and @person:contract-zzz"),
        ["person:contract-zzz"],
        "only the handle outside the span is a mention",
    );
}

/// **A fenced block is code, of either fence character and any length, and
/// it closes only on a fence at least as long as the one that opened it.**
#[test]
fn a_handle_inside_a_fenced_block_is_not_a_mention() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    for text in [
        "before\n```\n@person:contract-zzz\n```\nafter",
        "before\n~~~\n@person:contract-zzz\n~~~\nafter",
        "before\n````rust\n@person:contract-zzz\n```\n@person:contract-zzz\n````\nafter",
        "before\n~~~~~\n@person:contract-zzz\n~~~\n```\n~~~~~\nafter",
        "before\n  ```\n  @person:contract-zzz\n  ```\nafter",
    ] {
        assert!(names_in(text).is_empty(), "{text:?}");
    }
    assert_eq!(
        names_in("```\ncode\n```\nthen @person:contract-zzz"),
        ["person:contract-zzz"],
        "a mention after the fence closed is a mention",
    );
}

/// **Code is left exactly as written in both directions, and a mention
/// beside it still is not.** Stored and served are the same bytes for the
/// code, and the real handle outside it is stored as a badge and served as
/// the handle.
#[test]
fn resolving_and_rendering_leave_code_alone_and_the_rest_a_link() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let known = [thing("person:milhouse", Some("k7h2mn"))];
    let written = "`@person:milhouse` and @person:milhouse\n```\n@person:milhouse\n```\n";
    let stored = resolved(written, &known).expect("kinds are loaded");
    assert_eq!(
        stored,
        "`@person:milhouse` and @#k7h2mn\n```\n@person:milhouse\n```\n",
    );
    assert_eq!(rendered(&stored, &known), written);
    // A handle that names nothing: marked outside code, untouched inside.
    assert_eq!(
        rendered("`@person:contract-zzz`", &known),
        "`@person:contract-zzz`",
    );
    assert_eq!(
        rendered("@person:contract-zzz", &known),
        "@person:contract-zzz (no such handle)",
    );
}

/// **A mention stored inside code before this rule still renders as its
/// handle.** A handle in a code span used to be resolved like any other, so
/// rows hold the badge there, and serving the badge is the failure this
/// module exists to prevent. The mark is jojobot's own and is read
/// wherever it sits; a handle a caller typed is what code leaves alone.
#[test]
fn a_mark_stored_inside_code_renders_as_its_handle() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let known = [thing("person:milhouse", Some("k7h2mn"))];
    assert_eq!(
        rendered("see `@#k7h2mn` and\n```\n@#k7h2mn\n```\n", &known),
        "see `@person:milhouse` and\n```\n@person:milhouse\n```\n",
    );
}

/// **The stored mark is a forgery in code as it is outside it**, because
/// the render above reads it wherever it sits.
#[test]
fn a_stored_mark_is_a_forgery_inside_code_too() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    assert!(forged_mark("`@#k7h2mn`"));
    assert!(forged_mark("```\n@#k7h2mn\n```"));
    assert!(forged_mark("@#k7h2mn"));
    assert!(!forged_mark("`@person:milhouse` and no mark"));
}

/// **`followable` draws no link out of code.**
#[test]
fn followable_finds_nothing_inside_code() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    assert!(followable("`@person:milhouse`").is_empty());
    assert!(followable("```\n@person:milhouse\n```").is_empty());
    assert_eq!(followable("@person:milhouse").len(), 1);
}

/// **An unclosed fence or span is not code.** A stray fence that turned the
/// rest of the text into code would drop every link after it without a
/// word, so it is read as the words it is and a handle after it is still a
/// mention, which a write guard then answers loudly if it names nothing.
#[test]
fn an_unclosed_fence_or_span_is_not_code() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    assert_eq!(
        names_in("```\n@person:contract-zzz"),
        ["person:contract-zzz"]
    );
    assert_eq!(
        names_in("a ` lone tick then @person:contract-zzz"),
        ["person:contract-zzz"],
    );
    assert_eq!(
        names_in("``double then `single` then @person:contract-zzz"),
        ["person:contract-zzz"],
        "a span closes on a run of the same length only",
    );
    // An unclosed fence does not hide a closed pair after it.
    assert!(names_in("```\ntext\n~~~\n@person:contract-zzz\n~~~").is_empty());
}

/// **A span does not cross a blank line**, so a lone tick cannot pair with a
/// tick in the next paragraph and hide what lies between them.
#[test]
fn an_inline_span_does_not_cross_a_paragraph() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    assert_eq!(
        names_in("a `tick\n\n@person:contract-zzz then another` tick"),
        ["person:contract-zzz"],
    );
    assert!(names_in("a `tick\n@person:contract-zzz\nstill the span` ends").is_empty());
}

/// **An indented line is not code.** It cannot be told cheaply from the
/// continuation of a list item, and skipping it would hide a real mention.
#[test]
fn an_indented_line_is_not_code() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    assert_eq!(
        names_in("a list:\n\n    @person:contract-zzz"),
        ["person:contract-zzz"],
    );
}
