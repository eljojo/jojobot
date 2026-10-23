use super::*;

#[test]
fn person_id_prefixes_a_bare_handle_but_respects_a_typed_one() {
    assert_eq!(EntityId::person("person:alpha").as_str(), "person:alpha");
    assert_eq!(EntityId::person("person:alpha").as_str(), "person:alpha");
}

#[test]
fn validate_subject_accepts_ids_and_rejects_adversarial_ones() {
    // **The set is this case's subject, not its setup.** What makes an
    // adversarial id adversarial is that its kind half names no kind the
    // set holds, so the refusals below are statements about the loaded set.
    crate::memory::kinds::load_shipped();
    assert!(validate_subject(&EntityId::person("person:alpha")).is_ok());
    assert!(validate_subject(&EntityId("project:jojobot-server".into())).is_ok());
    // Injection vectors: newline, pipe, header, fence, space, uppercase, empty.
    for bad in [
        "person:contract-a|b",
        "a\nb",
        "### forged",
        "a`b",
        "a b",
        "Person:Alpha",
        "",
    ] {
        assert!(
            validate_subject(&EntityId(bad.into())).is_err(),
            "must reject {bad:?}"
        );
    }
}

/// Every shipped kind round-trips through its wire token, and nothing else
/// parses — a token nobody declared can never enter the store.
#[test]
fn the_shipped_kinds_round_trip_and_the_set_is_closed() {
    // **The set is this case's subject, not its setup.** The case walks the
    // shipped tokens and then asserts the set holds nothing else, so what
    // is loaded is the whole content of both assertions.
    crate::memory::kinds::load_shipped();
    let all = [
        (EntityKind::PERSON, "person"),
        (EntityKind::PROJECT, "project"),
        (EntityKind::PLACE, "place"),
        (EntityKind::EVENT, "event"),
        (EntityKind::WORK, "work"),
        (EntityKind::THING, "thing"),
        (EntityKind::ORG, "org"),
        (EntityKind::TOPIC, "topic"),
        (EntityKind::BOT, "bot"),
        (EntityKind::PET, "pet"),
        (EntityKind::RHYTHM, "rhythm"),
        (EntityKind::PROMISE, "promise"),
        (EntityKind::MACHINE, "machine"),
        (EntityKind::VIEW, "view"),
        (EntityKind::SESSION, "session"),
        (EntityKind::THREAD, "thread"),
        (EntityKind::DECISION, "decision"),
        (EntityKind::ROLE, "role"),
        (EntityKind::VOUCHER, "voucher"),
        (EntityKind::CHALLENGE, "challenge"),
        (EntityKind::DOCUMENT, "document"),
        (EntityKind::LIST, "list"),
        (EntityKind::USAGE, "usage"),
        (EntityKind::TRIP, "trip"),
    ];
    for (kind, token) in all {
        assert_eq!(kind.as_token(), token);
        assert_eq!(EntityKind::from_token(token), Some(kind));
    }
    assert_eq!(
        EntityKind::ALL.len(),
        all.len(),
        "every shipped kind is named above, and no other",
    );
    for unknown in ["receipt", "self", "Person", "", "peson"] {
        assert_eq!(
            EntityKind::from_token(unknown),
            None,
            "{unknown:?} is not a kind"
        );
    }
}
