//! `archive_entity` — Take an entity out of every default read, by handle. One way.
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
                       reason and when it was archived. ONE WAY, matching a claim's own archived \
                       state: there is no un-archive over this surface. THIS IS FOR SOMETHING THAT \
                       SHOULD NOT BE SURFACED BY DEFAULT ANY MORE — a mistaken write, one that \
                       should not have happened, somebody not relevant at all. It does not touch \
                       the entity's claims or edges, which stand exactly as recorded: archiving \
                       says the SUBJECT is out of scope, never that anything said about it was \
                       wrong. Archiving something already archived comes back blocked, saying the \
                       entity is already in the state you asked for — that answer means jojobot \
                       holds what you wanted, not that nothing happened. A handle that names \
                       nothing comes back blocked with the nearest handles."
    )]
    pub(crate) async fn archive_entity(
        &self,
        Parameters(args): Parameters<ArchiveEntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        if let Err(refused) = self.identified(args.sid.as_deref()) {
            return Ok(refused);
        }
        let handle = EntityId::person(&args.handle);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    fn archive(handle: &str, reason: &str, sid: &str) -> ArchiveEntityArgs {
        ArchiveEntityArgs {
            handle: handle.into(),
            reason: reason.into(),
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
