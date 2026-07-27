//! The formatting gate.
//!
//! The workspace was reformatted once, wholesale. **A one-time reformat with no
//! gate re-rots inside a week** — the drift it cleared had reached 791 hunks
//! across every crate, none of it deliberate, all of it noise in every diff
//! anybody had to read since.
//!
//! It lives as a test rather than as CI config because that is where this
//! repo's other machine gate lives (see `fixture_roster`): the green bar is
//! `nix develop -c cargo test`, so a rule enforced anywhere else is a rule that
//! is not enforced.
//!
//! It asserts, it never rewrites. A test that silently reformatted your working
//! tree would turn a red bar into a mysterious diff.

use std::path::Path;
use std::process::Command;

/// `cargo fmt --check` over the whole workspace.
///
/// **Skipped, loudly, when cargo is not on PATH.** The suite has to stay
/// runnable by a tool that vendors the crate without the toolchain around it;
/// what it must not do is pass silently in that case, so the skip prints.
#[test]
fn the_workspace_is_formatted() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the workspace root is two levels above this crate")
        .join("Cargo.toml");

    let ran = Command::new(env!("CARGO"))
        .args(["fmt", "--all", "--check", "--manifest-path"])
        .arg(&manifest)
        .output();

    let output = match ran {
        Ok(output) => output,
        Err(e) => {
            println!("SKIPPED: cargo could not be run ({e}) — formatting was NOT checked");
            return;
        }
    };

    assert!(
        output.status.success(),
        "the workspace is not formatted. Run `cargo fmt --all` and commit the result — on its \
         own, never mixed into a change somebody has to review.\n\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
