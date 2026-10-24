//! **The unit stops the server alone and lets the server stop its store.**
//!
//! `KillMode=control-group`, systemd's default, sends the stop signal to every
//! process of the unit at once, so the store dies at the signal and the drain
//! window has no store to finish its calls with. `mixed` sends it to the main
//! process only and kills what is left after that process has exited. The key's
//! spelling is the service manager's, and no process of ours reads it, so it is
//! pinned here against the module that sets it.

const MODULE: &str = include_str!("../../../nix/modules/jojobot.nix");

#[test]
fn the_unit_signals_the_main_process_alone_at_a_stop() {
    assert!(
        MODULE
            .lines()
            .any(|line| line.trim() == r#"KillMode = "mixed";"#),
        "the unit does not set KillMode = \"mixed\", so a stop signals the store with the server"
    );
}
