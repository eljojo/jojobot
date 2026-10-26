//! **How the fake follows its own merges**: where a claim address went, where a
//! thing ended up and the names folded into an entity.

use super::*;

/// **An entity as a receipt serves it**: with the names of what was folded into
/// it, as a listing of the roster serves them. The stored row carries none, so a
/// receipt built from the row alone would disagree with the listing.
pub(super) fn naming_folded_in(rows: &[Entity], entity: &mut Entity) {
    let mut listed = rows.to_vec();
    crate::memory::with_folded_names(&mut listed)
        .expect("the fake merges nothing into itself, so its merges never loop");
    if let Some(row) = listed.into_iter().find(|row| row.id == entity.id) {
        entity.folded_names = row.folded_names;
    }
}

/// **Where a thing ended up after every merge**, through a survivor folded in its
/// turn.
pub(super) fn ended_at(merges: &[StoredMerge], thing: EntityId) -> EntityId {
    follow_thing(thing, crate::memory::claim_walk::survivor_in(merges))
        .expect("the fake merges nothing into itself, so its merges never loop")
}

impl crate::memory::merge::ServedWrite for StoredWrite {
    fn key(&self) -> &str {
        &self.key
    }
    fn ordinal(&self) -> i64 {
        self.ordinal as i64
    }
    fn written_at(&self) -> Option<jiff::Timestamp> {
        self.written_at
    }
    fn fact(&self) -> &str {
        self.fact.as_str()
    }
    fn place(&mut self, ordinal: i64, fact: Option<&str>) {
        self.ordinal = ordinal as u64;
        if let Some(fact) = fact {
            self.fact = FactId(fact.to_string());
        }
    }
}

