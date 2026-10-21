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

/// **The things that name a handle are found by the key that names it.** A key
/// whose value is another thing's handle is asked like any other: the handle
/// written is the handle the key holds, wherever the holder sits. A decision that
/// replaces another is found this way, whichever topic it is filed under, and an
/// archived one is left out. A handle nothing names is answered with nothing.
pub async fn keys_holding_finds_the_decisions_that_name_a_handle<M: Memory>(store: &M) {
    let old = EntityId("decision:contract-holders-old".into());
    let nobody = EntityId("decision:contract-holders-nobody".into());
    let beside = EntityId("decision:contract-holders-beside".into());
    let away = EntityId("decision:contract-holders-away".into());
    let gone = EntityId("decision:contract-holders-struck".into());
    for id in [&old, &nobody] {
        add(
            store,
            NewEntity::new(id.clone(), id.slug(), "contract-fixture"),
        )
        .await;
    }
    // One replacement beside the old one's topic, one under another topic.
    let north = EntityId("topic:contract-holders-harbor".into());
    let south = EntityId("topic:contract-holders-quarry".into());
    add(
        store,
        NewEntity::new(north.clone(), north.slug(), "contract-fixture"),
    )
    .await;
    add(
        store,
        NewEntity::new(south.clone(), south.slug(), "contract-fixture"),
    )
    .await;
    for (id, parent, replaces) in [
        (&beside, &north, &old),
        (&away, &south, &old),
        (&gone, &north, &old),
    ] {
        add(
            store,
            NewEntity {
                parent: Some(parent.clone()),
                ..NewEntity::new(id.clone(), id.slug(), "contract-fixture")
            },
        )
        .await;
        capture(
            store,
            state(NewFact {
                fields: BTreeMap::from([("replaces".to_string(), replaces.to_string())]),
                ..NewFact::about(id.clone(), "what it replaces", date(2026, 6, 1))
            }),
        )
        .await;
    }
    store
        .archive_entity(&gone, "struck out", None)
        .await
        .expect("archive_entity should succeed");

    let named_by = |handle: &EntityId| {
        let test = Test::Equals(handle.to_string());
        async move {
            store
                .keys_holding("replaces", &test, Some(EntityKind::DECISION))
                .await
                .expect("keys_holding should succeed")
        }
    };
    let mut found = vec![away.clone(), beside.clone()];
    found.sort();
    assert_eq!(
        named_by(&old).await,
        found,
        "both replacements, whichever topic holds them, and not the archived one"
    );
    assert_eq!(
        named_by(&nobody).await,
        Vec::<EntityId>::new(),
        "a handle nothing names has no holder"
    );
}

/// What a read of the decision `id` says replaced it, through the walk.
async fn replaced_by<M: Memory>(store: &M, id: EntityId) -> Option<graph::ReplacedBy> {
    graph::walk(
        store,
        &[],
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(id),
                ..graph::Selection::default()
            },
            include: graph::Include {
                facts: false,
                prose: false,
                stood_for: false,
                status: None,
            },
            follow: None,
            history: None,
            masked: Default::default(),
        },
    )
    .await
    .expect("a handle is a selection")
    .objects
    .first()
    .and_then(|object| object.replaced_by.clone())
}

/// **A name that resembles no other decision's.** The creation screen refuses
/// a name that contains another's or shares two rare words with it, so a family
/// of fixtures is named by the last word of its slug.
fn display_name(id: &EntityId) -> String {
    id.slug().rsplit('-').next().unwrap_or_default().to_string()
}

/// **The slug word of the `n`th decision of a chain**: eight hex digits from a
/// fixed pseudo-random run, so no two steps are a letter or two apart, because
/// the creation screen also refuses a slug that close to another.
fn step_word(n: usize) -> String {
    let mut x: u32 = 0x2545_f491;
    for _ in 0..=n {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
    }
    format!("{x:08x}")
}

