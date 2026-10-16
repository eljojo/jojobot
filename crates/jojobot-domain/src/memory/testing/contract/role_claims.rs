//! **The claim's own exclusivity, proven against any [`Memory`] adapter.**
//!
//! A role is its own object, a child of the bot that holds it, and its holder
//! and claim moment are decided atomically inside the same write that lands
//! them — see [`crate::memory::refuses_role_write`] and
//! [`crate::session::claim_role`]. That is a property of the STORE, not of
//! the orientation door that calls it, so it belongs here rather than only
//! in an MCP-level test: the fake and the real adapter must both answer for
//! it, or a fake that only looks race-free is standing in for a real store
//! that still has one.
//!
//! **Only the role object is read.** A role claimed before the role became an
//! object lives in the bot's own `role/<role>/holder` and
//! `role/<role>/claimed_at`; the start's pass carries such a claim onto the
//! object and takes the keys off the bot, so the claim stays held.

use super::support::{add, capture, computed, computed_patch, ensure};
use super::*;

/// A role object for one case, and the bot it is the child of. Each case has
/// its own pair, so no case reads another's lease.
struct Seat {
    /// The role object.
    role: EntityId,
    /// The role's name, which is the slug of its handle.
    name: String,
    /// The bot that holds it, and the caller the case writes as.
    bot: EntityId,
}

async fn a_seat<M: Memory>(store: &M, bot: &str, role: &str) -> Seat {
    let bot = EntityId(bot.to_string());
    ensure(store, &bot).await;
    let role = EntityId(role.to_string());
    let name = role.as_str().trim_start_matches("role:").to_string();
    add(
        store,
        NewEntity {
            parent: Some(bot.clone()),
            ..NewEntity::new(role.clone(), name.as_str(), "contract-fixture")
        },
    )
    .await;
    Seat { role, name, bot }
}

fn role_fields(claimant: &str, at: jiff::Timestamp) -> std::collections::BTreeMap<String, String> {
    [
        (
            crate::session::ROLE_HOLDER.to_string(),
            claimant.to_string(),
        ),
        (crate::session::ROLE_CLAIMED_AT.to_string(), at.to_string()),
    ]
    .into_iter()
    .collect()
}

/// The fields the old shape kept on the bot.
fn old_role_fields(
    role: &str,
    claimant: &str,
    at: jiff::Timestamp,
) -> std::collections::BTreeMap<String, String> {
    [
        (
            crate::session::role_keys::role_holder_key(role),
            claimant.to_string(),
        ),
        (
            crate::session::role_keys::role_claimed_at_key(role),
            at.to_string(),
        ),
    ]
    .into_iter()
    .collect()
}