/// Decode the fake's merge accounts and keep unmapped writes at their old home.
/// The domain chooses the order and the ids of every mapped field write.
pub(super) fn merge_writes(writes: &mut Vec<StoredWrite>, merges: &[StoredMerge]) {
    for merge in merges {
        let claims = merge
            .claims
            .iter()
            .map(|(was, now)| (was.as_str().to_string(), now.as_str().to_string()))
            .collect();
        let (mut ours, mut theirs) = (Vec::new(), Vec::new());
        let mut rest = Vec::new();
        for write in std::mem::take(writes) {
            if write.entity == merge.survivor {
                ours.push(write);
            } else if write.entity == merge.folded
                && merge.claims.iter().any(|(was, _)| *was == write.fact)
            {
                theirs.push(write);
            } else {
                rest.push(write);
            }
        }
        let merged = crate::memory::merge::merge_in(ours, theirs, &claims);
        rest.extend(merged.into_iter().map(|mut write| {
            write.entity = merge.survivor.clone();
            write
        }));
        *writes = rest;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    #[tokio::test]
    async fn retained_fold_fields_keep_clears_and_class_membership() {
        let store = InMemoryMemory::booted();
        let home = EntityId("topic:rules-springfield".into());
        store
            .add_entity(NewEntity::new(home.clone(), "Retained fields", "fixture"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
        let mut new = NewFact::about(home.clone(), "kept field history", date(2026, 6, 1));
        new.fields = ["cleared", "set", "own", "legacy", "revived"]
            .into_iter()
            .map(|key| (key.to_string(), "old".to_string()))
            .collect();
        new.sets = ["cleared", "set", "revived"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let fact = store
            .capture(new)
            .await
            .expect("capture")
            .written()
            .expect("not blocked");
        // A write from before classes were recorded reaches as Legacy.
        for write in store.writes.lock().expect("writes").iter_mut() {
            if write.key == "legacy" {
                write.class = crate::memory::WriteClass::Legacy;
            }
        }
        let caller = EntityId("bot:sigma".into());
        store
            .update_fact(
                &fact.address(),
                FactPatch {
                    clear_fields: vec!["cleared".into(), "revived".into()],
                    ..Default::default()
                },
                &caller,
            )
            .await
            .expect("clear")
            .written()
            .expect("not blocked");
        store
            .update_fact(
                &fact.address(),
                FactPatch {
                    fields: [("revived".into(), "new".into())].into_iter().collect(),
                    ..Default::default()
                },
                &caller,
            )
            .await
            .expect("rewrite")
            .written()
            .expect("not blocked");
        let read = store
            .recall(&home)
            .await
            .expect("recall")
            .into_iter()
            .find(|read| read.id == fact.id)
            .expect("retained claim");
        assert_eq!(
            read.fields,
            [
                ("set", "old"),
                ("own", "old"),
                ("legacy", "old"),
                ("revived", "new")
            ]
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
        );
        assert_eq!(read.sets, ["set".to_string()].into_iter().collect());
        assert_eq!(
            read.reaching,
            ["set".to_string(), "legacy".to_string()]
                .into_iter()
                .collect()
        );
        let history = store.history(&home, "cleared").await.expect("history");
        assert_eq!(
            history
                .iter()
                .map(|write| write.value.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("old"), None],
            "a clear keeps the old write and its own tombstone"
        );
    }

    /// Retained rows keep their original homes and ordinals. Serving the merge
    /// must still preserve each side's order and put the survivor last on ties.
    #[tokio::test]
    async fn retained_fold_history_keeps_the_winner_and_each_sides_order() {
        for (days, unstamped, expected) in [
            (
                [3, 3, 3, 3],
                false,
                ["their-first", "their-last", "our-first", "our-last"],
            ),
            (
                [3, 3, 3, 3],
                true,
                ["their-first", "their-last", "our-first", "our-last"],
            ),
            (
                [5, 1, 3, 4],
                false,
                ["their-first", "their-last", "our-first", "our-last"],
            ),
            (
                [2, 4, 1, 3],
                false,
                ["their-first", "our-first", "their-last", "our-last"],
            ),
        ] {
            let store = InMemoryMemory::booted();
            let survivor = EntityId("topic:rules-springfield".into());
            let folded = EntityId("topic:rules-shelbyville".into());
            for (id, name) in [
                (&survivor, "Kept destination"),
                (&folded, "Retained source"),
            ] {
                store
                    .add_entity(NewEntity::new(id.clone(), name, "fixture"))
                    .await
                    .expect("add")
                    .written()
                    .expect("not blocked");
            }
            for (home, value) in [
                (&survivor, "our-first"),
                (&survivor, "our-last"),
                (&folded, "their-first"),
                (&folded, "their-last"),
            ] {
                let mut fact = NewFact::about(home.clone(), value, date(2026, 6, 1));
                fact.fields.insert("tone".into(), value.into());
                fact.sets.insert("tone".into());
                store
                    .capture(fact)
                    .await
                    .expect("capture")
                    .written()
                    .expect("not blocked");
            }
            // Retained rows have exact moments, including NULL on older writes.
            // Resetting a stated clock does not make ties: it still advances.
            for (write, day) in store
                .writes
                .lock()
                .expect("writes")
                .iter_mut()
                .filter(|write| write.key == "tone")
                .zip(days)
            {
                write.written_at = if unstamped {
                    None
                } else {
                    Some(
                        format!("2026-06-{day:02}T00:00:00Z")
                            .parse()
                            .expect("stamp"),
                    )
                };
            }
            let retained = || {
                store
                    .writes
                    .lock()
                    .expect("writes")
                    .iter()
                    .filter(|write| write.key == "tone")
                    .map(|write| {
                        (
                            write.entity.clone(),
                            write.ordinal,
                            write.value.clone(),
                            write.fact.clone(),
                            write.class,
                            write.written_at,
                            write.links.clone(),
                        )
                    })
                    .collect::<Vec<_>>()
            };
            let before = retained();
            store
                .merge(
                    &folded,
                    &survivor,
                    None,
                    date(2026, 6, 6),
                    &EntityId("bot:sigma".into()),
                    None,
                )
                .await
                .expect("merge");
            assert_eq!(
                retained(),
                before,
                "a logical merge must not rewrite retained rows"
            );
            let history = store.history(&survivor, "tone").await.expect("history");
            let values: Vec<_> = history
                .iter()
                .map(|write| write.value.as_deref().expect("value"))
                .collect();
            assert_eq!(values, expected, "days={days:?}, unstamped={unstamped}");
            assert_eq!(
                store
                    .fields(&survivor)
                    .await
                    .expect("fields")
                    .get("tone")
                    .map(String::as_str),
                Some("our-last")
            );
        }
    }
}
