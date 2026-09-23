//! **The claim's own exclusivity, proven against any [`Memory`] adapter.**
//!
//! A role's holder and claim moment are decided atomically inside the same
//! write that lands them — see [`crate::session::role_write_in`] and
//! [`crate::session::claim_role`]. That is a property of the STORE, not of
//! the orientation door that calls it, so it belongs here rather than only
//! in an MCP-level test: the fake and the real adapter must both answer for
//! it, or a fake that only looks race-free is standing in for a real store
//! that still has one.

use super::support::{capture, ensure};
use super::*;

const ROLE: &str = "contract-dispatch";

fn role_fields(claimant: &str, at: jiff::Timestamp) -> std::collections::BTreeMap<String, String> {
    [
        (crate::session::role_holder_key(ROLE), claimant.to_string()),
        (crate::session::role_claimed_at_key(ROLE), at.to_string()),
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
    let bot = EntityId("bot:contract-role-capture-race".into());
    ensure(store, &bot).await;
    let now = crate::session::testing::contract::epoch();

    capture(
        store,
        NewFact {
            fields: role_fields("delta", now),
            ..NewFact::about(bot.clone(), "delta claims the role", date(2026, 7, 1))
        },
    )
    .await;

    let refused = store
        .capture(NewFact {
            fields: role_fields("epsilon", now),
            ..NewFact::about(bot.clone(), "epsilon claims the role too", date(2026, 7, 1))
        })
        .await;
    assert!(
        matches!(
            &refused,
            Err(MemoryError::RoleTaken { role, holder, .. })
                if role == ROLE && holder == "delta"
        ),
        "a second capture while the lease is fresh must be refused, naming the first holder: \
         {refused:?}",
    );

    let fields = folded_fields_of(store, &bot).await;
    assert_eq!(
        fields.get(&crate::session::role_holder_key(ROLE)),
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
    let bot = EntityId("bot:contract-role-update-race".into());
    ensure(store, &bot).await;
    let now = crate::session::testing::contract::epoch();

    let claimed = capture(
        store,
        NewFact {
            fields: role_fields("delta", now),
            ..NewFact::about(bot.clone(), "delta claims the role", date(2026, 7, 1))
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
            &bot,
        )
        .await;
    assert!(
        matches!(
            &refused,
            Err(MemoryError::RoleTaken { role, holder, .. })
                if role == ROLE && holder == "delta"
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
    let bot = EntityId("bot:contract-role-concurrent-race".into());
    ensure(store, &bot).await;
    let now = crate::session::testing::contract::epoch();

    let (a, b) = tokio::join!(
        store.capture(NewFact {
            fields: role_fields("delta", now),
            ..NewFact::about(bot.clone(), "delta races for the role", date(2026, 7, 1))
        }),
        store.capture(NewFact {
            fields: role_fields("epsilon", now),
            ..NewFact::about(bot.clone(), "epsilon races for the role", date(2026, 7, 1))
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
/// release both write `role_holder_key` and `role_claimed_at_key`
/// together, the same shape a claim writes, differing only in the
/// timestamp — so a former holder's own write, arriving after a rival has
/// legitimately taken over, is refused by [`crate::session::claim_role`]
/// exactly as a stranger's would be, never a blind overwrite.
pub async fn a_stale_holders_own_write_is_refused_once_a_rival_has_taken_over<M: Memory>(
    store: &M,
) {
    let bot = EntityId("bot:contract-role-takeover-race".into());
    ensure(store, &bot).await;
    let claimed_at = crate::session::testing::contract::epoch();
    let stale = claimed_at + crate::session::LEASE_FRESHNESS + jiff::SignedDuration::from_secs(1);

    let claimed = capture(
        store,
        NewFact {
            fields: role_fields("delta", claimed_at),
            ..NewFact::about(bot.clone(), "delta claims the role", date(2026, 7, 1))
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
            &bot,
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
                &bot,
            )
            .await;
        assert!(
            matches!(delta_write, Err(MemoryError::RoleTaken { ref holder, .. }) if holder == "epsilon"),
            "delta's {label}, arriving after epsilon's legitimate takeover, must be refused \
             naming epsilon — never silently accepted: {delta_write:?}",
        );
    }

    let fields = folded_fields_of(store, &bot).await;
    assert_eq!(
        fields.get(&crate::session::role_holder_key(ROLE)),
        Some(&"epsilon".to_string()),
        "epsilon's legitimate claim must survive both of delta's refused writes: {fields:?}",
    );
}

pub async fn run_all_role_claims<M: Memory>(store: &M) {
    a_second_capture_racing_the_first_is_refused(store).await;
    a_rival_update_fact_claim_is_refused_while_the_lease_is_fresh(store).await;
    two_concurrent_captures_race_and_exactly_one_wins(store).await;
    a_stale_holders_own_write_is_refused_once_a_rival_has_taken_over(store).await;
}
