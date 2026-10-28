//! The instance's boot-time feature registry. `feature_shadow` is off by default;
//! named projection families compare store and image without changing the served
//! answer. The teaching pilot retires it. Values are read before the fold exists.

use jojobot_adapters::fold::Folded;
use jojobot_domain::memory::{EntityId, EntityKind, INSTANCE_RECORD, Memory};
use std::sync::Arc;

const KEY: &str = "feature_shadow";
const LEGACY_ENV: &str = "JOJOBOT_SHADOW";
const RETIRES: &str = "q-thin-4-teaching-pilot";

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
                "feature settings unreadable; starting with shadow off");
            [(KEY.to_string(), "off".to_string())].into()
        }),
    };
    for key in fields.keys().filter(|key| key.starts_with("feature_")) {
        if key != KEY {
            tracing::warn!(key, "unknown feature key ignored at boot");
        }
    }
    let (value, source) = fields
        .get(KEY)
        .map(|value| (value.as_str(), "instance"))
        .or_else(|| legacy.map(|value| (value, LEGACY_ENV)))
        .unwrap_or(("off", "default"));
    let spec = if value.trim() == "off" { "" } else { value };
    let folded = match Folded::new(memory.clone()).shadowing(spec) {
        Ok(folded) => folded,
        Err(error) => {
            let source = if source == "instance" {
                instance.as_str()
            } else {
                source
            };
            tracing::warn!(key = KEY, source, %error, "invalid shadow family; starting with shadow off");
            Folded::new(memory)
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
        deprecated = source == LEGACY_ENV, retires = RETIRES, "feature flag at boot");
    Ok(folded)
}

#[cfg(test)]
mod tests;
