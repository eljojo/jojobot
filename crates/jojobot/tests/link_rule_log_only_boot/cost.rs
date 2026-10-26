use super::*;
use jojobot_adapters::provisioned::overlay_contract::shipped_record;
use jojobot_domain::memory::owned::Provisions;
use tracing::instrument::WithSubscriber;
use tracing_subscriber::prelude::*;

#[derive(Clone, Default)]
struct Reads(std::sync::Arc<std::sync::Mutex<Vec<String>>>);

#[derive(Default)]
struct Query {
    summary: String,
    statement: String,
}

impl tracing::field::Visit for Query {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        match field.name() {
            "summary" => self.summary = value.into(),
            "db.statement" => self.statement = value.into(),
            _ => {}
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.record_str(field, &format!("{value:?}"));
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Reads {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        if event.metadata().target() != "sqlx::query" {
            return;
        }
        let mut query = Query::default();
        event.record(&mut query);
        let sql = if query.statement.trim().is_empty() {
            query.summary
        } else {
            query.statement
        };
        let sql = sql.split_whitespace().collect::<Vec<_>>().join(" ");
        if sql.starts_with("SELECT ") {
            self.0.lock().unwrap().push(sql);
        }
    }
}

impl Reads {
    async fn measure(memory: &DoltMemory) -> (usize, usize) {
        let reads = Self::default();
        let subscriber = tracing_subscriber::registry()
            .with(reads.clone())
            .with(tracing_subscriber::filter::LevelFilter::DEBUG);
        memory
            .link_rule_diagnostics()
            .with_subscriber(subscriber)
            .await
            .unwrap();
        let queries = reads.0.lock().unwrap();
        assert!(
            !queries.is_empty(),
            "the driver must report its actual reads"
        );
        let entity_reads = queries
            .iter()
            .filter(|sql| {
                sql.split_whitespace()
                    .collect::<Vec<_>>()
                    .windows(2)
                    .any(|words| words == ["FROM", "entity"])
            })
            .count();
        (queries.len(), entity_reads)
    }
}

async fn measurements() -> [(usize, usize); 3] {
    jojobot::wiring::set_link_rule_from(Some("log-only")).unwrap();
    let scratch = Scratch::new("link-rule-diagnostics-cost");
    let mut store = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&store, "diagnosticscost").await.unwrap();
    migrate::run(&pool).await.unwrap();
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory).await.unwrap();
    let holder = EntityId("thing:contract-links-holder".into());
    memory
        .add_entity(NewEntity::new(holder.clone(), "Holder", "contract-fixture"))
        .await
        .unwrap()
        .written()
        .unwrap();
    let small = memory
        .clone()
        .knowing(Provisions::new(vec![shipped_record("loops")]));
    let many = memory.knowing(Provisions::new(
        [
            "loops",
            "marked-things",
            "plain-listing",
            "matches-nothing",
            "marked-by-default",
            "marked-by-asking",
        ]
        .map(shipped_record)
        .into(),
    ));
    let one = Reads::measure(&small).await;
    let six = Reads::measure(&many).await;
    many.capture(memory::state(NewFact {
        fields: (0..32)
            .map(|n| (format!("background_{n}"), "only words".into()))
            .collect(),
        ..NewFact::about(holder, "background writes", date(2026, 8, 1))
    }))
    .await
    .unwrap()
    .written()
    .unwrap();
    let dense = Reads::measure(&many).await;
    pool.close().await;
    store.stop().await;
    [one, six, dense]
}

#[tokio::test]
async fn supplied_only_sql_budget_does_not_grow_with_records_or_write_rows() {
    let [one, six, dense] = measurements().await;
    assert_eq!(one.0, six.0, "reads must not follow supplied record count");
    assert_eq!(
        six.0, dense.0,
        "reads must not follow field-write row count"
    );
}

#[tokio::test]
async fn supplied_only_sql_budget_reads_the_entity_table_once() {
    for (_, entity_reads) in measurements().await {
        assert_eq!(entity_reads, 1, "one shared physical entity-row read");
    }
}
