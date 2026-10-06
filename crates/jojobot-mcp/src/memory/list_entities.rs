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
    /// **Narrow to this entity's DIRECT children only** — combines with
    /// `kind` when both are named. Never a grandchild and never the whole
    /// subtree: zoom one level at a time, the same way the tree itself is
    /// built one `parent` at a time. A handle that names no entity comes
    /// back blocked with candidates, never an empty list — an empty list
    /// means "nothing is under it", which is a different answer.
    #[serde(default)]
    pub(crate) parent: Option<String>,
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
        description = "List the entities jojobot knows, optionally narrowed to one kind and/or \
                       to one entity's DIRECT children (`parent`) — the inventory. Use it to \
                       orient, or as the cheap existence check before a write that must name an \
                       entity; use search when you are looking for something. `parent` never \
                       reaches a grandchild and never returns the whole subtree — one level at a \
                       time, the same way a tree is built one `parent` at a time; ask again with \
                       a child's own handle to go deeper. A `parent` naming nothing comes back \
                       blocked with candidates, never an empty list. Metadata only — no facts, \
                       no ordering guarantee. An archived entity is excluded — recall it by its \
                       own handle to read it whole, including why and when it was archived."
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
        let entities = match args.parent.as_deref() {
            None => self
                .memory
                .list_entities(kind)
                .await
                .map_err(memory_error)?,
            // **Direct children only — `Memory::children` already holds that
            // line**, so this wraps it rather than walking the tree itself.
            // An unresolved parent is `MemoryError::UnknownEntity`, which
            // `memory_declined` already turns into a blocked answer with
            // candidates — never an empty list, which would read as "nothing
            // is under it" rather than "that handle names nothing".
            Some(parent) => {
                let child_ids = match self.memory.children(&EntityId::person(parent)).await {
                    Ok(ids) => ids,
                    Err(e) => return memory_declined("list_entities", e),
                };
                let child_ids: std::collections::HashSet<_> = child_ids.into_iter().collect();
                self.memory
                    .list_entities(kind)
                    .await
                    .map_err(memory_error)?
                    .into_iter()
                    .filter(|e| child_ids.contains(&e.id))
                    .collect()
            }
        };
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
        if let Some(sid) = args.sid.as_deref() {
            self.registry.note_shown(sid, &body);
        }
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
                    parent: None,
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
                    parent: None,
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
                    parent: None,
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

    /// **`parent` reaches one level, never the whole subtree.**
    ///
    /// A tree at least two deep: `person:alpha` — `thing:jukebox` —
    /// `topic:the-five-words`. **What a build that walked the whole subtree
    /// would print here**: both `thing:jukebox` AND `topic:the-five-words`
    /// in `entities`, since a grandchild is still under the root by that
    /// (wrong) reading. The negative below is what tells that build from
    /// this one — the positive alone would pass either way.
    #[tokio::test]
    async fn a_parent_returns_direct_children_only_never_a_grandchild() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                sid: Some(sid.clone()),
                ..add_args("person", "alpha", "Alpha")
            }))
            .await
            .expect("add ok");
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                parent: Some("person:alpha".into()),
                sid: Some(sid.clone()),
                ..add_args("thing", "jukebox", "The Jukebox")
            }))
            .await
            .expect("add ok");
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                parent: Some("thing:jukebox".into()),
                sid: Some(sid),
                ..add_args("topic", "the-five-words", "The Five Words")
            }))
            .await
            .expect("add ok");

        let body = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: None,
                    parent: Some("person:alpha".into()),
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
        assert_eq!(
            ids,
            vec!["thing:jukebox"],
            "only the direct child answers: {body}",
        );
        assert!(
            !ids.contains(&"topic:the-five-words"),
            "a grandchild crossed the one-level boundary this verb's own description \
             promises it never does: {body}",
        );
    }

    /// **`parent` combines with `kind`, naturally**: two children, one of
    /// each kind, and `kind` keeps only the one that answers both axes.
    #[tokio::test]
    async fn a_parent_and_a_kind_narrow_together() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                sid: Some(sid.clone()),
                ..add_args("person", "alpha", "Alpha")
            }))
            .await
            .expect("add ok");
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                parent: Some("person:alpha".into()),
                sid: Some(sid.clone()),
                ..add_args("thing", "jukebox", "The Jukebox")
            }))
            .await
            .expect("add ok");
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                parent: Some("person:alpha".into()),
                sid: Some(sid),
                ..add_args("topic", "widgets", "Widgets")
            }))
            .await
            .expect("add ok");

        let body = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("thing".into()),
                    parent: Some("person:alpha".into()),
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
        assert_eq!(
            ids,
            vec!["thing:jukebox"],
            "kind and parent narrow together, not each alone: {body}",
        );
    }

    /// **An unknown parent is a blocked answer with candidates, never an
    /// empty list.** An empty `entities` list means "nothing is under it" —
    /// a different, wrong answer for a handle that names nothing at all.
    #[tokio::test]
    async fn an_unknown_parent_is_blocked_with_candidates_never_an_empty_list() {
        let jojobot = handler();
        ensure(&jojobot, "person:alpha").await;

        let result = jojobot
            .list_entities(Parameters(ListEntitiesArgs {
                kind: None,
                parent: Some("person:alphaa".into()),
                sid: None,
            }))
            .await
            .expect("the call succeeds; the guard answers in the body");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "person:alphaa", "{body}");
        assert_eq!(
            body["candidates"][0]["handle"], "person:alpha",
            "the near miss is not offered: {body}",
        );
    }

    /// 🚨 **Discoverability: the verb's own description names the new
    /// axis.** A capability whose only path is that somebody read the diff
    /// has no path.
    #[test]
    fn parent_is_named_on_the_verbs_own_description() {
        let tools = Jojobot::tool_router().list_all();
        let list_entities = tools
            .iter()
            .find(|t| t.name.as_ref() == "list_entities")
            .expect("list_entities is a tool");
        let tool_description = list_entities.description.as_deref().unwrap_or_default();
        assert!(
            tool_description.contains("`parent`"),
            "the tool-level description does not name parent: {tool_description}"
        );
        assert!(
            tool_description.contains("DIRECT"),
            "the tool-level description does not say direct children only: {tool_description}"
        );
    }
}
