//! The case that the gate reads markdown at the workspace root, kept apart so the
//! roster file holds its size.

use super::*;

/// **The gate reads markdown at the root of the workspace, not only under
/// `crates`.**
///
/// The root is where this repository's prose lives, and prose about this
/// software names handles: a document describing what a session should write
/// carries the handles it should write. Those files were outside the scan, so
/// an unlisted name in one was not a red run anywhere — the one file class the
/// gate exists for, in the one place it did not look.
///
/// Each half is paired, because a scan that reads nothing satisfies every
/// negative on its own: an off-roster handle at the root must be REPORTED, and
/// a roster handle in the same position must NOT be.
#[test]
fn the_gate_reads_markdown_at_the_workspace_root() {
    let unlisted = unlisted_handle("person");
    let scratch = Scratch::new("root-markdown");
    scratch.write("ROOT-DOC.md", &format!("the suite writes {unlisted} here"));
    scratch.write("crates/kept.rs", "// person:milhouse is listed");

    let violations = violations_in(&scanned_files(&scratch.0));

    assert!(
        violations.iter().any(|v| v.contains(&unlisted)),
        "an unlisted handle in a root markdown file has to be reported: {violations:?}"
    );
    assert!(
        !violations.iter().any(|v| v.contains("person:milhouse")),
        "…and a roster handle must still pass, or the report above is the scan \
         failing rather than the gate working: {violations:?}"
    );
}
