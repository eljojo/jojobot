//! The instance's boot-time feature registry. `feature_shadow` names the projection
//! families that are computed two ways and compared without changing the served
//! answer. The teaching ledger is always one of them unless the flag says `off`:
//! the log serves it, and the shadow is what keeps the legacy rows current and
//! compared, so a return to the old path stays possible until 9.3.3 cuts it. The
//! teaching pilot (`q-thin-4-teaching-pilot`) retires the flag.
//! Values are read before the fold exists.

use jojobot_adapters::fold::Folded;
use jojobot_domain::memory::{EntityId, EntityKind, INSTANCE_RECORD, Memory};
use std::sync::Arc;

const KEY: &str = "feature_shadow";
const LEGACY_ENV: &str = "JOJOBOT_SHADOW";
/// The family that is shadowed whenever the flag does not say `off`.
const TEACHINGS: &str = "teachings";

/// The families a value names, with the teaching ledger added when it is absent.
fn with_teachings(value: &str) -> String {
    let mut families: Vec<&str> = value
        .split(',')
        .map(str::trim)
        .filter(|family| !family.is_empty())
        .collect();
    if !families.contains(&TEACHINGS) {
        families.push(TEACHINGS);
    }
    families.join(",")
}

pub(crate) async fn at_boot(memory: Arc<dyn Memory>) -> anyhow::Result<Folded> {
    let legacy = std::env::var(LEGACY_ENV).ok();
    load(memory, legacy.as_deref()).await
}

async fn load(memory: Arc<dyn Memory>, legacy: Option<&str>) -> anyhow::Result<Folded> {
    let instance = EntityId(INSTANCE_RECORD.into());
    // A fresh store is seeded later; listing avoids handle-history resolution on a miss.
    let fields = match memory.list_entities(Some(EntityKind::TOPIC)).await {
        Ok(topics) if !topics.iter().any(|topic| topic.id == instance) => Default::default(),
        result => match result {
            Ok(_) => memory.fields(&instance).await,
            Err(error) => Err(error),
        }
        .unwrap_or_else(|error| {
            tracing::warn!(key = KEY, source = INSTANCE_RECORD, %error,
                "feature settings unreadable; starting with the default shadow");
            [(KEY.to_string(), String::new())].into()
        }),
    };
    for key in fields.keys().filter(|key| key.starts_with("feature_")) {
        if key != KEY {
            tracing::warn!(key, "unknown feature key ignored at boot");
        }
    }
    let (value, source) = fields
        .get(KEY)
        .map(|value| (value.as_str(), INSTANCE_RECORD))
        .or_else(|| legacy.map(|value| (value, LEGACY_ENV)))
        .unwrap_or(("", "default"));
    let spec = if value.trim() == "off" {
        tracing::warn!(
            key = KEY,
            source,
            "the teaching shadow is off: the legacy teaching rows are no longer kept and \
             compared, so they go stale and a return to the old path is not possible"
        );
        String::new()
    } else {
        with_teachings(value)
    };
    let folded = match Folded::new(memory.clone()).shadowing(&spec) {
        Ok(folded) => folded,
        Err(error) => {
            tracing::warn!(key = KEY, source, %error, "invalid shadow family; starting with the default shadow");
            Folded::new(memory)
                .shadowing(TEACHINGS)
                .expect("the teaching ledger is a family")
        }
    };
    let state = folded
        .shadow_counts()
        .iter()
        .map(|c| c.family.name())
        .collect::<Vec<_>>()
        .join(",");
    let state = if state.is_empty() { "off" } else { &state };
    tracing::info!(flag = "shadow", key = KEY, state = %state, source, takes_effect = "boot",
        deprecated = source == LEGACY_ENV, "feature flag at boot");
    Ok(folded)
}

#[cfg(test)]
mod tests;
