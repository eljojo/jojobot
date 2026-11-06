//! **The claims State the fake holds.** A write of the fake makes the events the
//! dolt writers make, through the domain's functions, and applies them here, so
//! the State answers what the fake answers for the thing it touched (rule
//! 9.3.3-d (a) and (b)). This file is the foundation only: the State, the one
//! call that applies events to it, and the key a handle stands for. The writers
//! that call it are the slices of 9.3.3-d.

use super::*;
use crate::log::claims::emit::{self, Origin, Served};
use crate::log::claims::import::record_id;
use crate::log::claims::{ClaimEvent, KeyId, MovedRecord, MovedWrite, State};
use crate::log::{Event, ObjectId, RecordId};
use crate::memory::write_origin::{WriteOrigin, written_by};

impl InMemoryMemory {
    /// **Apply the events of one write to the claims State**, in the order they
    /// were made.
    pub(super) fn emit(&self, events: impl IntoIterator<Item = Event<ClaimEvent>>) {
        let mut state = self.claims.lock().expect("fake mutex poisoned");
        for event in events {
            state.apply(&event);
        }
    }

    /// **The permanent key a handle stands for**: the key the fake files the thing
    /// under, as the dolt store's badge is. A name that stands for none keeps its
    /// spelling, as the import's reader keeps it.
    pub(super) fn object_key(&self, id: &EntityId) -> ObjectId {
        ObjectId(
            self.storage_key(id)
                .map_or_else(|| id.as_str().to_string(), |key| key.as_str().to_string()),
        )
    }

    /// **The record a claim is held in**, named by the row it is stored in: an
    /// address names a claim by the home and id it is served under, and a merge
    /// leaves a claim where it was written.
    fn stored_record(&self, served: &FactAddress) -> RecordId {
        let at = self.stored_address(served);
        record_id(&self.object_key(&at.home), at.local.as_str())
    }

    /// **Add the event of one claim write to the State**: the first write of a
    /// claim creates its record and every later one revises it. `fact` is the
    /// claim as it is served; the record is named by the row it is stored in.
    pub(super) fn emit_claim_write(&self, fact: &Fact, write: &ClaimWrite) {
        let record = self.stored_record(&FactAddress::new(fact.home.clone(), fact.id.clone()));
        let claim = emit::claim_body(
            fact,
            Served {
                home: self.object_key(&fact.home),
                label: fact.id.as_str().to_string(),
                edge: fact
                    .edge
                    .as_ref()
                    .map(|edge| (edge.shape.to_string(), self.object_key(&edge.object))),
                derived_from: fact
                    .derived_from
                    .as_ref()
                    .map(|from| self.stored_record(from)),
                refs: fact
                    .refs
                    .iter()
                    .map(|named| self.object_key(named))
                    .collect(),
                stands_for: fact
                    .stands_for
                    .iter()
                    .map(|named| self.stored_record(named))
                    .collect(),
            },
        );
        let run_bot = write
            .session
            .as_deref()
            .and_then(|run| self.run_bots.as_ref()?.bot_of_run(run));
        let by = written_by(
            Some(WriteOrigin {
                bot: write.bot.clone(),
                run: write.session.clone(),
                run_bot,
                app_stamp: fact.app_stamp.clone(),
            }),
            fact.app_stamp.as_ref(),
        );
        let origin = Origin {
            actor: by.writer,
            run: write.session.clone(),
            written_at: write.written_at,
        };
        self.emit([emit::claim_write(
            record,
            write.ordinal as i64,
            claim,
            &origin,
        )]);
    }

