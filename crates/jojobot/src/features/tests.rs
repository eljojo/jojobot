use super::*;
use jojobot_domain::memory::testing::{InMemoryMemory, contract::state};
use jojobot_domain::memory::{EntityId, NewEntity, NewFact};
use tracing::instrument::WithSubscriber;

#[derive(Clone, Default)]
struct Kept(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Kept {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
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

async fn held(fields: &[(&str, &str)]) -> Arc<InMemoryMemory> {
    let memory = Arc::new(InMemoryMemory::booted());
    let instance = EntityId("topic:instance".into());
    memory
        .add_entity(NewEntity::new(instance.clone(), "Instance", "fixture"))
        .await
        .unwrap()
        .written()
        .unwrap();
    memory
        .capture(state(NewFact {
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..NewFact::about(
                instance,
                "instance settings",
                jiff::civil::date(2026, 10, 28),
            )
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    memory
}

async fn logged(memory: Arc<dyn Memory>, legacy: Option<&str>) -> (anyhow::Result<Folded>, String) {
    let kept = Kept::default();
    let subscriber = crate::logging::logging(
        kept.clone(),
        false,
        tracing_subscriber::EnvFilter::new("info"),
    );
    let result = load(memory, legacy).with_subscriber(subscriber).await;
    let log = String::from_utf8(kept.0.lock().unwrap().clone()).unwrap();
    (result, log)
}

#[tokio::test]
async fn feature_registry_selects_instance_before_legacy_before_default_and_logs_its_source() {
    for (instance, legacy, expected, source) in [
        (None, None, "teachings", "default"),
        (
            None,
            Some("referring_to"),
            "referring_to,teachings",
            "JOJOBOT_SHADOW",
        ),
        (
            Some("referring_to"),
            Some("missing_family"),
            "referring_to,teachings",
            "instance",
        ),
        (Some("off"), Some("referring_to"), "off", "instance"),
    ] {
        let memory = match instance {
            Some(value) => held(&[("feature_shadow", value)]).await,
            None => Arc::new(InMemoryMemory::booted()),
        };
        let (folded, log) = logged(memory, legacy).await;
        let counts = folded.unwrap().shadow_counts();
        let families: Vec<_> = counts.iter().map(|c| c.family.name()).collect();
        let wanted: Vec<&str> = if expected == "off" {
            vec![]
        } else {
            expected.split(',').collect()
        };
        assert_eq!(families, wanted);
        assert_eq!(
            log.lines()
                .filter(|line| line
                    .split_whitespace()
                    .any(|word| word == "flag=\"shadow\""))
                .count(),
            1,
            "{log}"
        );
        for token in [
            format!("state={expected}"),
            format!("source=\"{source}\""),
            "takes_effect=\"boot\"".into(),
            format!("deprecated={}", source == "JOJOBOT_SHADOW"),
        ] {
            assert!(
                log.split_whitespace().any(|word| word == token),
                "{token}: {log}"
            );
        }
    }
}

#[tokio::test]
async fn feature_registry_ignores_unknown_keys_and_disables_unknown_families() {
    let memory = held(&[
        ("feature_shadow", "referring_to"),
        ("feature_future", "missing_family"),
    ])
    .await;
    let (folded, log) = logged(memory, None).await;
    assert_eq!(folded.unwrap().shadow_counts().len(), 2);
    assert!(
        log.lines().any(|line| line.contains("WARN")
            && line.contains("key=\"feature_future\"")
            && !line.contains("feature_shadow")),
        "{log}"
    );
    for instance in [true, false] {
        let memory = if instance {
            held(&[("feature_shadow", "missing_family")]).await
        } else {
            Arc::new(InMemoryMemory::booted())
        };
        let (result, log) = logged(memory, (!instance).then_some("missing_family")).await;
        let families: Vec<_> = result
            .expect("an unknown family keeps booting")
            .shadow_counts()
            .iter()
            .map(|c| c.family.name())
            .collect();
        assert_eq!(families, ["teachings"], "the default shadow stays on");
        let warning = log
            .lines()
            .find(|line| line.contains("WARN"))
            .expect("invalid flag warning");
        let source = if instance {
            "topic:instance"
        } else {
            "JOJOBOT_SHADOW"
        };
        for token in ["missing_family", "key=\"feature_shadow\"", source] {
            assert!(warning.contains(token), "{token}: {log}");
        }
        assert!(
            log.lines().any(|line| line.contains("state=teachings")),
            "{log}"
        );
    }
}

#[tokio::test]
async fn feature_registry_skips_missing_fields_and_logs_read_failures_with_shadow_off() {
    for exists in [false, true] {
        let memory = if exists {
            held(&[("feature_shadow", "referring_to")]).await
        } else {
            Arc::new(InMemoryMemory::booted())
        };
        let counted = Arc::new(jojobot_adapters::counted::Counted::new(memory));
        counted.fail_fields_of(Some("topic:instance"));
        let (folded, log) = logged(counted.clone(), Some("referring_to")).await;
        // No record: the legacy value and the teaching ledger. A record that cannot
        // be read: the default alone.
        assert_eq!(
            folded.unwrap().shadow_counts().len(),
            if exists { 1 } else { 2 },
            "{log}"
        );
        assert_eq!(
            counted.seen().get("fields").copied().unwrap_or(0),
            usize::from(exists)
        );
        if exists {
            assert!(
                log.lines().any(|line| line.contains("WARN")
                    && line.contains("key=\"feature_shadow\"")
                    && line.contains("source=\"topic:instance\"")),
                "{log}"
            );
        }
        counted.set_unreachable(true);
        let (folded, log) = logged(counted, Some("referring_to")).await;
        assert_eq!(folded.unwrap().shadow_counts().len(), 1, "{log}");
        assert!(
            log.lines()
                .any(|line| line.contains("WARN") && line.contains("source=\"topic:instance\"")),
            "{log}"
        );
    }
}

#[tokio::test]
async fn feature_registry_reaches_assembly_and_reads_the_legacy_environment() {
    if std::env::var("FEATURE_REGISTRY_CHILD").is_err() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "features::tests::feature_registry_reaches_assembly_and_reads_the_legacy_environment", "--nocapture"])
            .env("FEATURE_REGISTRY_CHILD", "1").env(LEGACY_ENV, "referring_to")
            .output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            output.status.success(),
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains("1 passed; 0 failed"),
            "the child must run its case: {stdout}"
        );
        return;
    }
    for (setting, expected) in [(None, 2), (Some("off"), 0), (Some("referring_to"), 2)] {
        let memory = if let Some(value) = setting {
            held(&[("feature_shadow", value)]).await
        } else {
            Arc::new(InMemoryMemory::booted())
        };
        let (folded, _) = crate::wiring::assemble_memory_with(memory, None)
            .await
            .unwrap();
        let counts = folded.shadow_counts();
        assert_eq!(counts.len(), expected);
        assert!(
            counts
                .iter()
                .all(|c| matches!(c.family.name(), "referring_to" | "teachings"))
        );
    }
    let (folded, _) = crate::wiring::assemble_memory_with(held(&[]).await, None)
        .await
        .unwrap();
    let counts = folded.shadow_counts();
    assert_eq!(
        counts.len(),
        1,
        "a present instance with no flag uses the legacy value"
    );
    assert!(
        counts
            .iter()
            .all(|count| count.family.name() == "referring_to")
    );
}

/// **A boot with no instance record keeps the legacy teaching rows and compares
/// them** (the PM's ruling on the default). The registry is read with no instance
/// record and no legacy environment, the ledger is made by the function the boot
/// calls, and then a contact, a repeat contact, a second run's contact and a clear
/// go through the port. The legacy table follows each write, and the shadow
/// compared each contact and found no disagreement. The registry cases above
/// assert family names; this one asserts what the family does.
#[tokio::test]
async fn a_default_boot_keeps_and_compares_the_legacy_teaching_rows() {
    use jojobot_adapters::dolt::Dolt;
    use jojobot_adapters::dolt::teaching::DoltTeachings;
    use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
    use jojobot_domain::session::Sid;
    use jojobot_domain::teaching::Teachings;

    let scratch = Scratch::new("default-shadow-rows");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    let at = |second: i64| jiff::Timestamp::from_second(1_700_000_000 + second).unwrap();

    let (folded, _) = logged(Arc::new(InMemoryMemory::booted()), None).await;
    let folded = folded.unwrap();
    let ledger = jojobot_adapters::teaching_log::serving(&pool, folded.shadow(), folded.writer())
        .await
        .expect("the ledger opens");
    assert!(
        ledger
            .first_contact(&Sid("homer".into()), "claims", at(1))
            .await
            .unwrap()
    );
    assert!(
        !ledger
            .first_contact(&Sid("homer".into()), "claims", at(2))
            .await
            .unwrap()
    );
    assert!(
        ledger
            .first_contact(&Sid("marge".into()), "mail", at(3))
            .await
            .unwrap()
    );
    ledger.forget(&Sid("marge".into())).await.unwrap();

    let rows: Vec<(String, String)> = DoltTeachings::open(pool.clone())
        .marks()
        .await
        .unwrap()
        .into_iter()
        .map(|mark| (mark.sid.0, mark.domain))
        .collect();
    assert_eq!(
        rows,
        [("homer".to_string(), "claims".to_string())],
        "the legacy rows follow the log's writes"
    );
    let counts = folded.shadow_counts();
    assert_eq!(counts.len(), 1);
    assert_eq!(counts[0].family.name(), "teachings");
    assert_eq!(counts[0].reads, 3, "each contact was compared");
    assert_eq!(counts[0].disagreements, 0);
    drop(ledger);
    server.stop().await;
}
