//! **The fixture recorded from the build deploy 9.1 shipped (b1409450)**, the
//! build the operator runs now. It is the first store recorded after 0108, so
//! a migration numbered past it (0109 on) is proved here against the store it
//! lands on in production.

use super::*;

/// The fixture recorded from the build deploy 9.1 shipped.
pub(super) const DEPLOY_9_1_FIXTURE_DIR: &str = "tests/fixtures/upgrade_deploy9_1";

#[tokio::test]
async fn the_current_binary_boots_on_a_store_the_deploy_9_1_binary_filled() {
    boots_on_a_store_filled_by(DEPLOY_9_1_FIXTURE_DIR, "boots-deploy9-1").await;
}