/// Decisions written in `order`, each naming the one it replaces, and then the
/// ones in `retired` archived. A pointer at nothing is refused, so every
/// decision exists before any names another.
async fn decisions<M: Memory>(
    store: &M,
    order: &[(&EntityId, Option<&EntityId>)],
    retired: &[&EntityId],
) {
    for (id, _) in order {
        add(
            store,
            NewEntity::new((*id).clone(), display_name(id), "contract-fixture"),
        )
        .await;
    }
    for (id, replaces) in order {
        let fields: BTreeMap<String, String> = replaces
            .map(|old| ("replaces".to_string(), old.to_string()))
            .into_iter()
            .collect();
        capture(
            store,
            state(NewFact {
                fields,
                ..NewFact::about(
                    (*id).clone(),
                    format!("the words of {}", id.slug()),
                    date(2026, 6, 1),
                )
            }),
        )
        .await;
    }
    for old in retired {
        store
            .archive_entity(old, "replaced", None)
            .await
            .expect("archive_entity should succeed");
    }
}

/// **A decision replaced by a retired one reads on to the decision that stands.**
/// The read of the oldest names its direct replacer and, because that one was
/// replaced too, the one standing at the end; the read of the middle one names
/// the standing one alone. A loop of two retired decisions that name each other
/// ends without a standing end. Read through the walk, so a store that cannot
/// give a retired decision's keys to it is held to the answer.
pub async fn a_retired_chain_of_decisions_reads_on_to_the_one_that_stands<M: Memory>(store: &M) {
    let first = EntityId("decision:contract-lineage-ember".into());
    let second = EntityId("decision:contract-lineage-flint".into());
    let third = EntityId("decision:contract-lineage-gale".into());
    let red = EntityId("decision:contract-lineage-lark".into());
    let blue = EntityId("decision:contract-lineage-moss".into());
    let chain = [
        (&first, None),
        (&second, Some(&first)),
        (&third, Some(&second)),
        (&red, Some(&blue)),
        (&blue, Some(&red)),
    ];
    // A pointer at nothing is refused, so every decision exists before any
    // names another.
    for (id, _) in chain {
        add(
            store,
            NewEntity::new(id.clone(), id.slug(), "contract-fixture"),
        )
        .await;
    }
    for (id, replaces) in chain {
        let fields: BTreeMap<String, String> = replaces
            .map(|old| ("replaces".to_string(), old.to_string()))
            .into_iter()
            .collect();
        capture(
            store,
            state(NewFact {
                fields,
                ..NewFact::about(
                    id.clone(),
                    format!("the words of {}", id.slug()),
                    date(2026, 6, 1),
                )
            }),
        )
        .await;
    }
    for old in [&first, &second, &red, &blue] {
        store
            .archive_entity(old, "replaced", None)
            .await
            .expect("archive_entity should succeed");
    }
    let replacement_of = |id: &EntityId| replaced_by(store, id.clone());
    let oldest = replacement_of(&first)
        .await
        .expect("the oldest is replaced");
    assert_eq!(oldest.handle, second, "the direct replacer, though retired");
    let end = oldest
        .standing
        .expect("the chain ends at a standing decision");
    assert_eq!(end.handle, third);
    assert_eq!(
        end.words,
        vec!["the words of contract-lineage-gale".to_string()]
    );
    let middle = replacement_of(&second)
        .await
        .expect("the middle is replaced");
    assert_eq!(middle.handle, third);
    assert!(middle.standing.is_none(), "the direct replacer stands");
    assert!(
        replacement_of(&third).await.is_none(),
        "a standing decision is not replaced"
    );
    let looped = replacement_of(&red).await.expect("each names the other");
    assert_eq!(looped.handle, blue);
    assert!(looped.standing.is_none(), "nothing in a loop stands");
}

