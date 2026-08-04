//! Stamp the running build's identity into the binary, for `ping` to report.
//!
//! `JOJOBOT_BUILD` from the environment, which is how the nix package supplies
//! it — a deployed binary is exactly the one nobody can inspect from the
//! outside. Otherwise `unknown`, which is a real answer: a build that cannot
//! say what it is says that, rather than reporting something plausible.
//!
//! Nothing here reaches the network, and nothing it emits describes anything
//! but jojobot's own binary.

fn main() {
    println!("cargo:rerun-if-env-changed=JOJOBOT_BUILD");

    let build = std::env::var("JOJOBOT_BUILD")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=JOJOBOT_BUILD={build}");
}
