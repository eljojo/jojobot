//! **Test support for the suites that spawn a real store.**
//!
//! It is a module of the library rather than a helper beside one suite because
//! the thing it hands out — a port — is shared by every process on the
//! machine, and a helper copied into each suite is a rule each copy can drift
//! from. That drift is what this file exists to end: the same defect was
//! repaired three times in three copies, and the copy nobody repaired is the
//! one two runs meet on.
//!
//! **The ports come from `jojobot-ports`**, the one arbiter the rooms draw from
//! as well. This file once kept a scheme of its own over the same range as the
//! rooms', and a room and a store test were then offered one port at the same
//! moment.

/// **A port no other claimer anywhere will be given**, held for the rest of
/// this process and with nothing bound to it: the caller's server binds it
/// next. The claim lasts as long as the process, so however many tests it runs
/// none is offered the same port twice.
///
/// A range with no port left is a test that cannot start, and this says so in
/// the arbiter's own words. It returns a bare port because every caller wants
/// one and has nowhere to put an error.
///
/// **It also installs the store log sink** ([`install_store_log_sink`]): every
/// test that starts a store claims a port first, so this is the one call they
/// all make, and none has to remember a second.
pub fn free_port() -> u16 {
    install_store_log_sink();
    jojobot_ports::claim_for_life()
        .unwrap_or_else(|refusal| panic!("no port can be claimed for this test: {refusal}"))
}

/// **Install the log sink a test that starts a store reads its refusals from.**
/// Idempotent, and the process's one global subscriber: a second call, from any
/// thread, finds the first one's. `free_port` calls it, because a test that
/// claims a port is about to start a store.
pub fn install_store_log_sink() {
    let _ = crate::log_capture::log_sink();
}

/// **Everything the sink has kept so far**, process-wide, for a test that asserts
/// on what the store's refusal logged.
pub fn store_log() -> String {
    crate::log_capture::log_sink().text()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dolt::Dolt;
    use crate::dolt::tests::Scratch;

    /// **A port the suites are handed is claimed in the arbiter every harness
    /// shares**, so a room can never be offered it. The claim is read through
    /// the arbiter's own public door and not through anything this file
    /// holds: a port from a private scheme would come back claimable.
    #[test]
    fn a_free_port_is_held_in_the_arbiter_the_rooms_draw_from() {
        let port = free_port();
        assert!(
            jojobot_ports::try_claim(port)
                .expect("the claim can be tried")
                .is_none(),
            "a port from free_port() was free for another harness to claim",
        );
    }

    /// **Two stores start at the same moment, each on a port of its own, and
    /// both come up.**
    ///
    /// The real thing rather than a model of it: two servers, started
    /// concurrently on two claimed ports, each binding its own for real. This
    /// is the failure as it arrives in a run — `PortTaken`, on a suite that
    /// changed nothing — and it is the one a helper that only narrows the
    /// window still produces.
    ///
    /// **How fast a pool hands out a connection is not what this asserts**, so
    /// the case gives each start a window as long as the one a store has to
    /// answer in. The window `start` itself passes is a probe's, a fifth of a
    /// second, and a machine running several bars at once can take longer than
    /// that to open a connection without anything being wrong with the ports.
    #[tokio::test]
    async fn two_stores_started_together_both_take_a_port() {
        let scratch = Scratch::new("port-block-together");
        let here_port = free_port();
        let there_port = free_port();
        assert_ne!(
            here_port, there_port,
            "the arbiter handed one port to two claimers"
        );

        let window = std::time::Duration::from_secs(30);
        let here = scratch.0.join("one");
        let there = scratch.0.join("other");
        let (first, second) = tokio::join!(
            Dolt::start_acquiring_within(&here, here_port, window),
            Dolt::start_acquiring_within(&there, there_port, window),
        );

        let mut first = first.expect("the first store comes up");
        let mut second = second.expect("the second store comes up, on a port of its own");
        first.stop().await;
        second.stop().await;
    }

    /// **A process can hold a whole run's cumulative total, not just a handful
    /// of concurrent servers.**
    ///
    /// `free_port` claims a port for the life of the process, and every
    /// `#[tokio::test]` of a suite that spawns its own store draws one. The
    /// busiest lib draws about a hundred in one run, and the busiest room suite
    /// about two hundred over the rooms' own use of the same arbiter. A hundred
    /// distinct ports from one process is the figure this pins: a draw that
    /// repeated a port, or ran out early, would fail a test that changed
    /// nothing.
    #[test]
    fn a_process_is_handed_a_hundred_distinct_ports_in_one_run() {
        let handed: Vec<u16> = (0..100).map(|_| free_port()).collect();

        let mut unique = handed.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            100,
            "a hundred calls must hand out a hundred distinct ports: {handed:?}",
        );
    }
}