    /// **Add the event of one key write to the State.** `home` is the thing the
    /// write belongs to as it is served, `at` the row the claim is stored in, and
    /// `ordinal` the place the write holds among the thing's writes of the key.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_key_write(
        &self,
        home: &EntityId,
        at: &FactAddress,
        key: &str,
        ordinal: u64,
        class: crate::memory::WriteClass,
        value: &Option<String>,
        links: &[String],
        moment: jiff::Timestamp,
        origin: &Option<WriteOrigin>,
    ) {
        let by = written_by(origin.clone(), None);
        let origin = Origin {
            actor: by.writer,
            run: origin.as_ref().and_then(|held| held.run.clone()),
            written_at: Some(moment),
        };
        self.emit([emit::key_write(
            record_id(&self.object_key(&at.home), at.local.as_str()),
            self.object_key(home),
            key,
            ordinal,
            class,
            value,
            links
                .iter()
                .map(|named| self.object_key(&EntityId(named.clone())))
                .collect(),
            &origin,
        )]);
    }

    /// **Add the event of a merge to the State**: the claims it gave new ids, and
    /// the place each key write takes among the survivor's, by the domain's one
    /// rule over the writes as they are served before the merge. Called before
    /// the merge is recorded.
    pub(super) fn emit_merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        moved: &[(FactId, FactId)],
        caller: &EntityId,
        session: Option<&str>,
    ) {
        let claims: std::collections::HashMap<&str, &str> = moved
            .iter()
            .map(|(was, now)| (was.as_str(), now.as_str()))
            .collect();
        let records = moved
            .iter()
            .map(|(was, now)| MovedRecord {
                record: self.stored_record(&FactAddress::new(folded.clone(), was.clone())),
                label: now.as_str().to_string(),
            })
            .collect();
        let (theirs, ours) = self.with_writes(|served| {
            let on = |key: &EntityId| -> Vec<StoredWrite> {
                served
                    .iter()
                    .filter(|w| &w.entity == key)
                    .cloned()
                    .collect()
            };
            (on(folded), on(survivor))
        });
        let writes = emit::placements(theirs, ours, &claims)
            .into_iter()
            .map(|placed| {
                let home = if placed.side == crate::memory::merge::Side::Folded {
                    folded
                } else {
                    survivor
                };
                MovedWrite {
                    record: self
                        .stored_record(&FactAddress::new(home.clone(), placed.write.fact.clone())),
                    key: KeyId(placed.write.key.clone()),
                    from_ordinal: placed.write.ordinal,
                    ordinal: placed.ordinal as u64,
                }
            })
            .collect();
        let actor =
            super::super::super::authorship::stored_writer(caller, |named| self.storage_key(named));
        let origin = Origin {
            actor: Some(crate::actor::Actor::Bot(actor)),
            run: session.map(str::to_string),
            written_at: Some(self.clock().now()),
        };
        self.emit([emit::merge(
            self.object_key(folded),
            self.object_key(survivor),
            records,
            writes,
            &origin,
        )]);
    }

    /// **The bot a write names**: the caller, when the caller is a bot. A write
    /// with no session still says who wrote it.
    pub(super) fn writing_bot(&self, caller: &EntityId) -> Option<EntityId> {
        (caller.kind() == Some(crate::memory::EntityKind::BOT)).then(|| caller.clone())
    }

    /// **Read the claims State**, as the seam's `read_claims` lets a case read it.
    pub fn read_claims<T>(&self, read: impl FnOnce(&State) -> T) -> T {
        read(&self.claims.lock().expect("fake mutex poisoned"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::claims::{KindId, ObjectBody};
    use crate::memory::Provenance;

    /// **Events applied to the fake are what its State answers.**
    #[test]
    fn events_applied_to_the_fake_are_what_its_state_answers() {
        let fake = InMemoryMemory::new();
        let created = Event {
            event_kind: String::new(),
            meaning_version: 1,
            actor: None,
            run: None,
            session_card: None,
            request: None,
            part: None,
            occurred_at: None,
            written_at: None,
            imported_source_time: None,
            payload: ClaimEvent::ObjectCreated {
                object: ObjectId("k1".into()),
                body: ObjectBody {
                    kind: KindId("person".into()),
                    parent: None,
                    name: "person:homer".into(),
                    source: "fixture".into(),
                    aliases: Vec::new(),
                    boot: None,
                    metadata: Default::default(),
                    archived: false,
                    bound_to: None,
                },
            },
        };
        assert!(fake.read_claims(|state| state.object(&ObjectId("k1".into())).is_none()));
        fake.emit([created]);
        assert!(fake.read_claims(|state| state.object(&ObjectId("k1".into())).is_some()));
    }

    /// **A handle stands for the key the fake files its thing under**, and a name
    /// no thing wears keeps its spelling.
    #[tokio::test]
    async fn a_handle_stands_for_the_key_the_fake_files_its_thing_under() {
        let fake = InMemoryMemory::booted();
        let homer = EntityId("person:homer".into());
        fake.add_entity(NewEntity::new(homer.clone(), "Homer", "fixture"))
            .await
            .expect("add")
            .written()
            .expect("written");
        let key = fake.storage_key(&homer).expect("the fake files the thing");
        assert_eq!(fake.object_key(&homer), ObjectId(key.as_str().to_string()));
        let unknown = EntityId("person:marge".into());
        assert_eq!(fake.object_key(&unknown), ObjectId("person:marge".into()));
    }

    /// **The claims State answers what the fake answers for the thing a claim
    /// write touched**: values, the moment each key's value was taken in, and who
    /// backs each key, after a capture and a rewrite that set and change keys.
    #[tokio::test]
    async fn a_claim_written_twice_is_held_by_the_state_as_the_fake_holds_it() {
        let fake = InMemoryMemory::booted();
        let homer = EntityId("person:homer".into());
        fake.add_entity(NewEntity::new(homer.clone(), "Homer", "fixture"))
            .await
            .expect("add")
            .written()
            .expect("written");
        let mut fact = NewFact::about(homer.clone(), "eats donuts", jiff::civil::date(2026, 9, 1));
        fact.provenance = Provenance::Inference;
        fact.fields = [("mood", "happy"), ("hair", "none")]
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        fact.sets = fact.fields.keys().cloned().collect();
        let made = fake
            .capture(fact)
            .await
            .expect("capture")
            .written()
            .expect("not blocked")
            .address();
        fake.update_fact(
            &made,
            FactPatch {
                content: Some("eats donuts daily".into()),
                fields: [("mood".to_string(), "grumpy".to_string())].into(),
                sets: ["mood".to_string()].into(),
                provenance: Some(Provenance::Inference),
                ..Default::default()
            },
            &EntityId("bot:sigma".into()),
        )
        .await
        .expect("the claim is rewritten")
        .written()
        .expect("not blocked");

        let key = fake.object_key(&homer);
        let declared = fake.declared_types().await.expect("declarations");
        let (held, ages, backing) = fake.read_claims(|state| {
            (
                state.fields(&key, &declared),
                state.held_since(&key, &declared),
                state.backing(&key, &declared),
            )
        });
        assert_eq!(held, fake.fields(&homer).await.expect("the fake's values"));
        assert_eq!(held["mood"], "grumpy");
        assert_eq!(
            ages,
            fake.field_ages(&homer).await.expect("the fake's ages").keys
        );
        assert_eq!(
            backing,
            fake.backing(&homer).await.expect("the fake's backing")
        );
    }

    /// **A merge in the State is the merge in the fake**: the folded thing's
    /// writes take their places among the survivor's, the survivor's own writes
    /// move on, and a write after the merge to a claim of either side lands where
    /// the fake serves it.
    #[tokio::test]
    async fn a_merge_and_the_writes_after_it_are_held_by_the_state_as_the_fake_holds_them() {
        let fake = InMemoryMemory::booted();
        let homer = EntityId("person:homer".into());
        let marge = EntityId("person:marge".into());
        for (who, name) in [(&homer, "Homer"), (&marge, "Marge")] {
            fake.add_entity(NewEntity::new(who.clone(), name, "fixture"))
                .await
                .expect("add")
                .written()
                .expect("written");
        }
        let said =
            |about: EntityId, what: &'static str, kept: &'static [(&'static str, &'static str)]| {
                let fake = &fake;
                async move {
                    let mut fact = NewFact::about(about, what, jiff::civil::date(2026, 9, 1));
                    fact.provenance = Provenance::Inference;
                    fact.fields = kept
                        .iter()
                        .map(|(key, value)| (key.to_string(), value.to_string()))
                        .collect();
                    fact.sets = fact.fields.keys().cloned().collect();
                    fake.capture(fact)
                        .await
                        .expect("capture")
                        .written()
                        .expect("not blocked")
                        .address()
                }
            };
        // Marge's write falls between Homer's two, so the merge places it
        // between them and moves Homer's later write on.
        let _happy = said(homer.clone(), "eats donuts", &[("mood", "happy")]).await;
        let calm = said(
            marge.clone(),
            "keeps the house",
            &[("mood", "calm"), ("hat", "red")],
        )
        .await;
        let grumpy = said(homer.clone(), "is grumpy", &[("mood", "grumpy")]).await;
        fake.merge(
            &marge,
            &homer,
            Some("filed twice"),
            jiff::civil::date(2026, 9, 3),
            &EntityId("bot:sigma".into()),
            Some("run-a"),
        )
        .await
        .expect("the fold lands");

        let agrees = || async {
            let key = fake.object_key(&homer);
            let declared = fake.declared_types().await.expect("declarations");
            let (held, ages, backing) = fake.read_claims(|state| {
                (
                    state.fields(&key, &declared),
                    state.held_since(&key, &declared),
                    state.backing(&key, &declared),
                )
            });
            assert_eq!(held, fake.fields(&homer).await.expect("the fake's values"));
            assert_eq!(
                ages,
                fake.field_ages(&homer).await.expect("the fake's ages").keys
            );
            assert_eq!(
                backing,
                fake.backing(&homer).await.expect("the fake's backing")
            );
            held
        };
        let held = agrees().await;
        assert_eq!(held["mood"], "grumpy");
        assert_eq!(
            held["hat"], "red",
            "the survivor holds a key only the fold held"
        );

        // Both sides' claims are written again after the merge; the moved claim is
        // named by the id the merge gave it.
        let moved = fake
            .merge_events(None)
            .await
            .expect("the merge events")
            .into_iter()
            .flat_map(|event| event.claims)
            .find(|(was, _)| *was == calm.local)
            .map(|(_, now)| now)
            .expect("the merge mapped the claim");
        let rewrite = |at: FactAddress, mood: &'static str| {
            let fake = &fake;
            async move {
                fake.update_fact(
                    &at,
                    FactPatch {
                        fields: [("mood".to_string(), mood.to_string())].into(),
                        sets: ["mood".to_string()].into(),
                        provenance: Some(Provenance::Inference),
                        ..Default::default()
                    },
                    &EntityId("bot:sigma".into()),
                )
                .await
                .expect("the claim is rewritten")
                .written()
                .expect("not blocked");
            }
        };
        rewrite(grumpy, "restless").await;
        rewrite(FactAddress::new(homer.clone(), moved), "serene").await;
        let held = agrees().await;
        assert_eq!(held["mood"], "serene", "the newest write holds the key");
    }

    /// **A clear and a retraction in the State are what the fake holds**: a clear
    /// sent to a claim that does not hold the key leaves the value, one sent to the
    /// holder takes the key off and nothing older returns, and a retracted claim's
    /// writes stop counting.
    #[tokio::test]
    async fn a_clear_and_a_retraction_are_held_by_the_state_as_the_fake_holds_them() {
        let fake = InMemoryMemory::booted();
        let homer = EntityId("person:homer".into());
        fake.add_entity(NewEntity::new(homer.clone(), "Homer", "fixture"))
            .await
            .expect("add")
            .written()
            .expect("written");
        let said = |what: &'static str, mood: &'static str| {
            let (fake, homer) = (&fake, homer.clone());
            async move {
                let mut fact = NewFact::about(homer, what, jiff::civil::date(2026, 9, 1));
                fact.provenance = Provenance::Inference;
                fact.fields = [("mood".to_string(), mood.to_string())].into();
                fact.sets = fact.fields.keys().cloned().collect();
                fake.capture(fact)
                    .await
                    .expect("capture")
                    .written()
                    .expect("not blocked")
                    .address()
            }
        };
        let agrees = || async {
            let key = fake.object_key(&homer);
            let declared = fake.declared_types().await.expect("declarations");
            let (held, ages, backing) = fake.read_claims(|state| {
                (
                    state.fields(&key, &declared),
                    state.held_since(&key, &declared),
                    state.backing(&key, &declared),
                )
            });
            assert_eq!(held, fake.fields(&homer).await.expect("the fake's values"));
            assert_eq!(
                ages,
                fake.field_ages(&homer).await.expect("the fake's ages").keys
            );
            assert_eq!(
                backing,
                fake.backing(&homer).await.expect("the fake's backing")
            );
            held
        };
        let clear = |at: FactAddress| {
            let fake = &fake;
            async move {
                fake.update_fact(
                    &at,
                    FactPatch {
                        clear_fields: vec!["mood".to_string()],
                        provenance: Some(Provenance::Inference),
                        ..Default::default()
                    },
                    &EntityId("bot:sigma".into()),
                )
                .await
                .expect("the clear lands")
                .written()
                .expect("not blocked");
            }
        };
        let older = said("felt content", "content").await;
        let newer = said("felt queasy", "queasy").await;

        clear(older).await;
        assert_eq!(agrees().await["mood"], "queasy");
        clear(newer).await;
        assert!(!agrees().await.contains_key("mood"));

        // A retracted claim's writes stop counting.
        let retracted = said("felt giddy", "giddy").await;
        assert_eq!(agrees().await["mood"], "giddy");
        fake.retract(
            &retracted,
            Some("a mistake"),
            jiff::civil::date(2026, 9, 5),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect("the claim is retracted");
        assert!(!agrees().await.contains_key("mood"));
    }

    /// **The bot that wrote is the actor of every write of a claim on the fake**,
    /// with no session to say so: a capture, an update and a retraction each name
    /// their bot, and a record a write creates is first written by it.
    #[tokio::test]
    async fn the_bot_that_wrote_is_the_actor_of_every_claim_write_on_the_fake() {
        let fake = InMemoryMemory::booted();
        let homer = EntityId("person:homer".into());
        let sigma = EntityId("bot:sigma".into());
        fake.add_entity(NewEntity::new(homer.clone(), "Homer", "fixture"))
            .await
            .expect("add")
            .written()
            .expect("written");
        let mut fact = NewFact::about(homer.clone(), "felt giddy", jiff::civil::date(2026, 9, 1));
        fact.provenance = Provenance::Inference;
        fact.written_by_bot = Some(sigma.clone());
        let made = fake
            .capture(fact)
            .await
            .expect("capture")
            .written()
            .expect("not blocked")
            .address();
        fake.update_fact(
            &made,
            FactPatch {
                content: Some("felt giddy all day".into()),
                provenance: Some(Provenance::Inference),
                ..Default::default()
            },
            &sigma,
        )
        .await
        .expect("the claim is rewritten")
        .written()
        .expect("not blocked");
        fake.retract(
            &made,
            Some("a mistake"),
            jiff::civil::date(2026, 9, 5),
            &sigma,
            None,
        )
        .await
        .expect("the claim is retracted");

        let by_sigma = Some(crate::actor::Actor::Bot(sigma));
        let held = fake.recall(&homer).await.expect("the claims");
        assert_eq!(held.len(), 2, "the claim and the record of its retraction");
        for fact in held {
            let record = fake.stored_record(&fact.address());
            let (first, history) = fake.read_claims(|state| {
                let held = state.record(&record).expect("the State holds the record");
                (
                    held.first_writer.clone(),
                    held.history
                        .iter()
                        .map(|write| write.actor.clone())
                        .collect::<Vec<_>>(),
                )
            });
            assert_eq!(first, by_sigma, "{record:?}: first written by the bot");
            assert!(
                history.iter().all(|actor| *actor == by_sigma),
                "{record:?}: every write names its bot: {history:?}"
            );
        }
    }

    /// **The bot that merged is the actor of the claim the merge writes on the
    /// fake**, with no session to say so: the record of the fold is first written
    /// by it, and every write of every record on the survivor names it.
    #[tokio::test]
    async fn the_bot_that_merged_is_the_actor_of_the_claim_the_merge_writes_on_the_fake() {
        let fake = InMemoryMemory::booted();
        let homer = EntityId("person:homer".into());
        let marge = EntityId("person:marge".into());
        let sigma = EntityId("bot:sigma".into());
        for (who, name) in [(&homer, "Homer"), (&marge, "Marge")] {
            fake.add_entity(NewEntity::new(who.clone(), name, "fixture"))
                .await
                .expect("add")
                .written()
                .expect("written");
            let mut fact = NewFact::about(
                who.clone(),
                "keeps the house",
                jiff::civil::date(2026, 9, 1),
            );
            fact.provenance = Provenance::Inference;
            fact.written_by_bot = Some(sigma.clone());
            fake.capture(fact)
                .await
                .expect("capture")
                .written()
                .expect("not blocked");
        }
        fake.merge(
            &marge,
            &homer,
            Some("filed twice"),
            jiff::civil::date(2026, 9, 3),
            &sigma,
            None,
        )
        .await
        .expect("the fold lands");

        let by_sigma = Some(crate::actor::Actor::Bot(sigma));
        let held = fake.recall(&homer).await.expect("the claims");
        assert_eq!(
            held.len(),
            3,
            "Homer's claim, Marge's claim and the record of the fold"
        );
        for fact in held {
            // A claim is served under the handle; the merges are walked by key.
            let served = FactAddress::new(
                fake.storage_key(&fact.home)
                    .expect("the fake files the thing"),
                fact.id.clone(),
            );
            let record = fake.stored_record(&served);
            let (first, history) = fake.read_claims(|state| {
                let held = state.record(&record).expect("the State holds the record");
                (
                    held.first_writer.clone(),
                    held.history
                        .iter()
                        .map(|write| write.actor.clone())
                        .collect::<Vec<_>>(),
                )
            });
            assert_eq!(first, by_sigma, "{record:?}: first written by the bot");
            assert!(
                history.iter().all(|actor| *actor == by_sigma),
                "{record:?}: every write names its bot: {history:?}"
            );
        }
    }
}
