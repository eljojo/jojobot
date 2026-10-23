//! Names and ownership of the scratch directories test harnesses make.

use std::sync::atomic::{AtomicU64, Ordering};

static SERIAL: AtomicU64 = AtomicU64::new(0);

/// A name unique to this process and call. Placement and cleanup belong to the caller.
pub fn name(label: &str) -> String {
    assert!(
        label_is_component(label),
        "a scratch label is one filename component"
    );
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    format!("jojobot-scratch-{}-{serial}-{label}", std::process::id())
}

/// The owner of exactly a name this helper makes. Labels never supply the pid.
pub fn owner(name: &str) -> Option<u32> {
    let mut parts = name.strip_prefix("jojobot-scratch-")?.splitn(3, '-');
    let pid = parts.next()?;
    let serial = parts.next()?;
    let label = parts.next()?;
    let owner: u32 = pid.parse().ok()?;
    let drawn: u64 = serial.parse().ok()?;
    (owner > 0
        && owner <= i32::MAX as u32
        && owner.to_string() == pid
        && drawn.to_string() == serial
        && label_is_component(label))
    .then_some(owner)
}

fn label_is_component(label: &str) -> bool {
    !label.is_empty() && !label.contains(['/', '\0'])
}
