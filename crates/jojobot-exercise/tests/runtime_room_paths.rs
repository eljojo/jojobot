use std::path::{Path, PathBuf};
use std::process::Command;

use jojobot_exercise::{expectations, failability};

const CHILD: &str = "JOJOBOT_ROOM_PATH_CASE";

struct Checkout(PathBuf);

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The already-built suite runs from elsewhere, with the package directory the
/// bundle runner supplies. Neither a rebuild nor the original files can help it.
fn in_checkout(case: &str) {
    let checkout = Checkout(std::env::temp_dir().join(jojobot_ports::scratch::name("room-paths")));
    for dir in ["rooms", "tests", "elsewhere"] {
        std::fs::create_dir_all(checkout.0.join(dir)).unwrap();
    }
    std::fs::write(
        checkout.0.join("rooms/vault.md"),
        "the room in the running checkout",
    )
    .unwrap();
    std::fs::write(
        checkout.0.join("tests/desk_lock.rs"),
        "fn the_proof_in_the_running_checkout() {}\n",
    )
    .unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", case, "--nocapture"])
        .env(CHILD, &checkout.0)
        .env("CARGO_MANIFEST_DIR", &checkout.0)
        .current_dir(checkout.0.join("elsewhere"));
    let out = command.output().unwrap();
    assert!(out.status.success(), "{case}: {out:?}");
}

#[test]
fn room_paths_read_the_running_checkouts_document() {
    let Some(root) = std::env::var_os(CHILD) else {
        return in_checkout("room_paths_read_the_running_checkouts_document");
    };
    let expected = Path::new(&root).join("rooms/vault.md");
    let path = expectations::room_document(expectations::VAULT_ROOM);
    assert_eq!(path, expected, "the runner supplied another checkout");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "the room in the running checkout"
    );
    std::fs::remove_file(&path).unwrap();
    assert!(
        std::fs::read_to_string(expectations::room_document(expectations::VAULT_ROOM)).is_err()
    );
}

#[test]
fn room_paths_read_the_running_checkouts_control_proof() {
    if std::env::var_os(CHILD).is_none() {
        return in_checkout("room_paths_read_the_running_checkouts_control_proof");
    }
    let control = failability::NegativeControl {
        room: expectations::VAULT_ROOM,
        lock: "a control in this checkout",
        file: "tests/desk_lock.rs",
        function: "the_proof_in_the_running_checkout",
        strength: failability::Strength::Discriminating,
    };
    assert!(failability::proof_exists(&control).is_ok());
    assert!(
        failability::proof_exists(&failability::NegativeControl {
            function: "a_proof_that_is_not_there",
            ..control
        })
        .is_err()
    );
}
