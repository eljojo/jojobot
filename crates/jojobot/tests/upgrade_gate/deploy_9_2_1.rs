//! **The fixture recorded from the build deploy 9.2.1 shipped (10097a43)**, the
//! build the operator runs now. A store that build filled is what this build
//! takes on its first boot, so every pass this build adds is proved against it.

use super::*;

/// The fixture recorded from the build deploy 9.2.1 shipped.
pub(super) const DEPLOY_9_2_1_FIXTURE_DIR: &str = "tests/fixtures/upgrade_deploy9_2_1";

#[tokio::test]
async fn the_current_binary_boots_on_a_store_the_deploy_9_2_1_binary_filled() {
    boots_on_a_store_filled_by(DEPLOY_9_2_1_FIXTURE_DIR, "boots-deploy9-2-1").await;
}
