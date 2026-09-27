//! **A process serves one store.** `wiring::open_provisioned` seeds the
//! process-wide kind set (`memory::kinds::load`) as its first act; a second
//! store provisioned in the same process would silently take over that
//! vocabulary out from under the first, with no error anywhere naming what
//! happened.
//!
//! **A test binary of its own, because the state under test is process-wide
//! by construction** — exactly why `unseeded_boot_order.rs` is one too. This
//! file is the one case that needs TWO provisions in one process to prove the
//! guard at all, so it cannot share a binary with anything else that calls
//! `open_provisioned`.

use std::path::PathBuf;

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::free_port;
use jojobot_domain::memory::owned::Provisions;

/// A directory of this run's own, removed when it is done.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn a_real_store(name: &str) -> (Scratch, Dolt, DoltMemory) {
    let path = std::env::temp_dir().join(format!(
        "jojobot-single-store-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let server = Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    migrate::run(server.pool()).await.expect("the schema");
    let bare = DoltMemory::open(server.pool().clone());
    (scratch, server, bare)
}

/// **The first provision in a process succeeds; a second one is refused.**
/// Paired in one case because the second's refusal is meaningless without
/// proof the first still works exactly as it did before this guard existed.
#[tokio::test]
async fn a_second_provision_in_the_same_process_is_refused() {
    let (scratch_a, mut server_a, bare_a) = a_real_store("a").await;
    let resolved_a = jojobot::wiring::open_provisioned(bare_a, Provisions::new(vec![]))
        .await
        .expect("the first provision in a process must succeed");
    resolved_a
        .list_entities(None)
        .await
        .expect("the first store answers reads, exactly as before this guard existed");

    let (scratch_b, mut server_b, bare_b) = a_real_store("b").await;
    let err = match jojobot::wiring::open_provisioned(bare_b, Provisions::new(vec![])).await {
        Ok(_) => panic!("a second store provisioned in the same process must be refused"),
        Err(e) => e,
    };
    let message = format!("{err:#}");
    assert!(
        message.contains("one store"),
        "the refusal must name why a process refuses a second store: {message}"
    );

    server_a.stop().await;
    server_b.stop().await;
    drop(scratch_a);
    drop(scratch_b);
}
