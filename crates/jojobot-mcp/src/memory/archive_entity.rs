//! `archive_entity` — Take an entity out of every default read, by handle, or
//! bring an archived one back with `restore: true`.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `archive_entity`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ArchiveEntityArgs {
    /// The entity's handle.
    pub(crate) handle: String,
    /// Why, in your own words. Required: an archived entity with no reason
    /// is a record nobody can interpret later, which defeats the point of
    /// leaving it reachable by digging. A mistaken write, one that should
    /// not have happened, somebody not relevant at all — examples, never a
    /// set to choose from.
    pub(crate) reason: String,
    /// **Send `true` to bring an archived entity back** into every default
    /// read instead of archiving it. `reason` is then why it comes back. The
    /// entity's claims, edges and children were never touched, so nothing else
    /// has to be restored. The act is recorded as a claim on the entity that
    /// names when it was archived and why.
    #[serde(default)]
    pub(crate) restore: Option<bool>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

/// Archive an entity: out of every default read, reachable by handle.
#[tool_router(router = archive_entity_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Take an entity out of every default read — list_entities excludes it, and \
                       everything that browses rather than names a handle stops seeing it. \
                       Nothing is deleted: the entity survives, a link's far end still resolves \
                       to it, and recalling it by its own handle serves it whole, including the \
                       reason and when it was archived. TO UNDO IT send restore: true with the same \
                       handle and a reason: the entity returns to every default read, and a \
                       claim on it records when it was archived, why, and why it came back. THIS \
                       IS FOR SOMETHING THAT \
                       SHOULD NOT BE SURFACED BY DEFAULT ANY MORE — a mistaken write, one that \
                       should not have happened, somebody not relevant at all. It does not touch \
                       the entity's claims or edges, which stand exactly as recorded: archiving \
                       says the SUBJECT is out of scope, never that anything said about it was \
                       wrong. FOR A DUPLICATE THAT HOLDS CLAIMS THIS IS THE WRONG VERB: it leaves \
                       the claims on a thing no default read shows, and merge_entities is the \
                       verb that carries them to the thing it duplicates. AN ARCHIVED LOOP DROPS \
                       OUT OF overdue READS, and the read counts it in archived_excluded. \
                       Archiving something already archived comes back blocked, saying the \
                       entity is already in the state you asked for — that answer means jojobot \
                       holds what you wanted, not that nothing happened. Restoring something that is not archived says the same. A handle that names \
                       nothing comes back blocked with the nearest handles."
    )]
    pub(crate) async fn archive_entity(
        &self,
        Parameters(args): Parameters<ArchiveEntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        let caller = match self.identified_for_write(args.sid.as_deref()).await {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let handle = EntityId::person(&args.handle);
        if let Some(refused) = self
            .refuse_a_stranger_the_role_object(&caller.bot, &handle)
            .await
        {
            return Ok(refused);
        }
        if args.restore.unwrap_or(false) {
            return self.restore(&handle, &args).await;
        }
        let entity = match self.memory.archive_entity(&handle, &args.reason).await {
            Ok(entity) => entity,
            Err(e) => return memory_declined("archive_entity", e),
        };
        self.beat("archive_entity", entity.id.as_str(), args.sid.as_deref())
            .await;
        let mut body = entity_json(&entity);
        // **No reason given, and that is the honest answer.** Reading a bare
        // handle as a person is what the argument means; a sentence
        // restating the comparison would be a manufactured justification.
        crate::answer::note_delta(
            &mut body,
            crate::answer::Difference::between("handle", Some(&args.handle), entity.id.as_str())
                .into_iter()
                .collect(),
        );
        json_result(&body)
    }
}

