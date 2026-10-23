//! **The refusals of the key exchange for a keyless run's old name**: a run that
//! has ended, and a run whose record the store cannot read. Neither hands out a
//! key.

use crate::*;

/// **The refusal when the keyless run's record cannot be read**: damage in the
/// store, a person's to repair. It names the run and the field, never the cell.
pub(crate) fn damaged_keyless_run_declined(
    run: &jojobot_domain::session::SessionId,
    what: &str,
) -> CallToolResult {
    tracing::warn!(session = %run, why = %what, "a keyless run's record cannot be read");
    crate::caller::handle_declined(
        "",
        crate::answer::WayForward::person(format!(
            "{NOTHING_RAN}, and no key was handed out: run {run} is damaged in the store \
             ({what}). Sending the same call again will meet the same damage; only a person \
             can repair it, so tell the operator what is named here."
        )),
    )
}

/// **The refusal for the old name of a keyless run that has ended**, in the words
/// of how it ended and of the way back. It names the run by its public name and
/// hands out no key.
pub(crate) fn ended_keyless_run_declined(
    run: &sid::Handle,
    state: jojobot_domain::session::SessionState,
) -> CallToolResult {
    let name = run.name.as_str();
    let how = match state {
        jojobot_domain::session::SessionState::Wrapped => format!(
            "Run {name} began before keys existed and has wrapped, so its old name opens \
             nothing and no key is handed out. If you hold the wrap_code its wrap handed back, \
             call start_here with your bot name and resume set to that code; otherwise boot \
             again with start_here."
        ),
        _ => format!(
            "Run {name} began before keys existed and stopped without wrapping, so its old name \
             opens nothing and no key is handed out. To take it back, call start_here with your \
             bot name and resume set to {name}; otherwise boot again with start_here."
        ),
    };
    handle_declined("", format!("{NOTHING_RAN}. {how}"))
}
