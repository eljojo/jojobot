//! Maps with structured keys serialize as ordered pairs. Hash iteration cannot
//! affect a snapshot's bytes, and JSON object keys cannot carry these identities.

use std::collections::HashMap;
use std::hash::Hash;
use std::ops::{Deref, DerefMut};

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SnapshotMap<K: Eq + Hash, V>(HashMap<K, V>);

impl<K: Eq + Hash, V> Default for SnapshotMap<K, V> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

impl<K: Eq + Hash, V> Deref for SnapshotMap<K, V> {
    type Target = HashMap<K, V>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<K: Eq + Hash, V> DerefMut for SnapshotMap<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<K: Serialize + Ord + Hash, V: Serialize> Serialize for SnapshotMap<K, V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut entries: Vec<_> = self.0.iter().collect();
        entries.sort_by(|(left, _), (right, _)| left.cmp(right));
        entries.serialize(serializer)
    }
}

impl<'de, K: Deserialize<'de> + Eq + Hash, V: Deserialize<'de>> Deserialize<'de>
    for SnapshotMap<K, V>
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(
            Vec::<(K, V)>::deserialize(deserializer)?
                .into_iter()
                .collect(),
        ))
    }
}

impl<K: JsonSchema + Eq + Hash, V: JsonSchema> JsonSchema for SnapshotMap<K, V> {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        <Vec<(K, V)>>::schema_name()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        <Vec<(K, V)>>::json_schema(generator)
    }
}
