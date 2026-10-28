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
        (None, None, "off", "default"),
        (None, Some("referring_to"), "referring_to", "JOJOBOT_SHADOW"),
        (
            Some("referring_to"),
            Some("missing_family"),
            "referring_to",
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
        assert_eq!(
            families,
            if expected == "off" {
                vec![]
            } else {
                vec!["referring_to"]
            }
        );
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
    assert_eq!(folded.unwrap().shadow_counts().len(), 1);
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
        assert!(
            result
                .expect("an unknown family keeps booting")
                .shadow_counts()
                .is_empty()
        );
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
        assert!(log.lines().any(|line| line.contains("state=off")), "{log}");
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
        assert_eq!(
            folded.unwrap().shadow_counts().len(),
            usize::from(!exists),
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
        assert!(folded.unwrap().shadow_counts().is_empty(), "{log}");
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
        assert!(std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "features::tests::feature_registry_reaches_assembly_and_reads_the_legacy_environment", "--nocapture"])
            .env("FEATURE_REGISTRY_CHILD", "1").env(LEGACY_ENV, "referring_to")
            .status().unwrap().success());
        return;
    }
    for (setting, expected) in [(None, 1), (Some("off"), 0), (Some("referring_to"), 1)] {
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
        assert!(counts.iter().all(|c| c.family.name() == "referring_to"));
    }
}
