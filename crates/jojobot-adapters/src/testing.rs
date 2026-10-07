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
pub fn free_port() -> u16 {
    jojobot_ports::claim_for_life()
        .unwrap_or_else(|refusal| panic!("no port can be claimed for this test: {refusal}"))
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

    /// **Two stores start at the same moment and both come up.**
    ///
    /// The real thing rather than a model of it: two servers, started
    /// concurrently on two claimed ports, each binding its own for real. This
    /// is the failure as it arrives in a run — `PortTaken`, on a suite that
    /// changed nothing — and it is the one a helper that only narrows the
    /// window still produces.
    #[tokio::test]
    async fn two_stores_started_together_both_take_a_port() {
        let scratch = Scratch::new("port-block-together");

        let here = scratch.0.join("one");
        let there = scratch.0.join("other");
        let (first, second) = tokio::join!(
            Dolt::start(&here, free_port()),
            Dolt::start(&there, free_port()),
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
