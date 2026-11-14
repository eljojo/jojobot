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
    "load-kinds",
    "record-supplied-handles",
    "adopt-handles-without-a-row",
    "resolve-stale-pointer-columns",
    "field-links",
    "migrate-forwarded-links",
    "repair-stored-rows",
    "storage-check",
    "permanent-ids",
    "due-days",
    "search-memory",
    "search-mail",
    "search-sessions",
    "startup-passes",
    "old-role-claims",
    "claims-import",
    "claims-reconcile",
];

#[tokio::test]
async fn a_boot_logs_the_time_of_every_step_that_touches_the_store() {
    let path = jojobot_adapters::testing::scratch_path("jojobot-boot-logs-steps");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let port = free_port();

    // Leave real teaching history with the snapshot-table migration still owed.
    let (mut prior, _) = jojobot_adapters::dolt::Dolt::ready(&path, free_port())
        .await
        .unwrap();
    let writer = jojobot_adapters::writer::Writer::default();
    let (teaching, keeper) =
        jojobot_adapters::teaching_log::serving_kept(prior.pool(), writer.clone())
            .await
            .unwrap();
    jojobot_domain::teaching::Teachings::first_contact(
        &teaching,
        &jojobot_domain::session::Sid("homer".into()),
        "claims",
        jiff::Timestamp::UNIX_EPOCH,
    )
    .await
    .unwrap();
    drop((teaching, keeper));
    writer.drain().await;
    drop(writer);
    sqlx::query("DROP TABLE log_snapshot")
        .execute(prior.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM schema_migration WHERE version = '0125_log_snapshot'")
        .execute(prior.pool())
        .await
        .unwrap();
    prior.stop().await;

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
    let skips: Vec<_> = text
        .lines()
        .filter(|line| line.contains("reason="))
        .collect();
    assert!(
        skips
            .iter()
            .any(|line| line.contains("INFO") && line.contains("fingerprint")),
        "the real migration account reaches the serving boot: {text}"
    );
    assert!(!skips.iter().any(|line| line.contains("WARN")), "{text}");
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

    // **A boot whose import is still owed fills the search index after the import.**
    // The State holds no claims until the import has run, and a scan before it reads
    // the store one thing at a time: minutes over a store of thousands of things.
    let at = |step: &str| {
        text.find(&format!("step=\"{step}\""))
            .unwrap_or_else(|| panic!("no line names the step {step}"))
    };
    assert!(
        at("claims-reconcile") < at("search-memory"),
        "the search scan ran before the import finished:\n{text}"
    );

    // **And the corpus is filled**: the step line is logged whether the scan worked
    // or not, so the case reads the scan's own line and the absence of the warning.
    assert!(
        !text.contains("SEARCH INDEX EMPTY"),
        "the search fill failed:\n{text}"
    );
    let built = text
        .lines()
        .find(|line| line.contains("search: index built from a full scan"))
        .unwrap_or_else(|| panic!("no line says the index was built:\n{text}"));
    let docs: usize = built
        .split("docs=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|number| number.parse().ok())
        .unwrap_or_else(|| panic!("the line names no document count: {built}"));
    assert!(docs > 0, "the index was built from an empty scan: {built}");

    let mut store = booted.store;
    store.stop().await;
    drop(scratch);
    kinds::load_shipped();
}
