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
