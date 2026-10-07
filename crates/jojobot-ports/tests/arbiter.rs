//! **The port arbiter, held against what it exists for.**
//!
//! Two runs on one machine must never be given the same port, and a run that
//! cannot be given one must be told so in words, not by a panic.

use std::collections::BTreeSet;
use std::net::TcpListener;
use std::path::PathBuf;

use jojobot_ports::{Allocator, END, FIRST, PortsError};

/// A directory of this case's own, so a case holds no claim another run needs.
fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "jojobot-ports-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// **Concurrent claimers never share a port.** Eight threads each draw twelve
/// ports through an allocator of their own over one directory of claim files.
/// An independent open of a claim file contends even inside one process, so
/// this is the race between runs and not a stand-in for it. The count is the
/// control: every one of the ninety-six claims was granted, so a thread that
/// was simply refused cannot make the uniqueness below pass.
#[test]
fn claimers_with_their_own_allocators_never_share_a_port() {
    let dir = scratch("share");
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let dir = dir.clone();
            std::thread::spawn(move || {
                let allocator = Allocator::within(31_000, 31_400, dir);
                (0..12)
                    .map(|_| {
                        // Only the lock protects the port from here on: the
                        // listener is what a harness lets go of before it
                        // starts its server.
                        allocator
                            .claim()
                            .expect("a port is free")
                            .release_listener()
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let held: Vec<_> = handles
        .into_iter()
        .flat_map(|h| h.join().expect("a claimer thread"))
        .collect();
    let ports: BTreeSet<u16> = held.iter().map(|c| c.port()).collect();
    assert_eq!(held.len(), 96, "a claimer was refused a port");
    assert_eq!(ports.len(), held.len(), "two claimers were given one port");
    assert!(ports.iter().all(|p| (31_000..31_400).contains(p)));
}

/// **An exhausted range is a refusal that says so.** A range of eight ports is
/// claimed until the allocator has none left; the answer is the named error,
/// not a panic, and it names the range. Letting one claim go makes the next
/// succeed — the control that the refusal was about the range being full.
#[test]
fn an_exhausted_range_is_a_named_refusal_and_not_a_panic() {
    let dir = scratch("exhaust");
    let allocator = Allocator::within(31_900, 31_908, dir);
    let mut held = Vec::new();
    let refusal = loop {
        match allocator.claim() {
            Ok(claim) => held.push(claim.release_listener()),
            Err(e) => break e,
        }
    };
    assert!(!held.is_empty(), "nothing in the range could be claimed");
    assert!(held.len() <= 8, "more claims than ports: {}", held.len());
    match &refusal {
        PortsError::Exhausted { first, end } => {
            assert_eq!((*first, *end), (31_900, 31_908));
            let said = refusal.to_string();
            assert!(
                said.contains("31900") && said.contains("31908"),
                "the refusal does not name the range: {said}"
            );
        }
        other => panic!("expected the range to be exhausted, got {other:?}"),
    }
    let released = held.pop().expect("a claim to release").port();
    // The lock alone, not a fresh bind: a process outside these harnesses can
    // take the port in the moment after the release, and that is no business of
    // the arbiter's lock.
    //
    // **Promptly, not instantly.** A lock belongs to an open file, and a
    // process being spawned on another thread of this binary holds a copy of
    // every open descriptor until it execs, so a release can land a few
    // milliseconds late. The sibling case below spawns processes at the same
    // time as this one runs.
    let mut again = false;
    for _ in 0..50 {
        if allocator
            .try_claim(released)
            .expect("the claim can be tried")
            .is_some()
        {
            again = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
    assert!(
        again,
        "a released port could not be claimed again in two seconds"
    );
}

/// **A listener on a port no other process is handed.** A port the kernel
/// picks for the caller (`bind` to port 0) comes from the outgoing range, where
/// any process opening a connection can take it the moment this one lets go, so
/// a case that frees it and claims it again fails on a loaded machine. A port
/// below that range is taken only by a process that asks for it by number.
fn an_outsider_on_a_port_the_kernel_never_hands_out() -> (TcpListener, u16) {
    let span = u32::from(END - FIRST);
    let start = std::process::id().wrapping_mul(2_654_435_761);
    for step in 0..span {
        let port = FIRST + (start.wrapping_add(step) % span) as u16;
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
            return (listener, port);
        }
    }
    panic!("no port in the arbiter's own range was free to listen on");
}

/// **A port a process outside these harnesses holds is passed over.** The lock
/// keeps other claimers away; it says nothing about a process that does not
/// ask it, and the bind beside the lock is what covers that. The range here is
/// the one port an outsider is listening on, so the only honest answer is that
/// nothing can be claimed, and the control is the same allocator over a port
/// nobody holds, which claims it.
#[test]
fn a_port_an_outsider_listens_on_is_passed_over() {
    let (outsider, held) = an_outsider_on_a_port_the_kernel_never_hands_out();
    assert!(
        held < 32_768,
        "the outsider is on a port from the outgoing range, so another process can take it \
         the moment it is freed",
    );
    let over_the_outsider = Allocator::within(held, held + 1, scratch("outsider"));
    assert!(
        matches!(over_the_outsider.claim(), Err(PortsError::Exhausted { .. })),
        "a port an outsider was already bound to was handed out, so the store would fail to take it",
    );
    drop(outsider);
    let free = Allocator::within(held, held + 1, scratch("outsider-gone"));
    assert_eq!(
        free.claim().expect("the port is free now").port(),
        held,
        "the same port was refused after the outsider let go",
    );
}

/// **The range stays below the kernel's outgoing range**, where an ordinary
/// connection from any process can take a port out from under a claim.
#[test]
fn the_range_stays_below_the_outgoing_connection_range() {
    // Read through `black_box` so the assertion is over what the library says
    // and not over a constant the compiler can fold away.
    let (first, end) = (std::hint::black_box(FIRST), std::hint::black_box(END));
    assert!(first >= 20_000, "the range reaches into real services");
    assert!(end <= 32_768, "the range reaches the outgoing range");
}

/// **A port handed out for the life of the process can be bound, and stays
/// claimed.** Nothing is left listening on it, because the caller's server
/// binds it next; the claim is what keeps every other run off it.
#[test]
fn a_port_claimed_for_life_can_be_bound_and_stays_claimed() {
    let port = jojobot_ports::claim_for_life().expect("a port");
    drop(TcpListener::bind(("127.0.0.1", port)).expect("the port is free to bind"));
    assert!(
        jojobot_ports::try_claim(port)
            .expect("the claim can be tried")
            .is_none(),
        "a port claimed for life was free for another claimer",
    );
}

/// The other process in the case below: told which port, it reports whether it
/// could claim it. Run on its own, with no port named, it does nothing.
#[test]
fn a_second_process_reports_whether_it_could_claim_the_port() {
    let Ok(port) = std::env::var("JOJOBOT_CLAIM_PROBE_PORT") else {
        return;
    };
    let port: u16 = port.parse().expect("a port number");
    let said = match jojobot_ports::try_claim(port).expect("the claim can be tried") {
        Some(_) => "PROBE:claimed",
        None => "PROBE:held-elsewhere",
    };
    println!("{said}");
}

/// **A claim reaches another PROCESS, and goes when its holder lets go.** A
/// real second process must be refused the port while this one holds it and
/// given it afterwards; the second answer is the control, since a probe that is
/// always refused would satisfy the first on its own.
#[test]
fn another_process_cannot_claim_a_held_port_and_can_once_it_is_released() {
    let held = jojobot_ports::claim().expect("a port");
    let port = held.port();
    let probe = || {
        let out = std::process::Command::new(std::env::current_exe().expect("this binary"))
            .args([
                "--exact",
                "a_second_process_reports_whether_it_could_claim_the_port",
                "--nocapture",
            ])
            .env("JOJOBOT_CLAIM_PROBE_PORT", port.to_string())
            .output()
            .expect("the second process runs");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let while_held = probe();
    assert!(
        while_held.contains("PROBE:held-elsewhere"),
        "a second process was given a port this one holds: {while_held}",
    );
    drop(held);
    let after = probe();
    assert!(
        after.contains("PROBE:claimed"),
        "the port stayed claimed after its holder let go: {after}",
    );
}

/// **A claim file the caller cannot open is a port held by somebody else.**
/// Another user's run leaves a claim file this one may not write, and that is
/// the same answer as a held lock: the port is not this claimer's, and the next
/// one may be. An error here would stop the whole claim while ports are free.
#[test]
fn a_claim_file_another_user_made_is_a_port_that_is_held() {
    use std::os::unix::fs::PermissionsExt;

    let dir = scratch("unwritable");
    let first = 31_900;
    let file = dir.join(first.to_string());
    std::fs::write(&file, b"").expect("a claim file");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o444))
        .expect("a file nobody may write");

    // The lock alone answers that the port is somebody's.
    let alone = Allocator::within(first, first + 1, dir.clone());
    assert!(
        alone.try_claim(first).expect("not an error").is_none(),
        "a claim file this claimer cannot open was treated as free or as a failure",
    );
    // A range of that one port is exhausted, in words, and not an I/O error.
    assert!(
        matches!(alone.claim(), Err(PortsError::Exhausted { .. })),
        "a port that cannot be claimed stopped the claim with another error",
    );
    // Beside ports that are free, no draw lands on it and none fails because of
    // it, wherever the search starts. The range is wide because a harness on
    // the same machine may be listening on any single port in it.
    for _ in 0..16 {
        let wide = Allocator::within(first, first + 40, dir.clone());
        let claim = wide.claim().expect("one of the other ports is free");
        assert_ne!(claim.port(), first, "the unwritable claim was handed out");
    }
}
