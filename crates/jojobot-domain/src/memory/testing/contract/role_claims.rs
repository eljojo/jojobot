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
//! **Both shapes are read.** A role claimed before the role became an object
//! lives in the bot's own `role/<role>/holder` and `role/<role>/claimed_at`,
//! and it stays held until the role object carries a moment of its own.

use super::support::{add, capture, ensure};
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
        (crate::session::role_holder_key(role), claimant.to_string()),
        (crate::session::role_claimed_at_key(role), at.to_string()),
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
        NewFact {
            fields: role_fields("delta", now),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        },
    )
    .await;

    let refused = store
        .capture(NewFact {
            fields: role_fields("epsilon", now),
            ..NewFact::about(
                seat.role.clone(),
                "epsilon claims the role too",
                date(2026, 7, 1),
            )
        })
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
        NewFact {
            fields: role_fields("delta", now),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        },
    )
    .await;

    let refused = store
        .update_fact(
            &claimed.address(),
            FactPatch {
                fields: role_fields("epsilon", now),
                ..FactPatch::default()
            },
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
        store.capture(NewFact {
            fields: role_fields("delta", now),
            ..NewFact::about(
                seat.role.clone(),
                "delta races for the role",
                date(2026, 7, 1)
            )
        }),
        store.capture(NewFact {
            fields: role_fields("epsilon", now),
            ..NewFact::about(
                seat.role.clone(),
                "epsilon races for the role",
                date(2026, 7, 1)
            )
        }),
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
        NewFact {
            fields: role_fields("delta", claimed_at),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        },
    )
    .await;

    // Delta's claim is now stale, so epsilon's claim is legitimate — the
    // same rival-takeover this contract already proves grants a fresh
    // claimant once the lease has gone cold.
    let taken_over = store
        .update_fact(
            &claimed.address(),
            FactPatch {
                fields: role_fields("epsilon", stale),
                ..FactPatch::default()
            },
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
                FactPatch {
                    fields: role_fields("delta", delta_writes),
                    ..FactPatch::default()
                },
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
    let renew = |at| FactPatch {
        fields: role_fields("delta", at),
        role_move: move_of(crate::session::RoleMoveKind::Renew),
        ..FactPatch::default()
    };

    let claimed = capture(
        store,
        NewFact {
            fields: role_fields("delta", t0),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        },
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
            FactPatch {
                fields: [(
                    crate::session::ROLE_CLAIMED_AT.to_string(),
                    jiff::Timestamp::UNIX_EPOCH.to_string(),
                )]
                .into_iter()
                .collect(),
                clear_fields: vec![crate::session::ROLE_HOLDER.to_string()],
                role_move: move_of(crate::session::RoleMoveKind::Release),
                ..FactPatch::default()
            },
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
            FactPatch {
                fields: role_fields("epsilon", later + jiff::SignedDuration::from_secs(2)),
                ..FactPatch::default()
            },
            &seat.bot,
        )
        .await;
    assert!(
        matches!(fresh, Ok(Guarded::Written(_))),
        "a fresh claimant takes the released role: {fresh:?}"
    );
}

