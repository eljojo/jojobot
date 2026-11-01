//! **The pass that takes the old role keys off a bot ends when a clear is
//! refused.** It runs at boot, so a clear that comes back blocked, with nothing
//! written, must leave the keys in place and end the pass, not repeat it.

use std::sync::Arc;

use super::support::{capture, computed, ensure};
use super::*;

/// A store that refuses every edit that clears a key and forwards the rest.
struct RefusesClears(Arc<dyn Memory>, std::sync::atomic::AtomicUsize);

#[crate::forward(
    forward_memory,
    via = 0,
    required_only,
    also(add_entity_with_first_claim, update_entity_with_claim)
)]
impl Memory for RefusesClears {
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        if patch.clear_fields.is_empty() {
            return self.0.update_fact(address, patch, caller).await;
        }
        // A pass that keeps sending the refused clear never yields to a timer,
        // so the store fails the loop itself after a few tries.
        if self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 10 {
            return Err(MemoryError::InvalidFact(
                "the pass sent the refused clear again and again".into(),
            ));
        }
        Ok(Guarded::Blocked {
            attempted: caller.clone(),
            candidates: Vec::new(),
        })
    }
}

#[tokio::test]
async fn a_blocked_clear_ends_the_pass_with_the_keys_in_place() {
    let inner = Arc::new(InMemoryMemory::booted());
    let bot = EntityId("bot:contract-role-moved".to_string());
    ensure(&*inner, &bot).await;
    let at = crate::session::testing::contract::epoch() + jiff::SignedDuration::from_secs(60);
    capture(
        &*inner,
        computed(NewFact {
            fields: [
                (
                    crate::session::role_keys::role_holder_key("contract-moved-role"),
                    "delta".to_string(),
                ),
                (
                    crate::session::role_keys::role_claimed_at_key("contract-moved-role"),
                    at.to_string(),
                ),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(
                bot.clone(),
                "claimed the contract-moved-role role",
                date(2026, 7, 1),
            )
        }),
    )
    .await;
    let store = RefusesClears(inner.clone(), Default::default());
    let pass = crate::session::role_keys::drop_old_role_keys(&store, &bot, "contract-moved-role")
        .await
        .expect("a refused clear ends the pass without an error");
    assert_eq!(pass, 0, "nothing was cleared");
    let held = inner.fields(&bot).await.expect("the bot's fields");
    assert!(
        held.contains_key(&crate::session::role_keys::role_holder_key(
            "contract-moved-role"
        )),
        "the refused clear left the key: {held:?}"
    );
}
