//! **Who holds a key, and in what order**, as [`Memory::keys_holding`] and
//! [`Memory::ordered_first_k`] answer it on every store.
//!
//! The expected sets are written out by hand from what each leaf means, so a
//! store that answers from values it keeps is held to the meaning and not to the
//! full read it is meant to agree with.

use super::support::{add, capture, state};
use super::*;
use crate::memory::filter::{Direction, Op, Test};
use std::collections::BTreeMap;

fn handles(kind: &str, slugs: &[&str]) -> Vec<EntityId> {
    let mut all: Vec<EntityId> = slugs
        .iter()
        .map(|slug| EntityId(format!("{kind}:{slug}")))
        .collect();
    all.sort();
    all
}

/// One thing holding these keys, folded onto it by a single claim.
async fn holding<M: Memory>(store: &M, id: &EntityId, held: &[(&str, &str)]) {
    add(
        store,
        NewEntity::new(id.clone(), id.slug(), "contract-fixture"),
    )
    .await;
    let fields: BTreeMap<String, String> = held
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    capture(
        store,
        state(NewFact {
            fields,
            ..NewFact::about(id.clone(), "what it holds", date(2026, 6, 1))
        }),
    )
    .await;
}

/// **A leaf is answered with the things it keeps, whole and trimmed, exactly as
/// the filter reads a value.** A list holds whole items; an ordering reads a
/// value as the type asked and a value that is not one does not match; a
/// negation asks that the key is held; an archived thing is left out.
pub async fn keys_holding_answers_each_leaf_as_a_filter_reads_it<M: Memory>(store: &M) {
    let ids = |slugs: &[&str]| handles("thing", slugs);
    let a = EntityId("thing:contract-holders-oak".into());
    let b = EntityId("thing:contract-holders-elm".into());
    let c = EntityId("thing:contract-holders-fir".into());
    let d = EntityId("thing:contract-holders-ash".into());
    let e = EntityId("thing:contract-holders-yew".into());
    let gone = EntityId("thing:contract-holders-box".into());
    holding(
        store,
        &a,
        &[
            ("holders_tags", "kitchen, garden"),
            ("holders_due", "2026-01-15"),
            ("holders_points", "3"),
            ("holders_status", "now"),
        ],
    )
    .await;
    holding(
        store,
        &b,
        &[
            ("holders_tags", "kitchen"),
            ("holders_due", " 2026-03-01 "),
            ("holders_points", "-0"),
            ("holders_status", " done "),
        ],
    )
    .await;
    holding(
        store,
        &c,
        &[
            ("holders_tags", "kitchenette,garden"),
            ("holders_due", "soon"),
            ("holders_points", "inf"),
            ("holders_status", "done"),
        ],
    )
    .await;
    holding(store, &d, &[("holders_points", "0")]).await;
    holding(store, &e, &[("holders_status", "next")]).await;
    holding(
        store,
        &gone,
        &[("holders_tags", "kitchen"), ("holders_status", "now")],
    )
    .await;
    store
        .archive_entity(&gone, "a fixture taken out", None)
        .await
        .expect("archive_entity should succeed");
    // The same key on a thing of another kind.
    let topic = EntityId("topic:contract-holders-bay".into());
    holding(store, &topic, &[("holders_status", "now")]).await;

    let ask = |key: &'static str, test: Test, kind| async move {
        store
            .keys_holding(key, &test, kind)
            .await
            .expect("keys_holding should succeed")
    };
    let thing = Some(EntityKind::THING);
    let text = |s: &str| s.to_string();

    assert_eq!(
        ask("holders_status", Test::Exists, thing).await,
        ids(&[
            "contract-holders-oak",
            "contract-holders-elm",
            "contract-holders-fir",
            "contract-holders-yew"
        ]),
        "the key held, an archived thing left out"
    );
    assert_eq!(
        ask("holders_status", Test::Equals(text("done")), thing).await,
        ids(&["contract-holders-elm", "contract-holders-fir"]),
        "a value is whole and trimmed on both sides"
    );
    assert_eq!(
        ask(
            "holders_status",
            Test::In(vec![text("now"), text("next")]),
            thing
        )
        .await,
        ids(&["contract-holders-oak", "contract-holders-yew"])
    );
    assert_eq!(
        ask("holders_status", Test::NotEquals(text("done")), thing).await,
        ids(&["contract-holders-oak", "contract-holders-yew"]),
        "a thing that lacks the key is not 'not done'"
    );
    assert_eq!(
        ask(
            "holders_status",
            Test::NotIn(vec![text("done"), text("next")]),
            thing
        )
        .await,
        ids(&["contract-holders-oak"])
    );
    assert_eq!(
        ask("holders_tags", Test::Contains(text("kitchen")), thing).await,
        ids(&["contract-holders-oak", "contract-holders-elm"]),
        "a whole item and never a prefix of one"
    );
    assert_eq!(
        ask("holders_tags", Test::Contains(text("garden")), thing).await,
        ids(&["contract-holders-oak", "contract-holders-fir"]),
        "items are read apart by the comma, spaces or none"
    );
    assert_eq!(
        ask(
            "holders_tags",
            Test::Contains(text("kitchen, garden")),
            thing
        )
        .await,
        Vec::<EntityId>::new(),
        "an item with a comma in it is no item"
    );
    assert_eq!(
        ask("holders_tags", Test::Contains(text("")), thing).await,
        Vec::<EntityId>::new()
    );
    assert_eq!(
        ask(
            "holders_due",
            Test::Order(Op::LessOrEqual, ValueType::Date, text("2026-03-01")),
            thing
        )
        .await,
        ids(&["contract-holders-oak", "contract-holders-elm"]),
        "a value that is no date does not match"
    );
    assert_eq!(
        ask(
            "holders_due",
            Test::Order(Op::Greater, ValueType::Date, text("2026-01-15")),
            thing
        )
        .await,
        ids(&["contract-holders-elm"])
    );
    assert_eq!(
        ask(
            "holders_points",
            Test::Order(Op::GreaterOrEqual, ValueType::Number, text("0")),
            thing
        )
        .await,
        ids(&[
            "contract-holders-oak",
            "contract-holders-elm",
            "contract-holders-fir",
            "contract-holders-ash"
        ]),
        "negative zero is zero, and a number with no end is a number"
    );
    assert_eq!(
        ask(
            "holders_points",
            Test::Order(Op::Less, ValueType::Number, text("1")),
            thing
        )
        .await,
        ids(&["contract-holders-elm", "contract-holders-ash"])
    );
    assert_eq!(
        ask("holders_status", Test::Equals(text("now")), None).await,
        {
            let mut both = ids(&["contract-holders-oak"]);
            both.push(topic.clone());
            both.sort();
            both
        },
        "with no kind named, every kind answers"
    );
    assert_eq!(
        ask("holders_nobody_writes_this", Test::Exists, thing).await,
        Vec::<EntityId>::new()
    );
}

