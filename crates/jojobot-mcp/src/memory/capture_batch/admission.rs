//! **A batch is admitted against the list's record ceiling as a whole** (rule 498).
//!
//! The check pass judges each item by the path that writes it, but the ceiling is
//! decided inside the store write, which a check never reaches. So the batch asks
//! the one admission both stores call, once, for every item it will add: a batch
//! that cannot fit is refused with nothing written, and never lands the part that
//! does.

use jojobot_domain::memory::capacity::{self, Admission, Container};

use super::*;

impl Jojobot {
    /// The refusal of a batch that adds `adding` records to `subject` past its
    /// ceiling, or `None` when it fits or the list has no ceiling.
    pub(super) async fn refuses_a_batch_over_the_ceiling(
        &self,
        subject: &EntityId,
        kind: Option<EntityKind>,
        adding: usize,
        borrow: bool,
    ) -> Option<Result<CallToolResult, McpError>> {
        if adding == 0 {
            return None;
        }
        let held = match self.memory.fields(subject).await {
            Ok(held) => held,
            Err(e) => return Some(memory_declined("capture", e)),
        };
        capacity::ceiling_of(kind, &held)?;
        let filed = match self.memory.recall(subject).await {
            Ok(filed) => filed,
            Err(e) => return Some(memory_declined("capture", e)),
        };
        let live = capacity::live_records(&filed);
        match capacity::admit(
            Container {
                handle: subject,
                kind,
                held: &held,
            },
            &live,
            adding,
            borrow,
        ) {
            Admission::Refused(refused) => Some(memory_declined("capture", refused)),
            _ => None,
        }
    }
}
