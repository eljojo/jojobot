//! **A value left as text is told apart by the claim that holds it.** A handle
//! that an older pass left as text, and that names a thing now, is a warning
//! while the claim holding it stands. Once the claim is superseded or archived
//! the boot says nothing, and the stored value is never rewritten.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::{EntityId, FactPatch, FactStatus, Memory, NewEntity, NewFact};
use tracing::instrument::WithSubscriber;

#[derive(Clone, Default)]
struct Kept(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

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

async fn boot_log(memory: &DoltMemory) -> String {
    let kept = Kept::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(kept.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .finish();
    jojobot::wiring::field_links::boot_field_links(memory)
        .with_subscriber(subscriber)
        .await;
    String::from_utf8_lossy(&kept.0.lock().expect("log buffer")).into_owned()
}

async fn add(memory: &DoltMemory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    memory
        .add_entity(NewEntity::new(id.clone(), name, "fixture"))
        .await
        .expect("add")
        .written()
        .expect("nothing collides");
    id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_text_value_warns_only_while_its_claim_stands() {
    for state in ["held", "superseded", "archived"] {
        let label = format!("text-value-{state}");
        let scratch = Scratch::new(&label);
        let mut server = Dolt::start(&scratch.0, free_port())
            .await
            .expect("the store comes up");
        let pool = migrated_database(&server, &label)
            .await
            .expect("a database of its own");
        migrate::run(&pool).await.expect("the schema");
        let memory = DoltMemory::open(pool.clone());
        jojobot_domain::memory::kinds::seed(&memory)
            .await
            .expect("the kinds are seeded");
        add(&memory, "bot:beta", "Beta").await;
        let holder = add(&memory, "work:alpha-queue", "Alpha queue").await;
        memory.migrate_field_links().await.expect("lowering");

        let mut old = NewFact::about(holder.clone(), "the old label", date(2026, 9, 1));
        old.fields.insert("label".into(), "plain words".into());
        let fact = memory
            .capture(old)
            .await
            .expect("capture")
            .written()
            .expect("lands");
        let key: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
            .bind(holder.as_str())
            .fetch_one(&pool)
            .await
            .expect("the badge reads");
        sqlx::query(
            "UPDATE field_write SET value = 'bot:beta' WHERE entity = ? AND `key` = 'label'",
        )
        .bind(&key)
        .execute(&pool)
        .await
        .expect("the older pass's text");
        if state == "superseded" {
            let mut newer = NewFact::about(holder.clone(), "the new label", date(2026, 9, 2));
            newer.fields.insert("label".into(), "plain words".into());
            memory
                .capture(newer)
                .await
                .expect("capture")
                .written()
                .expect("lands");
        } else if state == "archived" {
            memory
                .update_fact(
                    &fact.address(),
                    FactPatch {
                        status: Some(FactStatus::Archived),
                        ..Default::default()
                    },
                    &holder,
                )
                .await
                .expect("archive")
                .written()
                .expect("lands");
        }

        let log = boot_log(&memory).await;
        let warnings: Vec<_> = log
            .lines()
            .filter(|line| line.contains("FIELD VALUE LEFT AS TEXT"))
            .collect();
        if state == "held" {
            assert_eq!(warnings.len(), 1, "{state}: {log}");
            assert!(
                warnings[0].split_whitespace().any(|field| field == "WARN"),
                "{log}"
            );
        } else {
            assert!(warnings.is_empty(), "{state}: {log}");
            assert!(
                !log.contains("WARN") && !log.contains("ERROR"),
                "{state}: {log}"
            );
            assert!(
                !log.contains("bot:beta"),
                "historical values are not named: {log}"
            );
        }
        let raw: String = sqlx::query_scalar(
            "SELECT value FROM field_write WHERE entity = ? AND `key` = 'label' \
             ORDER BY ordinal LIMIT 1",
        )
        .bind(&key)
        .fetch_one(&pool)
        .await
        .expect("the raw value reads");
        assert_eq!(raw, "bot:beta", "{state}: the value is never rewritten");
        pool.close().await;
        server.stop().await;
    }
}
