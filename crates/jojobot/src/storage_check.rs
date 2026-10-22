//! **What the boot says about the damage the store check found.** The pass
//! itself is [`jojobot_domain::storage_check::StorageCheck`]; this reports each
//! class it counted. A class that found something is a warning with its count
//! and up to twenty ids. A store with none says so in one line that names every
//! class checked, so silence is never read as a pass that did not run. The pass
//! never stops a boot.

use jojobot_domain::storage_check::{StorageCheck, StorageClass};

/// Run the check once and log what it found.
pub async fn report_storage_damage(check: &dyn StorageCheck) {
    let started = std::time::Instant::now();
    let found = match check.check_storage().await {
        Ok(found) => found,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "STORAGE CHECK NOT RUN — the store could not be read, so nothing is known about \
                 damage a cut-off write left. The server starts; a restart repeats the check."
            );
            return;
        }
    };
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let mut damaged = 0usize;
    let mut tolerated = 0usize;
    for finding in found.iter().filter(|finding| finding.count > 0) {
        if finding.class.reported_at_info() {
            tolerated += 1;
            tracing::info!(
                class = finding.class.token(),
                count = finding.count,
                ids = ?finding.ids,
                "storage check: {}. Nothing was changed.",
                finding.class.meaning()
            );
            continue;
        }
        damaged += 1;
        tracing::warn!(
            class = finding.class.token(),
            count = finding.count,
            ids = ?finding.ids,
            "STORAGE DAMAGE — {}. Nothing was changed.",
            finding.class.meaning()
        );
    }
    if damaged == 0 && tolerated == 0 {
        let checked = StorageClass::ALL
            .iter()
            .map(|class| class.token())
            .collect::<Vec<_>>()
            .join("; ");
        tracing::info!(
            elapsed_ms,
            classes = StorageClass::ALL.len(),
            "storage check: none found in {checked}"
        );
    } else {
        tracing::info!(
            elapsed_ms,
            classes = StorageClass::ALL.len(),
            damaged,
            tolerated,
            "storage check: finished"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jojobot_domain::storage_check::{IDS_SHOWN, SeededStorage, StorageCheckError};
    use std::sync::{Arc, Mutex};

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

    impl Kept {
        fn lines(&self) -> Vec<String> {
            String::from_utf8(self.0.lock().expect("log buffer").clone())
                .expect("utf8")
                .lines()
                .map(str::to_string)
                .collect()
        }
    }

    fn capture() -> (Kept, tracing::subscriber::DefaultGuard) {
        let kept = Kept::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(kept.clone())
            .with_ansi(false)
            .with_max_level(tracing::Level::INFO)
            .finish();
        (kept, tracing::subscriber::set_default(subscriber))
    }

    #[tokio::test]
    async fn a_clean_store_logs_one_line_naming_every_class() {
        let (kept, _guard) = capture();
        report_storage_damage(&SeededStorage::default()).await;
        let lines = kept.lines();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("INFO") && lines[0].contains("none found"));
        for class in StorageClass::ALL {
            assert!(
                lines[0].contains(class.token()),
                "the line names {}: {}",
                class.token(),
                lines[0]
            );
        }
    }

    #[tokio::test]
    async fn each_class_found_is_one_warning_with_its_whole_count() {
        let (kept, _guard) = capture();
        let mut store = SeededStorage::default();
        for class in StorageClass::ALL {
            store = store.with(*class, &format!("the-one-of-{}", class.token()));
        }
        for n in 0..(IDS_SHOWN + 5) {
            store = store.with(StorageClass::HalfDoneWrite, &format!("extra-{n}"));
        }
        report_storage_damage(&store).await;
        let lines = kept.lines();
        let warnings: Vec<&String> = lines.iter().filter(|l| l.contains("WARN")).collect();
        let damage_classes = StorageClass::ALL
            .iter()
            .filter(|class| !class.reported_at_info())
            .count();
        assert!(damage_classes > 0 && damage_classes < StorageClass::ALL.len());
        assert_eq!(
            warnings.len(),
            damage_classes,
            "one warning per class that is damage: {lines:?}"
        );
        let cut = warnings
            .iter()
            .find(|l| l.contains(StorageClass::HalfDoneWrite.token()))
            .expect("the half-done class is warned about");
        assert!(
            cut.contains(&format!("count={}", IDS_SHOWN + 6)),
            "the count is whole: {cut}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("none found")),
            "a store with damage does not say none was found: {lines:?}"
        );
    }

    #[tokio::test]
    async fn a_class_a_store_holds_on_purpose_is_counted_at_info_and_is_not_damage() {
        let (kept, _guard) = capture();
        let store = SeededStorage::default()
            .with(StorageClass::MergedWithoutEvent, "thing:kettle")
            .with(StorageClass::MergedWithoutEvent, "thing:cactus");
        report_storage_damage(&store).await;
        let lines = kept.lines();
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("WARN") || l.contains("none found")),
            "{lines:?}"
        );
        let counted = lines
            .iter()
            .find(|l| l.contains(StorageClass::MergedWithoutEvent.token()))
            .expect("the count is reported");
        assert!(
            counted.contains("INFO") && counted.contains("count=2"),
            "{counted}"
        );
        assert!(counted.contains("thing:kettle"), "{counted}");
    }

    struct Unreadable;

    #[async_trait::async_trait]
    impl StorageCheck for Unreadable {
        async fn check_storage(
            &self,
        ) -> Result<Vec<jojobot_domain::storage_check::Finding>, StorageCheckError> {
            Err(StorageCheckError("the store went away".into()))
        }
    }

    #[tokio::test]
    async fn a_store_that_cannot_be_read_warns_that_the_check_did_not_run() {
        let (kept, _guard) = capture();
        report_storage_damage(&Unreadable).await;
        let lines = kept.lines();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("WARN") && lines[0].contains("NOT RUN"));
        assert!(!lines[0].contains("none found"));
    }
}
