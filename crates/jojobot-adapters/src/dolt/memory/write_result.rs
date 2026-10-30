//! Compose a write result and its links from the same stored field values.

use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct ServedFields {
    pub(super) values: BTreeMap<String, String>,
    pub(super) links: BTreeMap<String, Vec<EntityId>>,
}

impl DoltMemory {
    pub(super) async fn read_write_fields(
        &self,
        tx: &mut Transaction<'_, MySql>,
        record: &Fact,
    ) -> Result<(ServedFields, BTreeSet<String>, BTreeSet<String>), MemoryError> {
        let (fields, sets, reaching) =
            Self::fields_and_sets_of(tx, &record.home, &record.id).await?;
        Ok((self.serve_write_fields(tx, fields).await?, sets, reaching))
    }

    pub(super) async fn serve_written_fields(
        &self,
        tx: &mut Transaction<'_, MySql>,
        writes: &[(String, Option<String>)],
    ) -> Result<ServedFields, MemoryError> {
        let fields = writes
            .iter()
            .filter_map(|(key, value)| value.as_ref().map(|value| (key.clone(), value.clone())))
            .collect();
        self.serve_write_fields(tx, fields).await
    }

    pub(super) async fn serve_write_fields(
        &self,
        tx: &mut Transaction<'_, MySql>,
        mut fields: BTreeMap<String, String>,
    ) -> Result<ServedFields, MemoryError> {
        let mut asked = assembly::Asked::default();
        let links = self.marked_links_asked(tx, &mut asked, &fields).await?;
        self.compose_reference_fields_asked(tx, &mut asked, &mut fields)
            .await?;
        Ok(ServedFields {
            values: fields,
            links,
        })
    }
}
