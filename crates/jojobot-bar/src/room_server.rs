//! Only room executors receive the binary carrying the compiled grader route.

use super::*;

pub(super) fn build(log: &Path, cargo: &str) -> std::io::Result<(bool, String)> {
    run_phase(
        log,
        "cargo build --workspace --locked --features=jojobot/room-harness",
        cargo,
        &[
            "build",
            "--workspace",
            "--locked",
            "--features=jojobot/room-harness",
        ],
    )
}

pub(super) fn bundle(
    log: &Path,
    cargo: &str,
    summary: &mut Summary,
) -> std::io::Result<Option<ExitCode>> {
    // Compiling the ordinary test lane can replace the profile's binary with
    // the default build. Select the room binary last, just before archiving.
    let (ok, _) = build(log, cargo)?;
    if ok {
        summary.phase_ok("harness-server", "compiled");
        Ok(None)
    } else {
        summary.phase_failed("harness-server", "see log");
        finish(summary, log);
        Ok(Some(ExitCode::FAILURE))
    }
}
