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
    let dir = std::env::temp_dir().join(format!("jojobot-spawn-gate-{}", std::process::id()));
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