impl Jojobot {
    /// **Bring an archived entity back, and leave the record of it on the entity.**
    ///
    /// The store clears the archive and hands back what it cleared, because the
    /// row no longer says it. The record is an ordinary claim on the entity,
    /// worked out by the restorer rather than said by the operator, that names
    /// both acts. It is written after the restore, so a failure to write it is
    /// said in the answer rather than undoing a restore that landed.
    async fn restore(
        &self,
        handle: &EntityId,
        args: &ArchiveEntityArgs,
    ) -> Result<CallToolResult, McpError> {
        let (entity, was) = match self.memory.restore_entity(handle).await {
            Ok(restored) => restored,
            Err(e) => return memory_declined("archive_entity", e),
        };
        let day = self.dated(None, args.sid.as_deref()).await?;
        // **The same frame as `day`**: the run's zone, else the instance's. The
        // restore day and the archive day sit in one sentence of one claim.
        let zone = match self
            .caller(args.sid.as_deref())
            .ok()
            .flatten()
            .and_then(|caller| caller.zone())
        {
            Some(zone) => zone,
            None => self.unzoned_frame().await,
        };
        let archived_on = self.clock().day_of(was.at, &zone);
        let content = format!(
            "Restored from the archive on {day}, because: {}. It was archived on {archived_on}, \
             because: {}.",
            args.reason.trim(),
            was.reason,
        );
        let recorded = self
            .memory
            .capture(NewFact {
                provenance: Provenance::Inference,
                ..NewFact::about(entity.id.clone(), content, day)
            })
            .await;
        self.beat("archive_entity", entity.id.as_str(), args.sid.as_deref())
            .await;
        let mut body = entity_json(&entity);
        if let Some(object) = body.as_object_mut() {
            object.insert("restored".into(), true.into());
            let recorded_as = match recorded {
                Ok(Guarded::Written(fact)) => fact.address().to_string(),
                _ => "the restore landed, but the claim recording it could not be written; \
                      capture one on the entity to keep the history"
                    .to_string(),
            };
            object.insert("recorded_as".into(), recorded_as.into());
        }
        json_result(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    fn archive(handle: &str, reason: &str, sid: &str) -> ArchiveEntityArgs {
        ArchiveEntityArgs {
            handle: handle.into(),
            reason: reason.into(),
            restore: None,
            sid: Some(sid.into()),
        }
    }

    /// 🚨 **The pair that proves archiving through the verb reaches the same
    /// state the domain method does.** The receipt names the entity and its
    /// reason, and the same entity then drops out of `list_entities`.
    #[tokio::test]
    async fn archiving_by_handle_lands_and_is_excluded_from_the_default_listing() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        ensure(&jojobot, "person:bart").await;

        let body = json_of(
            &jojobot
                .archive_entity(Parameters(archive("bart", "a mistaken write", &sid)))
                .await
                .expect("archive_entity ok"),
        );
        assert_eq!(body["id"], "person:bart", "{body}");
        assert_eq!(body["archived"]["reason"], "a mistaken write", "{body}");
        assert!(body["archived"]["at"].as_str().is_some(), "{body}");

        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: None,
                    parent: None,
                    offset: None,
                    sid: None,
                }))
                .await
                .expect("list_entities ok"),
        );
        let ids: Vec<&str> = listed["entities"]
            .as_array()
            .expect("entities is a list")
            .iter()
            .map(|e| e["id"].as_str().expect("an id"))
            .collect();
        assert!(
            !ids.contains(&"person:bart"),
            "the entity this call archived still crossed the default listing: {listed}",
        );
    }

    /// **The restore claim names both days in one frame.** It says on what day
    /// the entity came back and on what day it was archived, and the archive day
    /// used to be read in UTC while the restore day was the run's own, in the
    /// instance's zone. The two acts happen a moment apart here, so one frame
    /// gives one day twice. The instance's zone is picked so that its date is not
    /// UTC's at this moment: one of two zones 25 hours apart always differs, which
    /// makes the case about the frame rather than about the time of day. The days
    /// are read as the dates the claim holds, not as its prose.
    #[tokio::test]
    async fn the_restore_claim_names_both_days_in_one_frame() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        let utc_today = jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::UTC)
            .date();
        let zone = ["Pacific/Kiritimati", "Pacific/Pago_Pago"]
            .into_iter()
            .find(|name| {
                let zone = jiff::tz::TimeZone::get(name).expect("a zone");
                jiff::Timestamp::now().to_zoned(zone).date() != utc_today
            })
            .expect("one of the two zones is on another date than UTC");
        ensure(&jojobot, "topic:instance").await;
        jojobot
            .memory
            .capture(jojobot_domain::memory::NewFact {
                fields: [("timezone".to_string(), zone.to_string())]
                    .into_iter()
                    .collect(),
                ..jojobot_domain::memory::NewFact::about(
                    EntityId("topic:instance".into()),
                    "the instance works in one zone",
                    utc_today,
                )
            })
            .await
            .expect("capture ok");
        ensure(&jojobot, "person:bart").await;
        jojobot
            .archive_entity(Parameters(archive("bart", "a mistaken write", &sid)))
            .await
            .expect("archive ok");

        let restored = json_of(
            &jojobot
                .archive_entity(Parameters(ArchiveEntityArgs {
                    restore: Some(true),
                    ..archive("bart", "it was real after all", &sid)
                }))
                .await
                .expect("restore ok"),
        );
        let address = restored["recorded_as"]
            .as_str()
            .expect("the restore records itself")
            .to_string();
        let record = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    facts: Some(true),
                    ..recall_args("person:bart")
                }))
                .await
                .expect("recall ok"),
        );
        let content = record["objects"][0]["facts"]
            .as_array()
            .and_then(|facts| {
                facts
                    .iter()
                    .find(|fact| fact["address"] == address.as_str())
            })
            .and_then(|fact| fact["content"].as_str())
            .unwrap_or_else(|| panic!("the restore claim reads back: {record}"))
            .to_string();
        let days: Vec<&str> = content
            .split(|c: char| !(c.is_ascii_digit() || c == '-'))
            .filter(|word| {
                word.len() == 10 && word.as_bytes()[4] == b'-' && word.as_bytes()[7] == b'-'
            })
            .collect();
        assert_eq!(days.len(), 2, "the claim names two days: {content}");
        assert_eq!(
            days[0], days[1],
            "the restore day and the archive day are in two frames: {content}"
        );
    }

    /// **Archiving something already archived is blocked, not a silent
    /// second write** — the caller is told the state jojobot holds is the
    /// one they asked for.
    #[tokio::test]
    async fn a_second_archive_is_blocked_and_names_the_state_jojobot_holds() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        ensure(&jojobot, "person:milhouse").await;
        jojobot
            .archive_entity(Parameters(archive("milhouse", "a mistaken write", &sid)))
            .await
            .expect("the first archive should succeed");

        let body = json_of(
            &jojobot
                .archive_entity(Parameters(archive(
                    "milhouse",
                    "somebody not relevant at all",
                    &sid,
                )))
                .await
                .expect("archive_entity ok"),
        );
        assert_eq!(body["status"], "blocked", "{body}");
        assert_eq!(body["wrote"], false, "{body}");
        assert!(
            body["how_to_proceed"]
                .as_str()
                .is_some_and(|s| s.contains("already archived")),
            "the refusal must say the entity is already in the state asked for: {body}",
        );
    }

    /// A handle nothing answers to is blocked with the nearest handles,
    /// never a bare miss and never a write.
    #[tokio::test]
    async fn an_unknown_handle_is_blocked_with_candidates() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        ensure(&jojobot, "person:gene").await;

        let body = json_of(
            &jojobot
                .archive_entity(Parameters(archive(
                    "person:contract-no-such",
                    "a mistaken write",
                    &sid,
                )))
                .await
                .expect("archive_entity ok"),
        );
        assert_eq!(body["status"], "blocked", "{body}");
        assert_eq!(body["wrote"], false, "{body}");
    }

    /// **The reason is required, never an enumeration.** An empty reason is
    /// refused the same way any other blank field is — it is not a fixed set
    /// of causes to validate against, only the ordinary "say something" bar
    /// every other free-text field on this surface holds to.
    #[tokio::test]
    async fn a_blank_reason_is_refused() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        ensure(&jojobot, "person:otto").await;

        let body = json_of(
            &jojobot
                .archive_entity(Parameters(archive("otto", "", &sid)))
                .await
                .expect("archive_entity ok"),
        );
        assert_eq!(body["status"], "blocked", "{body}");
        assert_eq!(body["wrote"], false, "{body}");
    }
}
