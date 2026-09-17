//! `list_entities` — The inventory: every entity jojobot knows, optionally by kind.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `list_entities`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListEntitiesArgs {
    /// Narrow to one kind; omit for every entity.
    #[serde(default)]
    pub(crate) kind: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

/// Every entity jojobot knows, optionally narrowed to one kind.
#[tool_router(router = list_entities_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "List the entities jojobot knows, optionally narrowed to one kind — the \
                       inventory. Use it to orient, or as the cheap existence check before a \
                       write that must name an entity; use search when you are looking for \
                       something. Metadata only — no facts, no ordering guarantee. An archived \
                       entity is excluded — recall it by its own handle to read it whole, \
                       including why and when it was archived."
    )]
    pub(crate) async fn list_entities(
        &self,
        Parameters(args): Parameters<ListEntitiesArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Resolved before the read runs — see [`Jojobot::attributable`]. This
        // verb publishes a `sid` and says it is what tells jojobot who is
        // asking, so a handle that addresses nothing is refused rather than
        // dropped.
        if let Err(refused) = self.attributable(args.sid.as_deref()) {
            return Ok(refused);
        }
        let kind = args.kind.as_deref().map(parse_kind).transpose()?;
        let entities = self
            .memory
            .list_entities(kind)
            .await
            .map_err(memory_error)?;
        // **Out of this default read.** Archived is reachable by asking for
        // the handle directly (through `recall`), never by browsing the
        // inventory — the same broad-door/direct-door split a claim's own
        // archived state already has.
        let before = entities.len();
        let entities: Vec<_> = entities.into_iter().filter(Entity::browsable).collect();
        let body = serde_json::json!({
            "count": entities.len(),
            // 🚨 **How many this read excluded as archived** — a total, the
            // same shape `recall`'s `withheld` uses: an empty inventory and a
            // suppressed one are the same "nothing here" without it.
            "archived_excluded": before - entities.len(),
            "entities": entities.iter().map(entity_json).collect::<Vec<_>>(),
        });
        json_result(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    /// A `recall` call naming nothing but a subject — the direct door.
    fn of_subject(subject: &str) -> RecallArgs {
        RecallArgs {
            subject: Some(subject.into()),
            kind: None,
            answers_type: None,
            fields: None,
            facts: None,
            stood_for: None,
            prose: None,
            charter: None,
            history: None,
            history_record: None,
            history_most: None,
            backing: None,
            built_on: None,
            values: None,
            values_most: None,
            overdue: None,
            near: None,
            follow: None,
            view: None,
            sid: None,
        }
    }

    /// 🚨 **The pair that proves the default door and the direct door differ.**
    /// The negative alone would pass on a read that returned nothing at all —
    /// the live entity is what says the read still found something.
    #[tokio::test]
    async fn an_archived_entity_is_out_of_the_default_listing_and_a_live_one_still_shows() {
        let jojobot = handler();
        ensure(&jojobot, "person:bart").await;
        ensure(&jojobot, "person:milhouse").await;
        jojobot
            .memory
            .archive_entity(&EntityId("person:bart".into()), "a mistaken write")
            .await
            .expect("archive_entity ok");

        let body = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: None,
                    sid: None,
                }))
                .await
                .expect("list_entities ok"),
        );
        let ids: Vec<&str> = body["entities"]
            .as_array()
            .expect("entities is a list")
            .iter()
            .map(|e| e["id"].as_str().expect("an id"))
            .collect();
        assert!(
            !ids.contains(&"person:bart"),
            "an archived entity crossed the default read: {body}",
        );
        assert!(
            ids.contains(&"person:milhouse"),
            "a live entity is missing from the default read: {body}",
        );
    }

    /// 🚨 **The listing accounts for what it excluded as archived** — the
    /// same reason `recall`'s `withheld` exists: an empty inventory and a
    /// suppressed one read as the same "nothing here" without a count.
    #[tokio::test]
    async fn the_default_listing_says_how_many_it_excluded_as_archived() {
        let jojobot = handler();
        ensure(&jojobot, "person:bart").await;
        ensure(&jojobot, "person:milhouse").await;
        jojobot
            .memory
            .archive_entity(&EntityId("person:bart".into()), "a mistaken write")
            .await
            .expect("archive_entity ok");

        let body = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: None,
                    sid: None,
                }))
                .await
                .expect("list_entities ok"),
        );
        assert_eq!(
            body["archived_excluded"], 1,
            "one entity was archived and the count says how many: {body}",
        );

        // The positive the count rests on: a store holding nothing archived
        // reports a zero, not an absent field.
        let clean_jojobot = handler();
        ensure(&clean_jojobot, "person:milhouse").await;
        let clean = json_of(
            &clean_jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: None,
                    sid: None,
                }))
                .await
                .expect("list_entities ok"),
        );
        assert_eq!(
            clean["archived_excluded"], 0,
            "nothing was archived and the count says zero, not nothing: {clean}",
        );
    }

    /// **The dig-for-it case.** Naming the handle directly, through `recall`,
    /// is the door that stays open — with the reason and the moment, not just
    /// the fact of it.
    #[tokio::test]
    async fn an_archived_entitys_own_handle_still_returns_it_whole_with_its_reason() {
        let jojobot = handler();
        ensure(&jojobot, "person:bart").await;
        jojobot
            .memory
            .archive_entity(&EntityId("person:bart".into()), "a mistaken write")
            .await
            .expect("archive_entity ok");

        let body = json_of(
            &jojobot
                .recall(Parameters(of_subject("person:bart")))
                .await
                .expect("recall ok"),
        );
        let object = &body["objects"][0];
        assert_eq!(object["id"], "person:bart", "{body}");
        assert_eq!(
            object["archived"]["reason"], "a mistaken write",
            "the direct door must serve the reason whole: {body}",
        );
        assert!(
            object["archived"]["at"].as_str().is_some(),
            "the direct door must serve when it was archived: {body}",
        );
    }
}
