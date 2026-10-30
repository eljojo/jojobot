//! The read boundary has no adapter dependency, including through dev edges.

#[test]
fn read_path_api_reaches_no_store_implementation() {
    let output = std::process::Command::new(env!("CARGO"))
        .args([
            "tree",
            "--offline",
            "--locked",
            "-p",
            "jojobot-query-api",
            "--edges",
            "normal,build,dev",
            "--prefix",
            "none",
        ])
        .output()
        .expect("cargo tree runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8(output.stdout).expect("cargo prints text");
    assert!(tree.lines().any(|line| line.starts_with("jojobot-domain ")));
    for line in tree.lines() {
        let name = line.split_whitespace().next().unwrap_or("");
        assert!(!reaches_a_store(name), "{tree}");
    }
}

/// **A crate that holds or drives a store**: the adapters, and every crate of the
/// sqlx family (the driver crates carry their own names, `sqlx-mysql` among them).
fn reaches_a_store(name: &str) -> bool {
    name == "jojobot-adapters" || name == "sqlx" || name.starts_with("sqlx-")
}

#[test]
fn the_store_check_names_the_sqlx_driver_crates_too() {
    assert!(reaches_a_store("sqlx-mysql"));
    assert!(reaches_a_store("sqlx-core"));
    assert!(reaches_a_store("jojobot-adapters"));
    assert!(!reaches_a_store("jojobot-domain"));
}
