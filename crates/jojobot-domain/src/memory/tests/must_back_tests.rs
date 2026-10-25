//! **A rewrite of a claim a newer claim displaced is refused**, on the fake. The
//! real store runs the same case in the adapters' tests.

use crate::memory::testing::{InMemoryMemory, contract};

#[tokio::test]
async fn the_fake_refuses_a_rewrite_of_a_claim_a_newer_claim_displaced() {
    contract::a_rewrite_of_a_claim_a_newer_claim_displaced_is_refused(&InMemoryMemory::booted())
        .await;
}