/// **The head of an ordering is the front of what the ordering places.** A value
/// that is no value of the type has no place, a tie is never cut, the direction
/// turns it round, `among` limits who is placed, and the head says whether
/// anything stands behind it.
pub async fn ordered_first_k_places_things_by_value_and_never_cuts_a_tie<M: Memory>(store: &M) {
    let slugs = [
        ("contract-ordered-walnut", "5"),
        ("contract-ordered-spruce", "1"),
        ("contract-ordered-hazel", "3"),
        ("contract-ordered-willow", "3"),
        ("contract-ordered-poplar", " 3 "),
        ("contract-ordered-larch", "soon"),
        ("contract-ordered-sycamore", "inf"),
        ("contract-ordered-juniper", "-2"),
    ];
    for (slug, value) in slugs {
        holding(
            store,
            &EntityId(format!("thing:{slug}")),
            &[("ordered_points", value)],
        )
        .await;
    }
    // No such key at all, and one that is archived.
    holding(
        store,
        &EntityId("thing:contract-ordered-redwood".into()),
        &[],
    )
    .await;
    let out = EntityId("thing:contract-ordered-hickory".into());
    holding(store, &out, &[("ordered_points", "-9")]).await;
    store
        .archive_entity(&out, "a fixture taken out", None)
        .await
        .expect("archive_entity should succeed");

    async fn ask<M: Memory>(
        store: &M,
        direction: Direction,
        among: Option<&[EntityId]>,
        k: usize,
    ) -> crate::memory::holders::OrderedHead {
        store
            .ordered_first_k(
                "ordered_points",
                ValueType::Number,
                direction,
                None,
                among,
                k,
            )
            .await
            .expect("ordered_first_k should succeed")
    }
    let names = |head: &crate::memory::holders::OrderedHead| -> Vec<String> {
        head.head
            .iter()
            .map(|(id, _)| {
                id.slug()
                    .trim_start_matches("contract-ordered-")
                    .to_string()
            })
            .collect()
    };
    let held = |head: &crate::memory::holders::OrderedHead, slug: &str| -> Option<String> {
        head.head
            .iter()
            .find(|(id, _)| id.slug() == format!("contract-ordered-{slug}"))
            .map(|(_, held)| held.clone())
    };

    // Six things have a place: -2, 1, 3, 3, 3, 5. The other three hold a word, a
    // number with no end and nothing.
    let first = ask(store, Direction::Ascending, None, 2).await;
    assert_eq!(names(&first), ["juniper", "spruce"]);
    assert!(first.cut, "the 3s and the 5 stand behind");

    let ties = ask(store, Direction::Ascending, None, 3).await;
    let mut three = names(&ties);
    assert_eq!(
        three.len(),
        5,
        "the third place is a tie of three, none cut"
    );
    assert_eq!(three.remove(0), "juniper");
    assert_eq!(three.remove(0), "spruce");
    three.sort();
    assert_eq!(three, ["hazel", "poplar", "willow"]);
    assert!(ties.cut, "the 5 stands behind");
    assert_eq!(
        held(&ties, "poplar").as_deref(),
        Some(" 3 "),
        "each value comes as it is held"
    );

    let all = ask(store, Direction::Ascending, None, 100).await;
    assert_eq!(
        all.head.len(),
        6,
        "a word, a number with no end and no value have no place"
    );
    assert!(!all.cut);

    let last = ask(store, Direction::Descending, None, 1).await;
    assert_eq!(names(&last), ["walnut"]);
    assert!(last.cut);

    let nothing = ask(store, Direction::Ascending, None, 0).await;
    assert!(nothing.head.is_empty());
    assert!(nothing.cut, "things are placed and none was asked for");

    let only = [
        EntityId("thing:contract-ordered-walnut".into()),
        EntityId("thing:contract-ordered-hazel".into()),
        EntityId("thing:contract-ordered-larch".into()),
        EntityId("thing:contract-ordered-redwood".into()),
    ];
    let among = ask(store, Direction::Descending, Some(&only), 1).await;
    assert_eq!(names(&among), ["walnut"]);
    assert!(among.cut, "gamma stands behind within those asked about");
    let among_all = ask(store, Direction::Descending, Some(&only), 10).await;
    assert_eq!(names(&among_all), ["walnut", "hazel"]);
    assert!(!among_all.cut);

    let dated = store
        .ordered_first_k(
            "ordered_points",
            ValueType::Date,
            Direction::Ascending,
            None,
            None,
            5,
        )
        .await
        .expect("ordered_first_k should succeed");
    assert!(
        dated.head.is_empty() && !dated.cut,
        "no value of these is a date, so none has a place"
    );

    // A thing of another kind holds the key too: the kind named decides who is
    // placed, and none named places every kind.
    holding(
        store,
        &EntityId("topic:contract-ordered-planter".into()),
        &[("ordered_points", "99")],
    )
    .await;
    let of_kind = |kind| async move {
        store
            .ordered_first_k(
                "ordered_points",
                ValueType::Number,
                Direction::Descending,
                kind,
                None,
                1,
            )
            .await
            .expect("ordered_first_k should succeed")
    };
    assert_eq!(names(&of_kind(None).await), ["planter"]);
    assert_eq!(names(&of_kind(Some(EntityKind::THING)).await), ["walnut"]);
}
