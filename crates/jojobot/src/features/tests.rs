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
                .filter(|line| line.contains("feature flag at boot"))
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
    let counted = Arc::new(jojobot_adapters::counted::Counted::new(Arc::new(
        InMemoryMemory::booted(),
    )));
    assert!(load(counted.clone(), None).await.is_ok());
    counted.fail_fields_of(Some("topic:instance"));
    assert!(
        load(counted, None).await.is_err(),
        "an unreadable record is not an absent setting"
    );
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
        log.contains("unknown feature key") && log.contains("feature_future"),
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
