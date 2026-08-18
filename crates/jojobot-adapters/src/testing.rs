//! **Test support for the suites that spawn a real store.**
//!
//! It is a module of the library rather than a helper beside one suite because
//! the thing it hands out — a port — is shared by every process on the
//! machine, and a helper copied into each suite is a rule each copy can drift
//! from. That drift is what this file exists to end: the same defect was
//! repaired three times in three copies, and the copy nobody repaired is the
//! one two runs meet on.

use std::net::TcpListener;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU16, Ordering};

/// The first port these suites may use. Below this is where real services
/// live.
const FIRST: u16 = 20_000;

/// How many ports one block holds. A suite starts a handful of servers, so a
/// block is far larger than any single run needs, and the count of blocks is
/// what a machine's concurrent runs draw from.
const BLOCK: u16 = 64;

/// How many blocks the range holds.
///
/// The count is what keeps the last block **below the range the kernel hands
/// out for outgoing connections** — 32768 on Linux by default. A block above
/// that line is one an ordinary connection from any process can take a port
/// out of, including the store's own client connections, so a block claimed
/// there is not the exclusive thing this file says it is.
const BLOCKS: u16 = 199;

/// **A range of ports this process holds against every other process**, and
/// the cursor that hands them out one at a time.
///
/// The claim is a listener on the block's first port, **held open for as long
/// as the block exists**. That is the whole mechanism: a bound socket is the
/// operating system's own answer to "is this taken", it is visible to every
/// other process, and a run that dies releases it without anybody tidying up.
///
/// **Choosing a port and binding it later are two moments, and the gap between
/// them is where two runs collide.** A helper that binds a candidate, reads
/// that it is free and releases it has told the caller about a moment that has
/// already passed: the store binds the port after that, and a second run
/// testing the same candidate in between is told the same thing. Holding the
/// claim is what makes the answer keep being true.
///
/// It does not close the window against a process outside these suites, and
/// nothing here can: the store binds its own port, so a port this block hands
/// out is still unbound until the server takes it. `Dolt::start` refuses a
/// port it cannot take, which is where that case is caught.
pub struct PortBlock {
    /// The lowest port this block hands out. The claim sits below it.
    first: u16,
    /// How many ports this block has handed out.
    handed: AtomicU16,
    /// The claim itself. Never read — dropping it releases the block, so it is
    /// held rather than used.
    _claim: TcpListener,
}

impl PortBlock {
    /// Take a block no other process holds.
    ///
    /// Every run walks the blocks in the same order, and that is not a
    /// collision: the run that binds a block keeps it, so a run that arrives
    /// second is refused and moves to the next one. **Starting each run
    /// somewhere else would only make a collision less likely**, which is the
    /// class of repair this file replaces.
    pub fn claim() -> PortBlock {
        for block in 0..BLOCKS {
            let first = FIRST + block * BLOCK;
            if let Ok(claim) = TcpListener::bind(("127.0.0.1", first)) {
                return PortBlock {
                    first: first + 1,
                    handed: AtomicU16::new(0),
                    _claim: claim,
                };
            }
        }
        panic!(
            "every one of the {BLOCKS} port blocks this suite uses is held. Either {BLOCKS} runs \
             are going at once, or servers from an earlier run are still alive."
        );
    }

    /// A port out of this block that nothing on the machine is listening on.
    ///
    /// The bind here is a check and not a claim — it asks whether somebody
    /// outside these suites took a port inside our block, which the block
    /// itself cannot prevent.
    pub fn port(&self) -> u16 {
        for _ in 0..BLOCK {
            let offset = self.handed.fetch_add(1, Ordering::Relaxed);
            assert!(
                offset < BLOCK - 1,
                "this block's {BLOCK} ports are used up. A suite that needs more than one block \
                 is a suite this helper was not written for."
            );
            let port = self.first + offset;
            if TcpListener::bind(("127.0.0.1", port)).is_ok() {
                return port;
            }
        }
        panic!("no free port left in this run's block")
    }
}

/// **A port no other caller anywhere will be given** — this process's own
/// block, claimed once and shared by every suite in the binary.
pub fn free_port() -> u16 {
    static BLOCK: OnceLock<PortBlock> = OnceLock::new();
    BLOCK.get_or_init(PortBlock::claim).port()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dolt::Dolt;
    use crate::dolt::tests::Scratch;

    /// **Two runs are never offered one port.**
    ///
    /// A second [`PortBlock`] stands in for a second run of these suites: it
    /// starts from a cold cursor and asks for ports at the same moment, which
    /// is exactly what a second `cargo test` on the machine does. Two of them
    /// in one process is the only way to put that race under an assertion,
    /// and it is faithful where it matters — the state that decides the answer
    /// is the block, and a fresh block is what a fresh process has.
    ///
    /// With a cursor alone both walk the same numbers from the same start, so
    /// the first port each hands out is the same port.
    #[test]
    fn two_independent_claims_are_never_offered_one_port() {
        let one = PortBlock::claim();
        let other = PortBlock::claim();

        let mine: Vec<u16> = (0..8).map(|_| one.port()).collect();
        let theirs: Vec<u16> = (0..8).map(|_| other.port()).collect();

        assert!(
            mine.iter().all(|p| !theirs.contains(p)),
            "two runs were offered the same port: {mine:?} against {theirs:?}",
        );
    }

    /// **…and the ports really are free**, which is what says the first case
    /// is not passing on two disjoint sets of numbers somebody else is using.
    #[test]
    fn a_port_this_block_hands_out_is_one_nothing_holds() {
        let block = PortBlock::claim();
        let port = block.port();

        TcpListener::bind(("127.0.0.1", port)).expect("the port handed out is free to bind");
    }

    /// **A port inside this block that somebody else holds is not handed
    /// out.** The block keeps other runs of these suites away; it says nothing
    /// about a process that is not one of them, and the bind inside
    /// [`PortBlock::port`] is what covers that.
    #[test]
    fn a_port_an_outsider_holds_is_passed_over() {
        let block = PortBlock::claim();
        let next = block.first;
        let outsider = TcpListener::bind(("127.0.0.1", next)).expect("the block's next port");

        let handed = block.port();

        assert_ne!(
            handed, next,
            "a port already bound was handed out, so the store would fail to take it",
        );
        drop(outsider);
    }

    /// **Two stores start at the same moment and both come up.**
    ///
    /// The real thing rather than a model of it: two servers, started
    /// concurrently from two blocks, each binding its own port for real. This
    /// is the failure as it arrives in a run — `PortTaken`, on a suite that
    /// changed nothing — and it is the one a helper that only narrows the
    /// window still produces.
    #[tokio::test]
    async fn two_stores_started_together_both_take_a_port() {
        let scratch = Scratch::new("port-block-together");
        let one = PortBlock::claim();
        let other = PortBlock::claim();

        let here = scratch.0.join("one");
        let there = scratch.0.join("other");
        let (first, second) = tokio::join!(
            Dolt::start(&here, one.port()),
            Dolt::start(&there, other.port()),
        );

        let mut first = first.expect("the first store comes up");
        let mut second = second.expect("the second store comes up, on a port of its own");
        first.stop().await;
        second.stop().await;
    }
}
