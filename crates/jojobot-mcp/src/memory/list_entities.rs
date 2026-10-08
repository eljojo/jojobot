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
    /// **How many entities you have already read.** The listing stops at the
    /// answer ceiling and names the `offset` that reads on; repeat the call
    /// with it to walk the rest.
    #[serde(default)]
    pub(crate) offset: Option<u32>,
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
                       blocked with candidates, never an empty list. Metadata only — no facts. \
                       Ordered by handle, and filled in that order under the answer ceiling: \
                       `not_shown` says how many entities came after the ones carried and the \
                       `offset` that reads on, and an `offset` past the last says so. An \
                       archived entity is excluded — recall it by its \
                       own handle to read it whole, including why and when it was archived, and \
                       call archive_entity with restore: true to bring it back. So is one that \
                       was merged into another (counted apart, as merged_excluded): its handle \
                       still answers, and says where it went. An empty answer carries `searched`: \
                       one line naming what it looked through and what it left out."
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
        let archived = entities.iter().filter(|e| e.archived.is_some()).count();
        let folded = entities.iter().filter(|e| e.left_out_as_folded()).count();
        let mut entities: Vec<_> = entities.into_iter().filter(Entity::browsable).collect();
        // **Ordered by handle, so a walk with `offset` is a walk of one list.**
        // The store hands entities back in whatever order it keeps them, and
        // a continuation that read a different order would skip some and
        // repeat others.
        entities.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        let held = entities.len();
        let offset = args.offset.map_or(0, |o| o as usize);
        // **The ceiling is spent in this order, whole entities only.** Rendered
        // first so the page can be filled once the rest of the answer is counted.
        let rendered: Vec<(serde_json::Value, usize)> = entities
            .iter()
            .skip(offset)
            .map(|entity| {
                let json = entity_json(entity);
                let size = json.to_string().chars().count() + 1;
                (json, size)
            })
            .collect();
        // The envelope, with the block that names what was left out at its
        // widest, so what ships is under the ceiling.
        let envelope = serde_json::json!({
            "count": rendered.len(),
            // 🚨 **How many this read excluded as archived** — a total, the
            // same shape `recall`'s `withheld` uses: an empty inventory and a
            // suppressed one are the same "nothing here" without it.
            "archived_excluded": archived,
            // **And how many it left out because they were folded into another
            // thing**, counted apart: an archived thing was put away, a folded
            // one went into the survivor, and the handle of each still answers.
            "merged_excluded": folded,
            "entities": [],
        });
        // The block is counted with the key it rides under.
        let widest_not_shown =
            serde_json::json!({"not_shown": answer::not_shown(held, held, "entities")});
        let rest = envelope.to_string().chars().count()
            + widest_not_shown.to_string().chars().count()
            + answer::STATUS_BAR_ROOM;
        let kept = jojobot_domain::text::Capped::beside(rest).head(&rendered, |(_, size)| *size);
        let shown = kept.kept().len();
        let mut body = envelope;
        body["count"] = shown.into();
        body["entities"] = kept
            .kept()
            .iter()
            .map(|(json, _)| json.clone())
            .collect::<Vec<_>>()
            .into();
        // **Eliding is never silent.** This says how many there are after the
        // ones above and which call reads them, and it is absent when nothing
        // was cut rather than saying "0 left out".
        if held > offset + shown {
            body["not_shown"] =
                answer::not_shown(held - offset - shown, offset + shown, "entities");
        }
        if shown == 0 && offset > 0 && held > 0 {
            body["past_the_end"] = format!(
                "offset {offset} is past the last of the {held} entities this read lists, so \
                 nothing is left to read"
            )
            .into();
        }
        if held == 0 {
            let looked = match (args.kind.as_deref(), args.parent.as_deref()) {
                (Some(kind), Some(parent)) => {
                    format!("the direct children of {parent} of kind {kind}")
                }
                (Some(kind), None) => format!("every entity of kind {kind}"),
                (None, Some(parent)) => format!("the direct children of {parent}"),
                (None, None) => "every kind of entity".to_string(),
            };
            body["searched"] = answer::population_line(
                &looked,
                &[
                    "archived entities".to_string(),
                    "entities merged into another".to_string(),
                ],
                "drop `kind` or `parent`; recall a handle to read an archived entity",
            )
            .into();
        }
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
            parent: None,
            answers_type: None,
            fields: None,
            facts: None,
            stood_for: None,
            status: None,
            prose: None,
            charter: None,
            keys: None,
            history: None,
            history_record: None,
            history_most: None,
            offset: None,
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
                    offset: None,
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
                    offset: None,
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
                    offset: None,
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

    /// 🚨 **An empty inventory names what it looked through.** "Nothing of that
    /// kind" and "nothing at all, because they are archived" are the same empty
    /// list without it. The line names the kind asked for, says archived
    /// entities were left out, and names the call that widens the read.
    #[tokio::test]
    async fn an_empty_listing_names_the_kind_it_looked_through_and_what_it_left_out() {
        let jojobot = handler();
        ensure(&jojobot, "thing:jukebox").await;
        jojobot
            .memory
            .archive_entity(&EntityId("thing:jukebox".into()), "a mistaken write")
            .await
            .expect("archive_entity ok");
        let asked = |kind: &str| ListEntitiesArgs {
            kind: Some(kind.into()),
            parent: None,
            offset: None,
            sid: None,
        };

        let empty = json_of(
            &jojobot
                .list_entities(Parameters(asked("thing")))
                .await
                .expect("list_entities ok"),
        );
        assert_eq!(empty["count"], 0, "{empty}");
        let line = empty["searched"]
            .as_str()
            .expect("an empty read names its population");
        assert!(!line.contains('\n'), "one line: {line}");
        assert!(
            line.contains("thing"),
            "names the kind it looked through: {line}"
        );
        assert!(line.contains("archived"), "names what it left out: {line}");
        assert!(
            line.contains("recall"),
            "names the call that widens it: {line}"
        );

        // The positive: the same call with one live matching entity returns it,
        // and an answer that holds something carries no such line.
        ensure(&jojobot, "thing:teapot").await;
        let full = json_of(
            &jojobot
                .list_entities(Parameters(asked("thing")))
                .await
                .expect("list_entities ok"),
        );
        assert_eq!(full["count"], 1, "{full}");
        assert!(
            full["searched"].is_null(),
            "a non-empty read needs no line: {full}"
        );
    }

    /// **A `parent` that has no children is named in the line**, so an empty
    /// answer under a parent is not read as an empty inventory.
    #[tokio::test]
    async fn an_empty_listing_under_a_parent_names_the_parent() {
        let jojobot = handler();
        ensure(&jojobot, "person:alpha").await;
        let body = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: None,
                    parent: Some("person:alpha".into()),
                    offset: None,
                    sid: None,
                }))
                .await
                .expect("list_entities ok"),
        );
        assert_eq!(body["count"], 0, "{body}");
        let line = body["searched"]
            .as_str()
            .expect("an empty read names its population");
        assert!(line.contains("person:alpha"), "names the parent: {line}");
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

    /// **An archived entity's block says how it comes back.** A reader who
    /// finds a thing archived by reaching for it by handle has no other way to
    /// learn that the act is undone by `archive_entity` with `restore`, and the
    /// block is where the reader is looking. An active entity carries no block.
    #[tokio::test]
    async fn an_archived_entitys_block_points_at_the_restore() {
        let jojobot = handler();
        ensure(&jojobot, "person:bart").await;
        ensure(&jojobot, "person:lisa").await;
        jojobot
            .memory
            .archive_entity(&EntityId("person:bart".into()), "a mistaken write")
            .await
            .expect("archive_entity ok");

        let read = |handle: &'static str| {
            let jojobot = &jojobot;
            async move {
                json_of(
                    &jojobot
                        .recall(Parameters(of_subject(handle)))
                        .await
                        .expect("recall ok"),
                )
            }
        };
        let archived = read("person:bart").await;
        let pointer = archived["objects"][0]["archived"]["how_to_restore"]
            .as_str()
            .unwrap_or_else(|| panic!("the block points at the restore: {archived}"));
        for named in ["archive_entity", "restore"] {
            assert!(pointer.contains(named), "{named}: {pointer}");
        }

        let active = read("person:lisa").await;
        assert!(active["objects"][0]["archived"].is_null(), "{active}");
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
                    offset: None,
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
                    offset: None,
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
                offset: None,
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

    /// Fold `duplicate` into `survivor` through the verb a caller uses.
    async fn merged(jojobot: &Jojobot, duplicate: &str, survivor: &str) {
        let landed = json_of(
            &jojobot
                .merge_entities(Parameters(crate::memory::merge_entities::MergeArgs {
                    duplicate: duplicate.into(),
                    survivor: survivor.into(),
                    reason: None,
                    recorded_at: None,
                    sid: Some(writing_as(jojobot)),
                }))
                .await
                .expect("merge ok"),
        );
        assert_eq!(landed["merged"], duplicate, "the merge landed: {landed}");
    }

    /// 🚨 **A thing that was folded into another is not in the inventory, is
    /// counted as left out, and still answers when it is asked for.** Paired
    /// with the survivor, which is listed: the negative alone would pass on a
    /// listing that returned nothing.
    #[tokio::test]
    async fn a_merged_away_entity_is_not_listed_but_is_counted_and_still_answers() {
        let jojobot = handler();
        ensure(&jojobot, "person:bart").await;
        ensure(&jojobot, "person:milhouse").await;
        merged(&jojobot, "person:bart", "person:milhouse").await;

        let body = json_of(
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
        let ids: Vec<&str> = body["entities"]
            .as_array()
            .expect("entities is a list")
            .iter()
            .filter_map(|e| e["id"].as_str())
            .collect();
        assert!(
            ids.contains(&"person:milhouse"),
            "the survivor is listed: {body}"
        );
        assert!(
            !ids.contains(&"person:bart"),
            "a folded thing is not a thing in the inventory: {body}"
        );
        assert_eq!(
            body["merged_excluded"], 1,
            "the listing says one was left out as folded: {body}"
        );
        assert_eq!(
            body["archived_excluded"], 0,
            "a folded thing is not counted as archived: {body}"
        );

        let recalled = json_of(
            &jojobot
                .recall(Parameters(of_subject("person:bart")))
                .await
                .expect("recall ok"),
        );
        assert!(
            recalled.to_string().contains("person:milhouse"),
            "the direct door still answers a folded handle, and says where it went: {recalled}"
        );
    }

    /// A slug of ten letters that depends on nothing but the index, spread far
    /// enough apart that no two screen as near-misses of each other.
    fn slug(i: usize) -> String {
        let mut state = (i as u64)
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (0..10)
            .map(|_| {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (b'a' + ((state >> 33) % 26) as u8) as char
            })
            .collect()
    }

    /// `count` things in the store, each with a `source` of `source_len`
    /// characters (200 at most), written straight to its port: the verbs' own guards are not
    /// what these cases are about. **The size comes from the source and not
    /// from more things**, because the write guard screens every new name
    /// against all the ones held, and a few hundred things cost seconds to add.
    async fn with_things(count: usize, source_len: usize) -> Jojobot {
        let jojobot = handler();
        for i in 0..count {
            jojobot
                .memory
                .add_entity(NewEntity::new(
                    EntityId::new(EntityKind::THING, slug(i)),
                    format!("Gizmo {}", slug(i)),
                    "s".repeat(source_len),
                ))
                .await
                .expect("an entity lands");
        }
        jojobot
    }

    async fn listed(jojobot: &Jojobot, offset: Option<u32>) -> serde_json::Value {
        json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: None,
                    parent: None,
                    offset,
                    sid: None,
                }))
                .await
                .expect("list_entities ok"),
        )
    }

    /// **A listing past the ceiling fits it, and a walk on returns each entity
    /// once, in order.** The answer plus the status bar is under the ceiling,
    /// what it left out is counted with the offset that reads on, and the parts
    /// together are exactly the entities in the store, ordered by handle.
    #[tokio::test]
    async fn a_listing_past_the_ceiling_fits_it_and_a_walk_returns_each_entity_once() {
        let jojobot = with_things(90, 200).await;
        let mut seen: Vec<String> = Vec::new();
        let mut offset = 0u32;
        let mut parts = 0;
        loop {
            let body = listed(&jojobot, Some(offset)).await;
            parts += 1;
            assert!(parts < 20, "the walk never ends");
            let size = body.to_string().chars().count();
            assert!(
                size + crate::answer::STATUS_BAR_ROOM <= jojobot_domain::text::ANSWER_CEILING,
                "part {parts} is {size} characters"
            );
            let entities = body["entities"].as_array().expect("a list of entities");
            assert_eq!(body["count"], entities.len(), "{body}");
            seen.extend(
                entities
                    .iter()
                    .map(|e| e["id"].as_str().expect("an id").to_string()),
            );
            match body["not_shown"]["offset"].as_u64() {
                Some(next) => {
                    assert_eq!(
                        next as usize,
                        seen.len(),
                        "the offset reads on from here: {body}"
                    );
                    offset = next as u32;
                }
                None => break,
            }
        }
        assert!(parts > 1, "ninety entities this size do not fit one answer");
        let mut expected: Vec<String> = (0..90).map(|i| format!("thing:{}", slug(i))).collect();
        expected.sort();
        assert_eq!(seen, expected, "every entity once, ordered by handle");
    }

    /// **The envelope is counted at every size of listing.** A fill that
    /// spends the whole ceiling on entities passes it by the envelope and the
    /// block that names what was left out, but only when the entities happen to
    /// add up inside that window. Sweeping the length of each entity's source
    /// in steps finer than the window is what lands in it.
    #[tokio::test]
    async fn a_listing_counts_its_own_envelope_at_every_size() {
        for source_len in 168..=192 {
            let count = 90;
            let jojobot = with_things(count, source_len).await;
            let body = listed(&jojobot, None).await;
            let size = body.to_string().chars().count();
            assert!(
                size + crate::answer::STATUS_BAR_ROOM <= jojobot_domain::text::ANSWER_CEILING,
                "sources of {source_len}: the answer is {size} characters"
            );
            let carried = body["entities"].as_array().expect("entities").len();
            let left = body["not_shown"]["count"].as_u64().unwrap_or(0) as usize;
            assert_eq!(carried + left, count, "sources of {source_len}: {body}");
        }
    }

    /// **A listing that fits comes back whole and unmarked.** The control for
    /// the two cases above: a fill that cut everything would pass them.
    #[tokio::test]
    async fn a_listing_that_fits_carries_every_entity_and_no_not_shown() {
        let jojobot = with_things(5, 40).await;
        let body = listed(&jojobot, None).await;
        assert_eq!(body["count"], 5, "{body}");
        assert_eq!(body["entities"].as_array().map(Vec::len), Some(5), "{body}");
        assert!(body.get("not_shown").is_none(), "{body}");
        assert!(body.get("past_the_end").is_none(), "{body}");
    }

    /// **An offset past the last entity says so** and carries nothing.
    #[tokio::test]
    async fn an_offset_past_the_last_entity_says_it_is_past_the_end() {
        let jojobot = with_things(5, 40).await;
        let body = listed(&jojobot, Some(50)).await;
        assert!(body["past_the_end"].is_string(), "{body}");
        assert_eq!(body["entities"].as_array().map(Vec::len), Some(0), "{body}");
        assert!(body.get("searched").is_none(), "{body}");
    }
}