/// **A role claimed before it was an object is still held.** The bot's own
/// `role/<role>/holder` and `role/<role>/claimed_at` answer until the role
/// object carries a moment, so a rival is refused naming the old holder, and that
/// holder's first write on the new object is a renewal, never a refusal. A
/// reader that understood only the new shape would see this role as empty, and a
/// watcher that sees an empty role starts a second session for it.
///
/// **Both halves**, or a store that read no old keys would pass the refusal as a
/// store that refused everything: the old holder's own write lands, and the rival
/// stays refused after it.
pub async fn a_role_claimed_in_the_old_shape_is_still_held<M: Memory>(store: &M) {
    let seat = a_seat(
        store,
        "bot:contract-role-old-shape",
        "role:contract-old-shape",
    )
    .await;
    let t0 = crate::session::testing::contract::epoch() + jiff::SignedDuration::from_secs(60);
    let soon = t0 + jiff::SignedDuration::from_secs(10);

    // The claim the previous build wrote, on the bot.
    capture(
        store,
        NewFact {
            fields: old_role_fields(&seat.name, "delta", t0),
            ..NewFact::about(seat.bot.clone(), "delta claimed the role", date(2026, 7, 1))
        },
    )
    .await;

    let rival = store
        .capture(NewFact {
            fields: role_fields("epsilon", soon),
            ..NewFact::about(
                seat.role.clone(),
                "epsilon claims the role",
                date(2026, 7, 1),
            )
        })
        .await;
    assert!(
        matches!(
            &rival,
            Err(MemoryError::RoleTaken { role, holder, .. })
                if *role == seat.name && holder == "delta"
        ),
        "a role claimed in the old shape must refuse a rival, naming its holder: {rival:?}",
    );

    // The old holder's first write on the role object: its own renewal.
    let renewed = store
        .capture(NewFact {
            fields: role_fields("delta", soon),
            ..NewFact::about(seat.role.clone(), "delta renews the role", date(2026, 7, 1))
        })
        .await;
    assert!(
        matches!(renewed, Ok(Guarded::Written(_))),
        "the old holder's own write must land, or this proves nothing: {renewed:?}",
    );
    let fields = folded_fields_of(store, &seat.role).await;
    assert_eq!(
        fields.get(crate::session::ROLE_HOLDER),
        Some(&"delta".to_string()),
        "the role object now carries the claim: {fields:?}",
    );

    let after = store
        .capture(NewFact {
            fields: role_fields("epsilon", soon + jiff::SignedDuration::from_secs(1)),
            ..NewFact::about(seat.role.clone(), "epsilon tries again", date(2026, 7, 1))
        })
        .await;
    assert!(
        matches!(&after, Err(MemoryError::RoleTaken { holder, .. }) if holder == "delta"),
        "the rival is still refused after the renewal: {after:?}",
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
        NewFact {
            fields: old_role_fields(&seat.name, "delta", t0),
            ..NewFact::about(seat.bot.clone(), "delta claimed the role", date(2026, 7, 1))
        },
    )
    .await;
    // Delta moves its claim onto the role object, then gives the role up.
    let claimed = capture(
        store,
        NewFact {
            fields: role_fields("delta", t0),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        },
    )
    .await;
    let released = store
        .update_fact(
            &claimed.address(),
            FactPatch {
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
            },
            &seat.bot,
        )
        .await;
    assert!(
        matches!(released, Ok(Guarded::Written(_))),
        "the holder's own release must land: {released:?}",
    );

    let taken = store
        .capture(NewFact {
            fields: role_fields("epsilon", soon),
            ..NewFact::about(
                seat.role.clone(),
                "epsilon claims the role",
                date(2026, 7, 1),
            )
        })
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
        NewFact {
            fields: role_fields("delta", now),
            ..NewFact::about(seat.role.clone(), "delta claims the role", date(2026, 7, 1))
        },
    )
    .await;

    let wrote = store
        .capture(NewFact {
            fields: [("agent".to_string(), "an-agent-id".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                seat.role.clone(),
                "the line records its agent",
                date(2026, 7, 1),
            )
        })
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

pub async fn run_all_role_claims<M: Memory>(store: &M) {
    a_role_claimed_in_the_old_shape_is_still_held(store).await;
    a_released_role_object_is_not_read_past_to_the_old_keys(store).await;
    a_role_objects_other_keys_land_without_moving_the_lease(store).await;
    a_second_capture_racing_the_first_is_refused(store).await;
    a_rival_update_fact_claim_is_refused_while_the_lease_is_fresh(store).await;
    two_concurrent_captures_race_and_exactly_one_wins(store).await;
    a_stale_holders_own_write_is_refused_once_a_rival_has_taken_over(store).await;
    a_renewal_after_a_release_is_refused_and_the_role_stays_free(store).await;
}
