//! **A script written on one thread is never "busy" because another thread
//! forked.**
//!
//! Eight threads each write a script and run it, again and again, which is what
//! a suite of cases that each stand up a stand-in agent does at once. Without
//! the gate, a fork on one thread copies a write-open script from another and
//! the exec on the other thread fails with `ETXTBSY`. The case counts those
//! failures, and any other failure to start, and expects none.

use std::process::Command;

use jojobot_exercise::spawn_gate;

const THREADS: usize = 8;
const ROUNDS: usize = 150;

#[test]
fn scripts_written_and_run_on_many_threads_are_never_busy() {
    let dir = std::env::temp_dir().join(jojobot_ports::scratch::name("spawn-gate"));
    std::fs::create_dir_all(&dir).expect("a scratch directory");

    let failures: usize = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..THREADS)
            .map(|worker| {
                let dir = dir.clone();
                scope.spawn(move || {
                    let mut failed = 0;
                    for round in 0..ROUNDS {
                        let script = dir.join(format!("script-{worker}-{round}"));
                        spawn_gate::write_script(&script, "#!/bin/sh\nexit 0\n")
                            .expect("the script is written");
                        match spawn_gate::guarded(|| Command::new(&script).spawn()) {
                            Ok(mut child) => {
                                child.wait().expect("the script finishes");
                            }
                            Err(_) => failed += 1,
                        }
                        let _ = std::fs::remove_file(&script);
                    }
                    failed
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().expect("a worker"))
            .sum()
    });
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        failures, 0,
        "scripts failed to start while others were being written"
    );
}

/// **A script another process still holds open for writing starts once that write
/// ends.** The lock above keeps this process's own forks away from a script being
/// written; a fork it does not control (a store the adapters start on another
/// thread) can still copy the descriptor, and the exec then reads `ETXTBSY` for the
/// moment between that child's fork and its exec. The case holds a write descriptor
/// open on a script through the first exec and lets go of it a few milliseconds
/// later, which is that window made long enough to see.
#[test]
fn a_script_still_open_for_writing_starts_once_the_write_ends() {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;

    let dir = std::env::temp_dir().join(jojobot_ports::scratch::name("spawn-busy"));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let script = dir.join("busy-script");
    let mut open = std::fs::File::create(&script).expect("the script is created");
    open.write_all(b"#!/bin/sh\nexit 0\n")
        .expect("the script is written");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("the script is runnable");

    let letting_go = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(10));
        drop(open);
    });
    let started = spawn_gate::spawn_retrying_when_busy(|| Command::new(&script).spawn());
    letting_go.join().expect("the other holder lets go");

    let mut child = started.expect("a script busy for a moment starts once the write ends");
    let finished = child.wait().expect("the script finishes").success();
    // The interpreter opens the script by its path after the exec, so the
    // directory stays until the child has ended.
    let _ = std::fs::remove_dir_all(&dir);
    assert!(finished);
}
