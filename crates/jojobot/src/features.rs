//! The instance's boot-time feature registry. `feature_shadow` is off by default;
//! named projection families compare store and image without changing the served
//! answer. The teaching pilot retires it. Values are read before the fold exists.

use jojobot_adapters::fold::Folded;
use jojobot_domain::memory::{EntityId, Memory, MemoryError};
use std::sync::Arc;

const KEY: &str = "feature_shadow";
const LEGACY_ENV: &str = "JOJOBOT_SHADOW";
const RETIRES: &str = "q-thin-4-teaching-pilot";

pub(crate) async fn at_boot(memory: Arc<dyn Memory>) -> anyhow::Result<Folded> {
    let legacy = std::env::var(LEGACY_ENV).ok();
    load(memory, legacy.as_deref()).await
}

async fn load(memory: Arc<dyn Memory>, legacy: Option<&str>) -> anyhow::Result<Folded> {
    let instance = EntityId("topic:instance".into());
    let fields = match memory.fields(&instance).await {
        Ok(fields) => fields,
        // A fresh store gets its instance record in a later startup pass.
        Err(MemoryError::UnknownEntity { attempted, .. }) if attempted == instance.as_str() => {
            Default::default()
        }
        Err(error) => return Err(error.into()),
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
    let folded = Folded::new(memory)
        .shadowing(spec)
        .map_err(|e| anyhow::anyhow!("{KEY} ({source}): {e}"))?;
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
