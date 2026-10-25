//! Refuse the bar while a crashed sabotage left a break in this checkout.

use std::process::Command;

/// Whether the bar must stop. The tool says what to put back. A bar fixture has
/// no tool above it and is not gated; a tool that cannot run gates, and says why.
pub fn left_applied() -> bool {
    let cwd = std::env::current_dir().expect("the bar has a working directory");
    let Some(tool) = cwd
        .ancestors()
        .map(|root| root.join("scripts/sabotage"))
        .find(|tool| tool.is_file())
    else {
        return false;
    };
    match Command::new("python3").arg(tool).arg("--gate").status() {
        Ok(status) => !status.success(),
        Err(error) => {
            eprintln!("bar: the sabotage gate cannot run: {error}");
            true
        }
    }
}
