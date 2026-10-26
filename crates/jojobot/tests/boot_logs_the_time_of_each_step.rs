//! **A boot names every step that reads or rewrites the store, with the time it
//! took.** A step that found nothing to do used to log nothing, so a stretch of
//! a minute could pass between two lines with no line saying what ran. The log
//! the binary writes under its default filter is read back, so a step that drops
//! out of the boot sequence, or out of the filter, is a line missing here.
//!
//! **A test binary of its own.** `boot_store` seeds the process-wide kind set on
//! success, and a case running beside this one would see it already loaded.

use std::sync::{Arc, Mutex};

use jojobot_domain::clock::Clock;
use jojobot_domain::memory::kinds;

use jojobot_adapters::testing::{Scratch, free_port};

#[derive(Clone, Default)]
struct Kept(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Kept {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("log buffer").extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Kept {
    type Writer = Kept;
    fn make_writer(&'a self) -> Kept {
        self.clone()
    }
}

/// The steps of `wiring::boot_store`, in the order it runs them.
const STEPS: [&str; 19] = [
    "store-up",
    "badge-the-unbadged",
    "rekey-handle-keyed-rows",
    "load-kinds",
    "record-supplied-handles",
    "adopt-handles-without-a-row",
    "resolve-stale-pointer-columns",
    "migrate-reference-fields",
    "field-links",
    "migrate-forward-names",
    "migrate-forwarded-links",
    "repair-stored-rows",
    "storage-check",
    "permanent-ids",
    "due-days",
    "search-mail",
    "search-sessions",
    "startup-passes",
    "old-role-claims",
];

#[tokio::test]
async fn a_boot_logs_the_time_of_every_step_that_touches_the_store() {
    let path = jojobot_adapters::testing::scratch_path("jojobot-boot-logs-steps");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let port = free_port();

    let kept = Kept::default();
    let guard = tracing::subscriber::set_default(jojobot::logging::logging(
        kept.clone(),
        false,
        jojobot::logging::filter_for(None),
    ));
    let booted = jojobot::wiring::boot_store(&path, port, Clock::Real)
        .await
        .expect("the boot stands");
    drop(guard);

    let text = String::from_utf8(kept.0.lock().expect("log buffer").clone()).expect("utf8");
    for step in STEPS {
        let named = format!("step=\"{step}\"");
        let line = text
            .lines()
            .find(|line| line.contains(&named))
            .unwrap_or_else(|| panic!("no line names the step {step}:\n{text}"));
        assert!(
            line.contains("elapsed_ms="),
            "the line for {step} carries no time: {line}"
        );
    }

    let mut store = booted.store;
    store.stop().await;
    drop(scratch);
    kinds::load_shipped();
}
