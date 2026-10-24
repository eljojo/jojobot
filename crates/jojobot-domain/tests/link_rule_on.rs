//! **The link decision of rule 646, switched on, in a process of its own.**
//!
//! The switch is process-wide: the readers that draw links from a served value
//! hold no store to ask. A case that turns it on reaches every other test in its
//! process, so these get a binary to themselves, as the kind set does.

use jojobot_domain::memory::testing::{InMemoryMemory, contract};
use jojobot_domain::memory::{LinkRule, link_rule, set_link_rule};

#[tokio::test]
async fn the_fake_links_by_mark_when_the_rule_is_on() {
    set_link_rule(LinkRule::On);
    assert_eq!(link_rule(), LinkRule::On);
    contract::run_all_link_at_write(&InMemoryMemory::booted()).await;
}