/// **Two decisions that replace one are told apart the same way every time.** A
/// standing replacer is read before a retired one, and between two of a kind the
/// lower handle is. The pairs are written higher handle first in one case and
/// lower first in the next, so a store that answered by the order it was
/// written in is held to the handle.
pub async fn two_replacers_are_chosen_by_standing_then_by_handle<M: Memory>(store: &M) {
    let id = |slug: &str| EntityId(format!("decision:contract-tie-{slug}"));
    // Two that stand: the lower handle, though it was written second.
    let (sorrel, spruce, aspen) = (id("sorrel"), id("spruce"), id("aspen"));
    decisions(
        store,
        &[
            (&sorrel, None),
            (&spruce, Some(&sorrel)),
            (&aspen, Some(&sorrel)),
        ],
        &[&sorrel],
    )
    .await;
    let read = replaced_by(store, sorrel.clone()).await.expect("replaced");
    assert_eq!(read.handle, aspen, "of two that stand, the lower handle");

    // One retired and one standing: the one that stands, though its handle is
    // the higher.
    let (thistle, bramble, yarrow) = (id("thistle"), id("bramble"), id("yarrow"));
    decisions(
        store,
        &[
            (&thistle, None),
            (&bramble, Some(&thistle)),
            (&yarrow, Some(&thistle)),
        ],
        &[&thistle, &bramble],
    )
    .await;
    let read = replaced_by(store, thistle.clone()).await.expect("replaced");
    assert_eq!(read.handle, yarrow, "the standing one, though higher");
    assert!(read.standing.is_none(), "it stands, so nothing further");

    // Two retired at one step, written higher first: the lower handle is
    // followed, and its chain is the one that reaches the decision that stands.
    let (nettle, clover, tansy, zephyr) = (id("nettle"), id("clover"), id("tansy"), id("zephyr"));
    decisions(
        store,
        &[
            (&nettle, None),
            (&tansy, Some(&nettle)),
            (&clover, Some(&nettle)),
            (&zephyr, Some(&clover)),
        ],
        &[&nettle, &tansy, &clover],
    )
    .await;
    let read = replaced_by(store, nettle.clone()).await.expect("replaced");
    assert_eq!(read.handle, clover, "of two retired, the lower handle");
    assert_eq!(
        read.standing.expect("clover leads to zephyr").handle,
        zephyr
    );

    // The same with the lower handle written first.
    let (heather, dahlia, willow, quince) =
        (id("heather"), id("dahlia"), id("willow"), id("quince"));
    decisions(
        store,
        &[
            (&heather, None),
            (&dahlia, Some(&heather)),
            (&willow, Some(&heather)),
            (&quince, Some(&dahlia)),
        ],
        &[&heather, &dahlia, &willow],
    )
    .await;
    let read = replaced_by(store, heather.clone()).await.expect("replaced");
    assert_eq!(read.handle, dahlia, "the same answer in the other order");
    assert_eq!(
        read.standing.expect("dahlia leads to quince").handle,
        quince
    );
}

/// **A chain of replacements is followed as far as the bound and no further.**
/// The last decision stands. With the oldest read, a chain whose standing end is
/// 32 steps on is followed to it, and one a step
/// longer ends with nothing standing named but still names the direct replacer.
pub async fn a_chain_of_replacements_is_followed_to_its_bound_and_no_further<M: Memory>(store: &M) {
    // The value, not the constant: a bound asserted through itself moves with it.
    let bound = 32;
    for (steps, followed) in [(bound, true), (bound + 1, false)] {
        let name = |n: usize| {
            if followed {
                EntityId(format!("decision:contract-reach-{}", step_word(n)))
            } else {
                EntityId(format!("decision:contract-span-{}", step_word(n + 100)))
            }
        };
        let ids: Vec<EntityId> = (0..=steps).map(name).collect();
        let order: Vec<(&EntityId, Option<&EntityId>)> = ids
            .iter()
            .enumerate()
            .map(|(n, id)| (id, n.checked_sub(1).map(|before| &ids[before])))
            .collect();
        let retired: Vec<&EntityId> = ids[..steps].iter().collect();
        decisions(store, &order, &retired).await;
        let read = replaced_by(store, ids[0].clone()).await.expect("replaced");
        assert_eq!(read.handle, ids[1], "the direct replacer, {steps} steps");
        match (followed, read.standing) {
            (true, Some(end)) => assert_eq!(end.handle, ids[steps]),
            (false, None) => {}
            (followed, end) => panic!(
                "a chain {steps} steps long: followed {followed}, standing {:?}",
                end.map(|end| end.handle)
            ),
        }
    }
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