/// **A thing's folded fields, read the served way** — duplicated from
/// `base::thing_fields` rather than reused: that function is private to a
/// file this module does not edit, per the batch this shipped in.
async fn folded_fields_of<M: Memory>(
    store: &M,
    id: &EntityId,
) -> std::collections::BTreeMap<String, String> {
    graph::walk(
        store,
        &[],
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(id.clone()),
                ..graph::Selection::default()
            },
            include: graph::Include {
                facts: false,
                prose: false,
                stood_for: false,
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
    .unwrap_or_else(|| panic!("a walk from {id} must answer with the thing itself"))
    .fields
    .clone()
}

/// **A second `capture` naming the same role, while the first claim is
/// fresh, is refused** — the capture-side atomic block, reached directly.
/// Every MCP-level claim test after the very first goes through
/// `update_fact` instead, because a claim record already exists by then;
/// this is the one path those tests never touch, on the fake or the real
/// store.
pub async fn a_second_capture_racing_the_first_is_refused<M: Memory>(store: &M) {
    let seat = a_seat(
        store,
        "bot:contract-role-capture-race",
        "role:contract-capture-race",
    )
    .await;
    let now = crate::session::testing::contract::epoch();

    capture(
        store,
        computed(NewFact {
            fields: role_fields("delta", now),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        }),
    )
    .await;

    let refused = store
        .capture(computed(NewFact {
            fields: role_fields("epsilon", now),
            ..NewFact::about(
                seat.role.clone(),
                "epsilon claims the role too",
                date(2026, 7, 1),
            )
        }))
        .await;
    assert!(
        matches!(
            &refused,
            Err(MemoryError::RoleTaken { role, holder, .. })
                if *role == seat.name && holder == "delta"
        ),
        "a second capture while the lease is fresh must be refused, naming the first holder: \
         {refused:?}",
    );

    let fields = folded_fields_of(store, &seat.role).await;
    assert_eq!(
        fields.get(crate::session::ROLE_HOLDER),
        Some(&"delta".to_string()),
        "the refused second capture must not have moved the holder: {fields:?}",
    );
}

/// **A rival claimant through `update_fact`, while the lease is fresh, is
/// refused** — the renewal-side atomic block, the path a real second
/// claimant takes once a claim record already exists. Ported so the real
/// store answers for it directly, not only through the MCP-level boot-door
/// tests that already covered it for the fake.
pub async fn a_rival_update_fact_claim_is_refused_while_the_lease_is_fresh<M: Memory>(store: &M) {
    let seat = a_seat(
        store,
        "bot:contract-role-update-race",
        "role:contract-update-race",
    )
    .await;
    let now = crate::session::testing::contract::epoch();

    let claimed = capture(
        store,
        computed(NewFact {
            fields: role_fields("delta", now),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        }),
    )
    .await;

    let refused = store
        .update_fact(
            &claimed.address(),
            computed_patch(FactPatch {
                fields: role_fields("epsilon", now),
                ..FactPatch::default()
            }),
            &seat.bot,
        )
        .await;
    assert!(
        matches!(
            &refused,
            Err(MemoryError::RoleTaken { role, holder, .. })
                if *role == seat.name && holder == "delta"
        ),
        "a rival update_fact claim while the lease is fresh must be refused: {refused:?}",
    );
}

/// **Two captures on the same role, run genuinely concurrently, and at most
/// one wins.** `tokio::join!` polls both futures rather than awaiting one to
/// completion before starting the other — the shape that would surface a
/// lock dropped between the read and the write, where a sequential case
/// cannot: two callers awaited one after another never observe each other's
/// in-flight state, however the store is built.
///
/// **The loser's own error is one of two, both correct, and the difference
/// is the adapter's, not this test's.** Against the fake, a single
/// `std::sync::Mutex` serializes the two calls outright, so the second one
/// to acquire it already sees the first one's write and is told
/// `RoleTaken`. Against the real store, two transactions can both start
/// before either commits, so the database's own optimistic concurrency
/// (`MemoryError::Conflict`, SQLSTATE `40001`) can catch the collision
/// instead — a real, documented, non-error outcome that means "retry the
/// same call", not a defect in the atomic check.
pub async fn two_concurrent_captures_race_and_exactly_one_wins<M: Memory>(store: &M) {
    let seat = a_seat(
        store,
        "bot:contract-role-concurrent-race",
        "role:contract-concurrent-race",
    )
    .await;
    let now = crate::session::testing::contract::epoch();

    let (a, b) = tokio::join!(
        store.capture(computed(NewFact {
            fields: role_fields("delta", now),
            ..NewFact::about(
                seat.role.clone(),
                "delta races for the role",
                date(2026, 7, 1)
            )
        })),
        store.capture(computed(NewFact {
            fields: role_fields("epsilon", now),
            ..NewFact::about(
                seat.role.clone(),
                "epsilon races for the role",
                date(2026, 7, 1)
            )
        })),
    );
    let a_won = matches!(a, Ok(Guarded::Written(_)));
    let b_won = matches!(b, Ok(Guarded::Written(_)));
    assert!(
        !(a_won && b_won),
        "two concurrent captures on one role must never both be granted: a={a:?} b={b:?}"
    );
    assert!(
        a_won || b_won,
        "two concurrent captures on one role must grant at least one of them: a={a:?} b={b:?}"
    );
    let loser = if a_won { &b } else { &a };
    assert!(
        matches!(
            loser,
            Err(MemoryError::RoleTaken { .. }) | Err(MemoryError::Conflict)
        ),
        "the side that did not win must be told RoleTaken or Conflict — the store's own \
         signal that a concurrent write landed on the same instant, asking the caller to \
         retry — never any other answer: {loser:?}",
    );
}

/// **A stale former holder's own write is a self-claim like any other, so
/// it faces the same check a rival's would** — the fix for the exact bug a
/// release once had: reading the holder, then writing unconditionally,
/// with nothing between the two re-checking who holds it NOW. Renew and
/// release both write `holder` and `claimed_at`
/// together, the same shape a claim writes, differing only in the
/// timestamp — so a former holder's own write, arriving after a rival has
/// legitimately taken over, is refused by [`crate::session::claim_role`]
/// exactly as a stranger's would be, never a blind overwrite.
pub async fn a_stale_holders_own_write_is_refused_once_a_rival_has_taken_over<M: Memory>(
    store: &M,
) {
    let seat = a_seat(
        store,
        "bot:contract-role-takeover-race",
        "role:contract-takeover-race",
    )
    .await;
    let claimed_at = crate::session::testing::contract::epoch();
    let stale = claimed_at + crate::session::LEASE_FRESHNESS + jiff::SignedDuration::from_secs(1);

    let claimed = capture(
        store,
        computed(NewFact {
            fields: role_fields("delta", claimed_at),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        }),
    )
    .await;

    // Delta's claim is now stale, so epsilon's claim is legitimate — the
    // same rival-takeover this contract already proves grants a fresh
    // claimant once the lease has gone cold.
    let taken_over = store
        .update_fact(
            &claimed.address(),
            computed_patch(FactPatch {
                fields: role_fields("epsilon", stale),
                ..FactPatch::default()
            }),
            &seat.bot,
        )
        .await;
    assert!(
        matches!(taken_over, Ok(Guarded::Written(_))),
        "epsilon's claim must be granted once delta's has gone stale, or this proves nothing \
         about a takeover: {taken_over:?}",
    );

    // Delta's own write — release (an expired `claimed_at`) or renewal (a
    // fresh one) both take this shape — must be refused now that epsilon
    // legitimately holds it, never accepted as if delta still did.
    for (label, delta_writes) in [
        ("release", jiff::Timestamp::UNIX_EPOCH),
        ("renewal", stale + jiff::SignedDuration::from_secs(1)),
    ] {
        let delta_write = store
            .update_fact(
                &claimed.address(),
                computed_patch(FactPatch {
                    fields: role_fields("delta", delta_writes),
                    ..FactPatch::default()
                }),
                &seat.bot,
            )
            .await;
        assert!(
            matches!(delta_write, Err(MemoryError::RoleTaken { ref holder, .. }) if holder == "epsilon"),
            "delta's {label}, arriving after epsilon's legitimate takeover, must be refused \
             naming epsilon — never silently accepted: {delta_write:?}",
        );
    }

    let fields = folded_fields_of(store, &seat.role).await;
    assert_eq!(
        fields.get(crate::session::ROLE_HOLDER),
        Some(&"epsilon".to_string()),
        "epsilon's legitimate claim must survive both of delta's refused writes: {fields:?}",
    );
}

/// **A renewal that arrives after the holder's release is refused, and the
/// role stays free.** This is the interleaving a write in flight makes with
/// its session's wrap: the renewal read the claim while the session still held
/// it, the release landed, and only then does the renewal write. Taken as a
/// claim it would lease the role to a session that is over; as a renewal it
/// applies only while its own sid holds the role.
///
/// **Both halves**, or a store that refused every renewal would pass: the same
/// renewal lands while its sid holds the role, and a fresh claimant takes the
/// role the instant after the release.
pub async fn a_renewal_after_a_release_is_refused_and_the_role_stays_free<M: Memory>(store: &M) {
    let seat = a_seat(
        store,
        "bot:contract-role-renew-after-release",
        "role:contract-renew-after-release",
    )
    .await;
    let t0 = crate::session::testing::contract::epoch() + jiff::SignedDuration::from_secs(60);
    let later = t0 + jiff::SignedDuration::from_secs(60);
    let move_of = |kind| {
        Some(crate::session::RoleMove {
            kind,
            role: seat.name.clone(),
            claimant: "delta".to_string(),
        })
    };
    let renew = |at| {
        computed_patch(FactPatch {
            fields: role_fields("delta", at),
            role_move: move_of(crate::session::RoleMoveKind::Renew),
            ..FactPatch::default()
        })
    };

    let claimed = capture(
        store,
        computed(NewFact {
            fields: role_fields("delta", t0),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        }),
    )
    .await;

    // The positive: while delta holds the role, its renewal lands.
    let renewed = store
        .update_fact(&claimed.address(), renew(later), &seat.bot)
        .await;
    assert!(
        matches!(renewed, Ok(Guarded::Written(_))),
        "a holder's own renewal must land, or this proves nothing: {renewed:?}"
    );

    // The release lands: the holder is cleared and the moment reads as stale.
    let released = store
        .update_fact(
            &claimed.address(),
            computed_patch(FactPatch {
                fields: [(
                    crate::session::ROLE_CLAIMED_AT.to_string(),
                    jiff::Timestamp::UNIX_EPOCH.to_string(),
                )]
                .into_iter()
                .collect(),
                clear_fields: vec![crate::session::ROLE_HOLDER.to_string()],
                role_move: move_of(crate::session::RoleMoveKind::Release),
                ..FactPatch::default()
            }),
            &seat.bot,
        )
        .await;
    assert!(
        matches!(released, Ok(Guarded::Written(_))),
        "the holder's own release must land: {released:?}"
    );
    let fields = folded_fields_of(store, &seat.role).await;
    assert_eq!(
        fields.get(crate::session::ROLE_HOLDER),
        None,
        "a release clears the holder: {fields:?}"
    );

    // The renewal that was in flight arrives now.
    let late = store
        .update_fact(
            &claimed.address(),
            renew(later + jiff::SignedDuration::from_secs(1)),
            &seat.bot,
        )
        .await;
    assert!(
        matches!(late, Err(MemoryError::RoleNotHeld { .. })),
        "a renewal after the release must be refused: {late:?}"
    );
    let fields = folded_fields_of(store, &seat.role).await;
    assert_eq!(
        fields.get(crate::session::ROLE_HOLDER),
        None,
        "a refused renewal leaves the role free: {fields:?}"
    );

    // And the role is free for a fresh claimant at once.
    let fresh = store
        .update_fact(
            &claimed.address(),
            computed_patch(FactPatch {
                fields: role_fields("epsilon", later + jiff::SignedDuration::from_secs(2)),
                ..FactPatch::default()
            }),
            &seat.bot,
        )
        .await;
    assert!(
        matches!(fresh, Ok(Guarded::Written(_))),
        "a fresh claimant takes the released role: {fresh:?}"
    );
}

/// **A claim in the old shape moves onto its role object, and the bot's keys for
/// that role are gone.** The positive comes first, in the same case: the object
/// holds the holder and the moment, or a store that dropped the keys and wrote
/// nothing would pass the absence. What stays: another role's keys on the same
/// bot, a key that names no role, and every record that is not the empty claim
/// record, whether it still holds another key or was written under other words.
/// The claim record the move leaves empty is archived, because an active record
/// with no role key reads as a rule the bot has to follow.
pub async fn a_claim_moved_onto_its_object_leaves_no_role_keys_on_the_bot<M: Memory>(store: &M) {
    let seat = a_seat(store, "bot:contract-role-moved", "role:contract-moved-role").await;
    let t0 = crate::session::testing::contract::epoch() + jiff::SignedDuration::from_secs(60);
    let soon = t0 + jiff::SignedDuration::from_secs(10);

    // The claim the previous build wrote, on the bot, with the words it wrote.
    let old = capture(
        store,
        computed(NewFact {
            fields: old_role_fields(&seat.name, "delta", t0),
            ..NewFact::about(
                seat.bot.clone(),
                "claimed the contract-moved-role role",
                date(2026, 7, 1),
            )
        }),
    )
    .await;
    // A second role of the same bot.
    let other = capture(
        store,
        computed(NewFact {
            fields: old_role_fields("contract-kept-role", "delta", t0),
            ..NewFact::about(
                seat.bot.clone(),
                "claimed the contract-kept-role role",
                date(2026, 7, 1),
            )
        }),
    )
    .await;
    // A record in the claim's own words that holds the keys beside a key that
    // names no role.
    let mut mixed_fields = old_role_fields(&seat.name, "delta", t0);
    mixed_fields.insert("agent".to_string(), "an-agent".to_string());
    let mixed = capture(
        store,
        computed(NewFact {
            fields: mixed_fields,
            ..NewFact::about(
                seat.bot.clone(),
                "claimed the contract-moved-role role",
                date(2026, 7, 1),
            )
        }),
    )
    .await;

    // A record that holds nothing but the keys, under words that are not the
    // claim's own, is somebody's note and is not archived for being emptied.
    let note = capture(
        store,
        computed(NewFact {
            fields: old_role_fields(&seat.name, "delta", t0),
            ..NewFact::about(seat.bot.clone(), "delta's own note", date(2026, 7, 1))
        }),
    )
    .await;

    // The holder's first write on the object moves the claim.
    let renewed = store
        .capture(computed(NewFact {
            fields: role_fields("delta", soon),
            ..NewFact::about(seat.role.clone(), "delta renews the role", date(2026, 7, 1))
        }))
        .await;
    assert!(
        matches!(renewed, Ok(Guarded::Written(_))),
        "the claim lands on the object, or this proves nothing: {renewed:?}",
    );
    let touched = crate::session::role_keys::drop_old_role_keys(store, &seat.bot, &seat.name)
        .await
        .expect("the old keys come off");
    assert_eq!(touched, 3, "the three records that held the role's keys");

    let on_object = folded_fields_of(store, &seat.role).await;
    assert_eq!(
        on_object.get(crate::session::ROLE_HOLDER),
        Some(&"delta".to_string()),
        "the object holds the holder: {on_object:?}",
    );
    assert_eq!(
        on_object
            .get(crate::session::ROLE_CLAIMED_AT)
            .map(String::as_str),
        Some(soon.to_string().as_str()),
        "and the moment: {on_object:?}",
    );
    let on_bot = folded_fields_of(store, &seat.bot).await;
    for key in [
        crate::session::role_keys::role_holder_key(&seat.name),
        crate::session::role_keys::role_claimed_at_key(&seat.name),
    ] {
        assert!(
            !on_bot.contains_key(&key),
            "the bot no longer holds {key}: {on_bot:?}"
        );
    }
    assert_eq!(
        on_bot
            .get(&crate::session::role_keys::role_holder_key(
                "contract-kept-role"
            ))
            .map(String::as_str),
        Some("delta"),
        "another role's keys stay: {on_bot:?}",
    );
    assert_eq!(
        on_bot.get("agent").map(String::as_str),
        Some("an-agent"),
        "a key that names no role stays: {on_bot:?}",
    );

    let status_of = |facts: &[Fact], address: &FactAddress| {
        facts
            .iter()
            .find(|f| &f.address() == address)
            .unwrap_or_else(|| panic!("{address} is still readable"))
            .status
    };
    let facts = store.recall(&seat.bot).await.expect("the bot's records");
    assert_eq!(
        status_of(&facts, &old.address()),
        FactStatus::Archived,
        "the claim record the move left empty is archived"
    );
    assert_eq!(
        status_of(&facts, &mixed.address()),
        FactStatus::Active,
        "a record that holds anything else stays"
    );
    assert_eq!(
        status_of(&facts, &other.address()),
        FactStatus::Active,
        "another role's record stays"
    );
    assert_eq!(
        status_of(&facts, &note.address()),
        FactStatus::Active,
        "a record under words that are not the claim's own stays"
    );

    // And nothing is left to do: the second pass writes nothing.
    let again = crate::session::role_keys::drop_old_role_keys(store, &seat.bot, &seat.name)
        .await
        .expect("a second pass reads");
    assert_eq!(again, 0, "a role with no old keys changes nothing");
}

/// The pass over every bot, held to finishing every role.
async fn run_the_pass<M: Memory>(store: &M) -> crate::session::role_keys::OldRoleKeysPass {
    let pass = crate::session::role_keys::move_old_role_claims(store, date(2026, 7, 2))
        .await
        .expect("the pass reads every bot");
    assert!(pass.failed.is_empty(), "{pass:?}");
    pass
}

/// **A role claimed in the old shape is carried onto its object by the start's
/// pass, and stays held.** The bot's own `role/<role>/holder` and
/// `role/<role>/claimed_at` name no lease by themselves; the pass writes the
/// holder and the moment onto a new role object, a child of the bot, and takes
/// the keys off. Both halves: a rival is refused naming the old holder, and the
/// old holder's own write lands. A store that dropped the keys and carried
/// nothing would seat the rival.
pub async fn a_pass_carries_a_role_claimed_in_the_old_shape_onto_its_object<M: Memory>(store: &M) {
    let bot = EntityId("bot:contract-role-carried".to_string());
    ensure(store, &bot).await;
    let role = EntityId("role:contract-carried".to_string());
    let t0 = crate::session::testing::contract::epoch() + jiff::SignedDuration::from_secs(60);
    let soon = t0 + jiff::SignedDuration::from_secs(10);
    capture(
        store,
        computed(NewFact {
            fields: old_role_fields("contract-carried", "delta", t0),
            ..NewFact::about(
                bot.clone(),
                "claimed the contract-carried role",
                date(2026, 7, 1),
            )
        }),
    )
    .await;
    assert!(
        matches!(
            store.fields(&role).await,
            Err(MemoryError::UnknownEntity { .. })
        ),
        "the role is not an object before the pass, or this proves nothing",
    );

    let pass = run_the_pass(store).await;
    assert!(pass.moved >= 1, "the claim was carried: {pass:?}");

    let on_object = folded_fields_of(store, &role).await;
    assert_eq!(
        on_object.get(crate::session::ROLE_HOLDER),
        Some(&"delta".to_string()),
        "the object holds the holder: {on_object:?}",
    );
    assert_eq!(
        on_object
            .get(crate::session::ROLE_CLAIMED_AT)
            .map(String::as_str),
        Some(t0.to_string().as_str()),
        "and the moment the old keys held: {on_object:?}",
    );
    let object = store
        .list_entities(Some(EntityKind::ROLE))
        .await
        .expect("the roles list")
        .into_iter()
        .find(|e| e.id == role)
        .expect("the role object exists");
    assert_eq!(object.parent.as_ref(), Some(&bot), "a child of the bot");
    let on_bot = folded_fields_of(store, &bot).await;
    assert!(
        !on_bot.contains_key(&crate::session::role_keys::role_holder_key(
            "contract-carried"
        )) && !on_bot.contains_key(&crate::session::role_keys::role_claimed_at_key(
            "contract-carried"
        )),
        "the bot no longer holds the old keys: {on_bot:?}",
    );

    let rival = store
        .capture(computed(NewFact {
            fields: role_fields("epsilon", soon),
            ..NewFact::about(role.clone(), "epsilon claims the role", date(2026, 7, 2))
        }))
        .await;
    assert!(
        matches!(
            &rival,
            Err(MemoryError::RoleTaken { role, holder, .. })
                if role == "contract-carried" && holder == "delta"
        ),
        "a carried claim must refuse a rival, naming its holder: {rival:?}",
    );
    let renewed = store
        .capture(computed(NewFact {
            fields: role_fields("delta", soon),
            ..NewFact::about(role.clone(), "delta renews the role", date(2026, 7, 2))
        }))
        .await;
    assert!(
        matches!(renewed, Ok(Guarded::Written(_))),
        "the old holder's own write must land, or this proves nothing: {renewed:?}",
    );

    let again = run_the_pass(store).await;
    assert_eq!(
        (again.moved, again.cleared),
        (0, 0),
        "a second pass finds nothing to write: {again:?}",
    );
}

/// **The pass carries a claim onto a role object that already exists with no
/// moment, where it is.** One role is one object: the claim lands on the object a
/// bot already has under the name rather than making a second.
pub async fn a_pass_writes_the_claim_onto_a_role_object_that_has_no_moment<M: Memory>(store: &M) {
    let seat = a_seat(store, "bot:contract-role-bare", "role:contract-bare").await;
    let t0 = crate::session::testing::contract::epoch() + jiff::SignedDuration::from_secs(60);
    capture(
        store,
        computed(NewFact {
            fields: old_role_fields(&seat.name, "delta", t0),
            ..NewFact::about(
                seat.bot.clone(),
                "claimed the contract-bare role",
                date(2026, 7, 1),
            )
        }),
    )
    .await;

    let pass = run_the_pass(store).await;
    assert!(pass.moved >= 1, "{pass:?}");

    let on_object = folded_fields_of(store, &seat.role).await;
    assert_eq!(
        on_object.get(crate::session::ROLE_HOLDER),
        Some(&"delta".to_string()),
        "the existing object holds the claim: {on_object:?}",
    );
    let roles = store
        .list_entities(Some(EntityKind::ROLE))
        .await
        .expect("the roles list");
    assert_eq!(
        roles.iter().filter(|e| e.id == seat.role).count(),
        1,
        "one role is one object",
    );
}

/// **A role that has moved is left as it is, and its stale keys come off.** The
/// object's holder and moment are the lease; the keys on the bot name an older
/// holder and must not overwrite them. The positive is the object keeping its
/// newer holder, which is what a pass that carried every old claim would break.
pub async fn a_pass_leaves_a_moved_role_as_it_is_and_takes_its_stale_keys_off<M: Memory>(
    store: &M,
) {
    let seat = a_seat(store, "bot:contract-role-stale", "role:contract-stale").await;
    let t0 = crate::session::testing::contract::epoch() + jiff::SignedDuration::from_secs(60);
    let later = t0 + jiff::SignedDuration::from_secs(600);
    capture(
        store,
        computed(NewFact {
            fields: role_fields("epsilon", later),
            ..NewFact::about(
                seat.role.clone(),
                "epsilon claims the role",
                date(2026, 7, 1),
            )
        }),
    )
    .await;
    capture(
        store,
        computed(NewFact {
            fields: old_role_fields(&seat.name, "delta", t0),
            ..NewFact::about(
                seat.bot.clone(),
                "claimed the contract-stale role",
                date(2026, 7, 1),
            )
        }),
    )
    .await;

    let pass = run_the_pass(store).await;
    assert!(pass.cleared >= 1, "the stale record was cleared: {pass:?}");

    let on_object = folded_fields_of(store, &seat.role).await;
    assert_eq!(
        on_object.get(crate::session::ROLE_HOLDER),
        Some(&"epsilon".to_string()),
        "the object keeps its holder: {on_object:?}",
    );
    assert_eq!(
        on_object
            .get(crate::session::ROLE_CLAIMED_AT)
            .map(String::as_str),
        Some(later.to_string().as_str()),
        "and its moment: {on_object:?}",
    );
    let on_bot = folded_fields_of(store, &seat.bot).await;
    assert!(
        !on_bot.contains_key(&crate::session::role_keys::role_holder_key(&seat.name)),
        "the stale keys are gone: {on_bot:?}",
    );
}

/// **A half-written old claim carries nothing.** Keys with no holder, or a
/// moment that does not read, say nothing about a lease, so the pass takes them
/// off and writes no claim: the role stays free. The pair is the claim the pass
/// does carry, in the first case above.
pub async fn a_pass_carries_nothing_for_a_half_written_old_claim<M: Memory>(store: &M) {
    let seat = a_seat(store, "bot:contract-role-half", "role:contract-half").await;
    let t0 = crate::session::testing::contract::epoch() + jiff::SignedDuration::from_secs(60);
    capture(
        store,
        computed(NewFact {
            fields: [(
                crate::session::role_keys::role_holder_key(&seat.name),
                "delta".to_string(),
            )]
            .into_iter()
            .collect(),
            ..NewFact::about(
                seat.bot.clone(),
                "claimed the contract-half role",
                date(2026, 7, 1),
            )
        }),
    )
    .await;
    let mut unreadable = old_role_fields("contract-half-moment", "delta", t0);
    unreadable.insert(
        crate::session::role_keys::role_claimed_at_key("contract-half-moment"),
        "some time ago".to_string(),
    );
    capture(
        store,
        computed(NewFact {
            fields: unreadable,
            ..NewFact::about(
                seat.bot.clone(),
                "claimed the contract-half-moment role",
                date(2026, 7, 1),
            )
        }),
    )
    .await;

    run_the_pass(store).await;

    let on_object = folded_fields_of(store, &seat.role).await;
    assert!(
        !on_object.contains_key(crate::session::ROLE_HOLDER),
        "no holder was carried: {on_object:?}",
    );
    let on_bot = folded_fields_of(store, &seat.bot).await;
    assert!(
        !on_bot
            .keys()
            .any(|key| key.starts_with("role/contract-half")),
        "the half-written keys are gone: {on_bot:?}",
    );
    let taken = store
        .capture(computed(NewFact {
            fields: role_fields("epsilon", t0),
            ..NewFact::about(
                seat.role.clone(),
                "epsilon claims the role",
                date(2026, 7, 2),
            )
        }))
        .await;
    assert!(
        matches!(taken, Ok(Guarded::Written(_))),
        "a role with no carried claim is free: {taken:?}",
    );
    assert!(
        !store
            .list_entities(Some(EntityKind::ROLE))
            .await
            .expect("the roles list")
            .iter()
            .any(|e| e.id.as_str() == "role:contract-half-moment"),
        "a claim whose moment does not read makes no role object",
    );
}

/// **A released role object is not read past to the bot's old keys.** A release
/// clears the holder and keeps the moment, so the old keys, which still name the
/// holder the release ended, must not answer for a role object that carries a
/// moment. The positive is the rival that takes the released role at once.
pub async fn a_released_role_object_is_not_read_past_to_the_old_keys<M: Memory>(store: &M) {
    let seat = a_seat(
        store,
        "bot:contract-role-released-over-old",
        "role:contract-released-over-old",
    )
    .await;
    let t0 = crate::session::testing::contract::epoch() + jiff::SignedDuration::from_secs(60);
    let soon = t0 + jiff::SignedDuration::from_secs(10);

    capture(
        store,
        computed(NewFact {
            fields: old_role_fields(&seat.name, "delta", t0),
            ..NewFact::about(seat.bot.clone(), "delta claimed the role", date(2026, 7, 1))
        }),
    )
    .await;
    // Delta moves its claim onto the role object, then gives the role up.
    let claimed = capture(
        store,
        computed(NewFact {
            fields: role_fields("delta", t0),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        }),
    )
    .await;
    let released = store
        .update_fact(
            &claimed.address(),
            computed_patch(FactPatch {
                fields: [(
                    crate::session::ROLE_CLAIMED_AT.to_string(),
                    jiff::Timestamp::UNIX_EPOCH.to_string(),
                )]
                .into_iter()
                .collect(),
                clear_fields: vec![crate::session::ROLE_HOLDER.to_string()],
                role_move: Some(crate::session::RoleMove {
                    kind: crate::session::RoleMoveKind::Release,
                    role: seat.name.clone(),
                    claimant: "delta".to_string(),
                }),
                ..FactPatch::default()
            }),
            &seat.bot,
        )
        .await;
    assert!(
        matches!(released, Ok(Guarded::Written(_))),
        "the holder's own release must land: {released:?}",
    );

    let taken = store
        .capture(computed(NewFact {
            fields: role_fields("epsilon", soon),
            ..NewFact::about(
                seat.role.clone(),
                "epsilon claims the role",
                date(2026, 7, 1),
            )
        }))
        .await;
    assert!(
        matches!(taken, Ok(Guarded::Written(_))),
        "a released role is free although the bot's old keys still name its old holder: \
         {taken:?}",
    );
}

/// **Anything else written to a role object is not a lease question.** The agent
/// key and a watcher's marks land while another session's lease is fresh, and
/// they move neither the holder nor the moment.
pub async fn a_role_objects_other_keys_land_without_moving_the_lease<M: Memory>(store: &M) {
    let seat = a_seat(
        store,
        "bot:contract-role-other-keys",
        "role:contract-other-keys",
    )
    .await;
    let now = crate::session::testing::contract::epoch();
    capture(
        store,
        computed(NewFact {
            fields: role_fields("delta", now),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        }),
    )
    .await;

    let wrote = store
        .capture(computed(NewFact {
            fields: [("agent".to_string(), "an-agent-id".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                seat.role.clone(),
                "the line records its agent",
                date(2026, 7, 1),
            )
        }))
        .await;
    assert!(
        matches!(wrote, Ok(Guarded::Written(_))),
        "a write that names no holder is not a claim and must land: {wrote:?}",
    );
    let fields = folded_fields_of(store, &seat.role).await;
    assert_eq!(
        fields.get("agent"),
        Some(&"an-agent-id".to_string()),
        "{fields:?}"
    );
    assert_eq!(
        fields.get(crate::session::ROLE_HOLDER),
        Some(&"delta".to_string()),
        "the agent key did not move the holder: {fields:?}",
    );
}

/// **Renewals overwrite the lease moment; they do not add to the claim's
/// history.** A renewal is machine state: the holder is alive, and the one
/// thing that changes is when. Twenty renewals leave the moment with the one
/// write the claim made, carrying the newest moment, and the claim with the one
/// write it started with.
///
/// **Paired with the positive**, or a store that ignored renewals would pass:
/// the renewed moment is what a rival is refused against, so a claim made after
/// the ORIGINAL moment would have lapsed is still refused, and one made after
/// the renewed moment lapses is granted.
pub async fn many_renewals_leave_the_lease_moment_at_one_write<M: Memory>(store: &M) {
    let seat = a_seat(
        store,
        "bot:contract-role-many-renewals",
        "role:contract-many-renewals",
    )
    .await;
    let t0 = crate::session::testing::contract::epoch() + jiff::SignedDuration::from_secs(60);
    let claimed = capture(
        store,
        computed(NewFact {
            fields: role_fields("delta", t0),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        }),
    )
    .await;

    let mut last = t0;
    for n in 1..=20 {
        last = t0 + jiff::SignedDuration::from_mins(5 * n);
        let renewed = store
            .update_fact(
                &claimed.address(),
                computed_patch(FactPatch {
                    fields: role_fields("delta", last),
                    role_move: Some(crate::session::RoleMove {
                        kind: crate::session::RoleMoveKind::Renew,
                        role: seat.name.clone(),
                        claimant: "delta".to_string(),
                    }),
                    ..FactPatch::default()
                }),
                &seat.bot,
            )
            .await;
        assert!(
            matches!(renewed, Ok(Guarded::Written(_))),
            "renewal {n} must land: {renewed:?}"
        );
    }

    let moment = store
        .history(&seat.role, crate::session::ROLE_CLAIMED_AT)
        .await
        .expect("history is readable");
    assert_eq!(
        moment.len(),
        1,
        "twenty renewals must leave the lease moment at the claim's one write: {moment:?}"
    );
    let holder = store
        .history(&seat.role, crate::session::ROLE_HOLDER)
        .await
        .expect("history is readable");
    assert_eq!(holder.len(), 1, "a renewal rewrites no holder: {holder:?}");
    let writes = store
        .claim_history(&claimed.address())
        .await
        .expect("the claim's writes are readable");
    assert_eq!(
        writes.len(),
        1,
        "a renewal adds no write to the claim: {writes:?}"
    );

    let fields = folded_fields_of(store, &seat.role).await;
    assert_eq!(
        fields.get(crate::session::ROLE_CLAIMED_AT),
        Some(&last.to_string()),
        "the one write carries the newest moment: {fields:?}"
    );
    assert_eq!(
        fields.get(crate::session::ROLE_HOLDER),
        Some(&"delta".to_string()),
        "{fields:?}"
    );

    // The renewed moment is the lease: a rival one second past the ORIGINAL
    // lease is refused, and one second past the renewed lease is granted.
    let rival = |at| {
        computed_patch(FactPatch {
            fields: role_fields("epsilon", at),
            ..FactPatch::default()
        })
    };
    let inside = t0 + crate::session::LEASE_FRESHNESS + jiff::SignedDuration::from_secs(1);
    let refused = store
        .update_fact(&claimed.address(), rival(inside), &seat.bot)
        .await;
    assert!(
        matches!(refused, Err(MemoryError::RoleTaken { ref holder, .. }) if holder == "delta"),
        "the lease the renewals kept alive refuses a rival: {refused:?}"
    );
    let past = last + crate::session::LEASE_FRESHNESS + jiff::SignedDuration::from_secs(1);
    let taken = store
        .update_fact(&claimed.address(), rival(past), &seat.bot)
        .await;
    assert!(
        matches!(taken, Ok(Guarded::Written(_))),
        "a lease that lapsed without a renewal is takeable: {taken:?}"
    );
}

/// **A renewal restamps the moment it overwrites.** The renewal replaces the
/// value of the lease moment in place, and the age of that key is the age of the
/// write holding it, so it has to read as the renewal and not as the claim. A
/// store that replaced the value and kept the stamp would report a lease moment
/// older than the renewal that wrote it, and a reader asking how long a thing has
/// gone untouched would count from the claim. The holder is not rewritten by a
/// renewal and keeps the age it had.
pub async fn a_renewal_restamps_the_lease_moment_it_overwrites<M: Memory>(store: &M) {
    let seat = a_seat(store, "bot:contract-role-restamp", "role:contract-restamp").await;
    let t0 = crate::session::testing::contract::epoch() + jiff::SignedDuration::from_secs(60);
    let claimed = capture(
        store,
        computed(NewFact {
            fields: role_fields("delta", t0),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        }),
    )
    .await;
    let before = store
        .field_ages(&seat.role)
        .await
        .expect("the role object has ages");
    let moment_then = *before
        .keys
        .get(crate::session::ROLE_CLAIMED_AT)
        .expect("the lease moment was stamped");
    let holder_then = *before
        .keys
        .get(crate::session::ROLE_HOLDER)
        .expect("the holder was stamped");

    // The stores stamp from their own clocks, so let one tick pass between the
    // claim and its renewal.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let renewed = store
        .update_fact(
            &claimed.address(),
            computed_patch(FactPatch {
                fields: role_fields("delta", t0 + jiff::SignedDuration::from_mins(5)),
                role_move: Some(crate::session::RoleMove {
                    kind: crate::session::RoleMoveKind::Renew,
                    role: seat.name.clone(),
                    claimant: "delta".to_string(),
                }),
                ..FactPatch::default()
            }),
            &seat.bot,
        )
        .await;
    assert!(
        matches!(renewed, Ok(Guarded::Written(_))),
        "the renewal must land: {renewed:?}"
    );
    let after = store
        .field_ages(&seat.role)
        .await
        .expect("the role object has ages");
    assert!(
        after.keys[crate::session::ROLE_CLAIMED_AT] > moment_then,
        "the overwritten moment kept the stamp of the claim it replaced: {before:?} then {after:?}"
    );
    assert_eq!(
        after.keys[crate::session::ROLE_HOLDER],
        holder_then,
        "a renewal rewrites no holder, so its age does not move: {before:?} then {after:?}"
    );
}

/// **Only a renewal's moment is overwritten.** Every other key on a role, and
/// the claim and the release, still append: a second key written twice has two
/// writes, and a claim, a release and a second claim leave the moment with three.
/// Without this, a store that overwrote every key on a role object would pass the
/// case above.
pub async fn only_a_renewals_moment_is_overwritten<M: Memory>(store: &M) {
    let seat = a_seat(
        store,
        "bot:contract-role-only-renewals",
        "role:contract-only-renewals",
    )
    .await;
    let t0 = crate::session::testing::contract::epoch() + jiff::SignedDuration::from_secs(60);
    let claimed = capture(
        store,
        computed(NewFact {
            fields: role_fields("delta", t0),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        }),
    )
    .await;

    for agent in ["first-agent", "second-agent"] {
        let wrote = store
            .capture(computed(NewFact {
                fields: [("agent".to_string(), agent.to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    seat.role.clone(),
                    "the line records its agent",
                    date(2026, 7, 1),
                )
            }))
            .await;
        assert!(matches!(wrote, Ok(Guarded::Written(_))), "{wrote:?}");
    }
    let agent = store.history(&seat.role, "agent").await.expect("history");
    assert_eq!(
        agent.len(),
        2,
        "a key that is not the lease moment appends: {agent:?}"
    );

    let move_of = |kind| {
        Some(crate::session::RoleMove {
            kind,
            role: seat.name.clone(),
            claimant: "delta".to_string(),
        })
    };
    let released = store
        .update_fact(
            &claimed.address(),
            computed_patch(FactPatch {
                fields: [(
                    crate::session::ROLE_CLAIMED_AT.to_string(),
                    jiff::Timestamp::UNIX_EPOCH.to_string(),
                )]
                .into_iter()
                .collect(),
                clear_fields: vec![crate::session::ROLE_HOLDER.to_string()],
                role_move: move_of(crate::session::RoleMoveKind::Release),
                ..FactPatch::default()
            }),
            &seat.bot,
        )
        .await;
    assert!(matches!(released, Ok(Guarded::Written(_))), "{released:?}");
    let taken = store
        .update_fact(
            &claimed.address(),
            computed_patch(FactPatch {
                fields: role_fields("epsilon", t0 + jiff::SignedDuration::from_mins(1)),
                ..FactPatch::default()
            }),
            &seat.bot,
        )
        .await;
    assert!(matches!(taken, Ok(Guarded::Written(_))), "{taken:?}");

    let moment = store
        .history(&seat.role, crate::session::ROLE_CLAIMED_AT)
        .await
        .expect("history is readable");
    assert_eq!(
        moment.len(),
        3,
        "a claim, a release and a second claim each append: {moment:?}"
    );
}

/// **A renewal that carries more than the lease is applied whole.** The renewal
/// that overwrites the lease moment and appends nothing is the plain one: the
/// holder, the moment, and the move. A patch that also rewrites the claim's
/// words, or sets another key, is an ordinary edit with a renewal inside it, and
/// answering `Written` after dropping those members would tell the caller its
/// edit landed when it did not. Each member is read back.
///
/// **Paired with the plain renewal**, or a store that took every renewal the
/// ordinary way would pass: after the two edits the lease moment holds the
/// edits' writes, and the plain renewal that follows still adds none.
pub async fn a_renewal_that_carries_more_than_the_lease_is_applied_whole<M: Memory>(store: &M) {
    let seat = a_seat(
        store,
        "bot:contract-role-renew-whole",
        "role:contract-renew-whole",
    )
    .await;
    let t0 = crate::session::testing::contract::epoch() + jiff::SignedDuration::from_secs(60);
    let claimed = capture(
        store,
        computed(NewFact {
            fields: role_fields("delta", t0),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        }),
    )
    .await;
    let renewing = |at, extra: FactPatch| {
        computed_patch(FactPatch {
            fields: role_fields("delta", at)
                .into_iter()
                .chain(extra.fields.clone())
                .collect(),
            role_move: Some(crate::session::RoleMove {
                kind: crate::session::RoleMoveKind::Renew,
                role: seat.name.clone(),
                claimant: "delta".to_string(),
            }),
            ..extra
        })
    };
    let plain = FactPatch::default();

    // The claim's own words ride in the renewal.
    let at = t0 + jiff::SignedDuration::from_mins(5);
    let worded = store
        .update_fact(
            &claimed.address(),
            renewing(
                at,
                FactPatch {
                    details: Some("renewed with a note".to_string()),
                    ..plain.clone()
                },
            ),
            &seat.bot,
        )
        .await;
    assert!(
        matches!(worded, Ok(Guarded::Written(_))),
        "a renewal with a note must land: {worded:?}"
    );
    let recalled = store.recall(&seat.role).await.expect("recall");
    let record = recalled
        .iter()
        .find(|f| f.id == claimed.id)
        .expect("the claim is still there");
    assert_eq!(
        record.details.as_deref(),
        Some("renewed with a note"),
        "the note the renewal carried was dropped: {record:?}"
    );

    // Another key rides in the renewal.
    let at = t0 + jiff::SignedDuration::from_mins(10);
    let keyed = store
        .update_fact(
            &claimed.address(),
            renewing(
                at,
                computed_patch(FactPatch {
                    fields: [("agent".to_string(), "an-agent-id".to_string())]
                        .into_iter()
                        .collect(),
                    ..plain.clone()
                }),
            ),
            &seat.bot,
        )
        .await;
    assert!(
        matches!(keyed, Ok(Guarded::Written(_))),
        "a renewal with another key must land: {keyed:?}"
    );
    let fields = folded_fields_of(store, &seat.role).await;
    assert_eq!(
        fields.get("agent"),
        Some(&"an-agent-id".to_string()),
        "the key the renewal carried was dropped: {fields:?}"
    );
    assert_eq!(
        fields.get(crate::session::ROLE_CLAIMED_AT),
        Some(&at.to_string()),
        "and the lease moved with it: {fields:?}"
    );

    // The plain renewal still adds no write to the claim.
    let before = store
        .claim_history(&claimed.address())
        .await
        .expect("the claim's writes are readable")
        .len();
    let renewed = store
        .update_fact(
            &claimed.address(),
            renewing(t0 + jiff::SignedDuration::from_mins(15), plain),
            &seat.bot,
        )
        .await;
    assert!(
        matches!(renewed, Ok(Guarded::Written(_))),
        "the plain renewal must land: {renewed:?}"
    );
    let after = store
        .claim_history(&claimed.address())
        .await
        .expect("the claim's writes are readable")
        .len();
    assert_eq!(
        before, after,
        "a plain renewal adds no write to the claim: {before} then {after}"
    );
}

/// **A renewal is a write on the thing, and a reader asking when the thing last
/// changed hears it.** The renewal overwrites the lease moment and adds no
/// claim write, so the only mark it leaves beyond the moment itself is the
/// signal that the thing was written. A store that filed that signal where no
/// reader of the thing looks would still answer the moment correctly and report
/// the thing as last written when the moment was stamped, a beat before the
/// renewal finished.
pub async fn a_renewal_is_a_write_on_the_thing<M: Memory>(store: &M) {
    let seat = a_seat(store, "bot:contract-role-signal", "role:contract-signal").await;
    let t0 = crate::session::testing::contract::epoch() + jiff::SignedDuration::from_secs(60);
    let claimed = capture(
        store,
        computed(NewFact {
            fields: role_fields("delta", t0),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        }),
    )
    .await;
    // The stores stamp from their own clocks, so let one tick pass between the
    // claim and its renewal.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let renewed = store
        .update_fact(
            &claimed.address(),
            computed_patch(FactPatch {
                fields: role_fields("delta", t0 + jiff::SignedDuration::from_mins(5)),
                role_move: Some(crate::session::RoleMove {
                    kind: crate::session::RoleMoveKind::Renew,
                    role: seat.name.clone(),
                    claimant: "delta".to_string(),
                }),
                ..FactPatch::default()
            }),
            &seat.bot,
        )
        .await;
    assert!(
        matches!(renewed, Ok(Guarded::Written(_))),
        "the renewal must land: {renewed:?}"
    );
    let ages = store
        .field_ages(&seat.role)
        .await
        .expect("the role object has ages");
    let moment = *ages
        .keys
        .get(crate::session::ROLE_CLAIMED_AT)
        .expect("the lease moment was stamped");
    let newest = ages.newest.expect("a written thing has a newest write");
    assert!(
        newest > moment,
        "the renewal left no write on the thing after the moment it stamped: newest {newest}, \
         lease moment {moment}"
    );
}

pub async fn run_all_role_claims<M: Memory>(store: &M) {
    many_renewals_leave_the_lease_moment_at_one_write(store).await;
    a_renewal_restamps_the_lease_moment_it_overwrites(store).await;
    a_renewal_is_a_write_on_the_thing(store).await;
    only_a_renewals_moment_is_overwritten(store).await;
    a_pass_carries_a_role_claimed_in_the_old_shape_onto_its_object(store).await;
    a_pass_writes_the_claim_onto_a_role_object_that_has_no_moment(store).await;
    a_pass_leaves_a_moved_role_as_it_is_and_takes_its_stale_keys_off(store).await;
    a_pass_carries_nothing_for_a_half_written_old_claim(store).await;
    a_claim_moved_onto_its_object_leaves_no_role_keys_on_the_bot(store).await;
    a_released_role_object_is_not_read_past_to_the_old_keys(store).await;
    a_role_objects_other_keys_land_without_moving_the_lease(store).await;
    a_second_capture_racing_the_first_is_refused(store).await;
    a_rival_update_fact_claim_is_refused_while_the_lease_is_fresh(store).await;
    two_concurrent_captures_race_and_exactly_one_wins(store).await;
    a_stale_holders_own_write_is_refused_once_a_rival_has_taken_over(store).await;
    a_renewal_after_a_release_is_refused_and_the_role_stays_free(store).await;
    a_renewal_that_carries_more_than_the_lease_is_applied_whole(store).await;
}
