//! A narrow summary names the log that holds the probe's panic.

mod support;

use std::{fs, path::Path, process::Command};

#[test]
fn sabotage_probe_reads_its_named_log_and_keeps_unmeasured_runs_unmeasured() {
    let root = std::env::temp_dir().join(format!("sabotage-probe-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let target = root.join("alpha.rs");
    let tool = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/sabotage");
    for (marker, kept, verdict) in [
        (true, true, "DEPENDED ON"),
        (false, true, "NOT MEASURED"),
        (true, false, "DEPENDED ON"),
    ] {
        fs::write(&target, "let alpha = 1;\n").unwrap();
        let ran = support::output_fenced(
            Command::new(&tool)
                .current_dir(&root)
                .env("SABOTAGE_STATE_DIR", root.join("state"))
                .args([target.to_str().unwrap(), "let alpha = 1;", "let alpha = 2;", "--"])
                .args(["python3", "-c", r#"
import pathlib, sys
probe = 'panic!' in pathlib.Path(sys.argv[1]).read_text()
said = 'SABOTAGE-PROBE-REACHED-THIS-SITE\n' if probe and sys.argv[2] == 'true' else 'build stopped\n'
if sys.argv[3] == 'true':
    pathlib.Path('narrow alpha.log').write_text(said)
    print('log: narrow alpha.log (1 lines)')
else:
    print(said)
sys.exit(101 if probe else 0)
"#])
                .arg(&target)
                .arg(marker.to_string())
                .arg(kept.to_string()),
        )
        .unwrap();
        let said = String::from_utf8_lossy(&ran.stdout);
        assert!(
            ran.status.success(),
            "{said}\n{}",
            String::from_utf8_lossy(&ran.stderr)
        );
        assert!(
            said.contains(&format!("SABOTAGE PROBE: {verdict}")),
            "{said}"
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "let alpha = 1;\n");
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn sabotage_reads_only_the_current_log_before_or_after_a_run_ends() {
    use std::os::unix::process::ExitStatusExt;

    let tool = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/sabotage");
    for (stop_at, stopped) in [("", false), ("clippy", false), ("build", true)] {
        let root = std::env::temp_dir().join(jojobot_bar::scratch::name("sabotage-current-log"));
        fs::create_dir_all(&root).unwrap();
        support::loop_repository(&root);
        let cargo = root.join("cargo");
        support::write_loop_cargo(
            &cargo,
            r#"#!/bin/sh
if [ "$1" = "$STOP_AT" ]; then
  kill -TERM "$PPID"
  exit 0
fi
case "$1" in
  test)
    case " $* " in
      *" --list "*)
        case " $* " in *" --ignored "*) ;; *) echo 'alpha::first: test' ;; esac
        exit 0 ;;
    esac
    echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s' ;;
esac
exit 0
"#,
        );
        let log = root.join("target/bar/narrow-jojobot-bar.log");
        fs::create_dir_all(log.parent().unwrap()).unwrap();
        fs::write(&log, "test result: ok. 99 passed; 0 failed;\n").unwrap();
        let ran = support::output_fenced(
            Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
                .current_dir(&root)
                .args(["narrow", "--crate", "jojobot-bar", "--filter", "alpha"])
                .env("CARGO", &cargo)
                .env("TMPDIR", &root)
                .env("BAR_HISTORY_DIR", root.join("history"))
                .env("STOP_AT", stop_at),
        )
        .unwrap();
        if stop_at.is_empty() {
            assert!(ran.status.success(), "{ran:?}");
        } else {
            assert_eq!(ran.status.signal(), Some(15), "{ran:?}");
        }
        let stdout = String::from_utf8(ran.stdout).unwrap();
        let current = fs::read_to_string(&log).unwrap();
        assert!(!current.contains("99 passed"), "{current}");
        let code = ran.status.code().unwrap_or(143);
        let read = support::output_fenced(
            Command::new("python3")
                .current_dir(&root)
                .args(["-c", r#"
import json, runpy, sys
g = runpy.run_path(sys.argv[1])
said = sys.argv[2]
read = g['bar_logs'](said)
print(json.dumps({'read': read, 'stopped': g['stopped_before_any_test'](int(sys.argv[3]), said + read)}))
"#])
                .arg(&tool)
                .arg(&stdout)
                .arg(code.to_string()),
        )
        .unwrap();
        assert!(read.status.success(), "{read:?}");
        let got: serde_json::Value = serde_json::from_slice(&read.stdout).unwrap();
        assert_eq!(got["stopped"], stopped, "{stop_at}: {stdout}\n{current}");
        assert_eq!(got["read"], format!("\n{current}"), "{stop_at}: {stdout}");
        fs::remove_dir_all(root).unwrap();
    }
}
