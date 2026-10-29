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
        assert!(!matches!(name, "jojobot-adapters" | "sqlx"), "{tree}");
    }
}
