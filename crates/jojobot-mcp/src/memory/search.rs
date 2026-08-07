//! `search` — The front door — one ranked list over entities, facts, prose and mail.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// The `edge` filter of a `search` — a shape and the entity it points at.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EdgeFilterArgs {
    /// Narrow to one shape (`location` · `membership` · `attendance` · `about` ·
    /// `connection`).
    /// Omit for **any** edge pointing at `object` — "what's connected to X".
    #[serde(default)]
    pub shape: Option<String>,
    /// The entity the edge must point at, as `kind:slug`.
    pub object: String,
}

/// Arguments to `search`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    /// Free text over entity handles/names, fact claims and details, and an
    /// entity's prose. **All words must match.** Optional when at least one
    /// filter below is given.
    #[serde(default)]
    pub query: Option<String>,
    /// Narrow to one entity kind — an entity's own kind, a fact's subject's kind,
    /// or the kind of the entity whose prose matched.
    #[serde(default)]
    pub kind: Option<String>,
    /// `active` (the default) or `superseded`. A superseded fact is **excluded
    /// unless asked for by name** — a claim already moved past must not come
    /// back as current truth.
    #[serde(default)]
    pub status: Option<String>,
    /// `testimony` or `inference`.
    #[serde(default)]
    pub provenance: Option<String>,
    /// Facts about this entity, as `kind:slug`.
    #[serde(default)]
    pub subject: Option<String>,
    /// Facts drawing a matching edge. With `kind`, this is how a cross-entity
    /// question ("which people are in X") is answered in one call.
    #[serde(default)]
    pub edge: Option<EdgeFilterArgs>,
    /// Whether messages left in mailboxes are searched too. **Defaults to
    /// true** — a report filed for another session is exactly the context you
    /// would not know to go looking for. Pass `false` to keep session traffic
    /// out of a question about the operator's life.
    #[serde(default)]
    pub include_mail: Option<bool>,
    /// How many results; defaults to 20. There is no pagination — a second page
    /// is a better query.
    #[serde(default)]
    pub limit: Option<u32>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// One search result on the wire. **Every hit says what it is** (`hit`), so a
/// caller reads a mixed list without guessing from its shape — and each kind of
/// hit carries what makes it actionable: an entity its handle, a fact its whole
/// row and address, prose its title, the entity that owns it and the text
/// around the match.
///
/// **And every hit arrives with its surroundings.** A fact adds `about` and
/// `home` — its subject and its home entity, resolved to every name they
/// answer to — and an
/// entity or a stretch of prose adds `edges`, where it sits in the graph. The
/// enrichment is strictly additive: `subject` is still the same handle string
/// here as in `recall`, so one record has one spelling across every verb.
///
/// **What a hit does NOT carry is the id of the thing it was stored in.** Both
/// entity and prose hits shipped one, and it was the only place on the whole
/// surface where a caller could learn that entities have documents at all. It
/// is not an address a caller can use — every verb here is addressed by handle
/// or by fact address — so nothing was lost by pulling it and a standing leak
/// was closed. Internally the id is still what orders a hit list; that is the
/// index's business (`jojobot_adapters::search::tiebreak`) and it stops there.
fn hit_json(hit: &Hit) -> serde_json::Value {
    match hit {
        Hit::Entity { entity, edges, .. } => {
            let mut body = entity_json(entity);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("hit".into(), "entity".into());
                obj.insert("edges".into(), edges.iter().map(edge_json).collect());
            }
            body
        }
        Hit::Fact {
            fact,
            subject,
            home,
        } => {
            let mut body = fact_json(fact);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("hit".into(), "fact".into());
                obj.insert("about".into(), entity_ref_json(subject));
                obj.insert("home".into(), entity_ref_json(home));
            }
            body
        }
        // A mail hit is unmistakably mail: the whole envelope, so a reader can
        // tell live work from an archived report without a second call, and the
        // id that takes delivery of the rest. `body` is deliberately absent —
        // what is here is the snippet, and read_message is how the message is
        // taken whole.
        Hit::Message { message, snippet } => serde_json::json!({
            "hit": "message",
            "id": message.id.as_str(),
            "mailbox": message.mailbox.as_str(),
            "state": message.state.as_token(),
            "sender": message.sender,
            "subject": message.subject,
            "sent_at": message.sent_at.to_string(),
            "notes": message.notes,
            "snippet": snippet,
        }),
        Hit::Prose {
            title,
            entity,
            edges,
            snippet,
            ..
        } => serde_json::json!({
            "hit": "prose",
            "title": title,
            "entity": entity.as_ref().map(entity_json),
            "edges": edges.iter().map(edge_json).collect::<Vec<_>>(),
            "snippet": snippet,
        }),
    }
}

/// **Whether this answer covered mail, and why not when it didn't.**
///
/// One shape, always present, so a caller reads it in one pass instead of
/// branching on which keys came back — the same deal `owned_mailbox` makes.
///
/// It exists because silence is a lie here. A search is a read of an in-process
/// index: if the mailbox world was unreachable when that index was built, mail
/// is simply not in it, and an answer that comes back without mail hits and
/// without a word reads as "no message says that". That is a different claim
/// from "jojobot has read no messages", and it is the one a caller acts on.
fn mail_coverage(query: &SearchQuery, coverage: Coverage) -> serde_json::Value {
    let excluded = |note: &str| serde_json::json!({ "searched": false, "note": note });
    if !query.include_mail {
        return excluded(
            "you passed include_mail: false, so messages were left out of this answer.",
        );
    }
    if query.is_fact_scoped() {
        return excluded(
            "this query filters on a property only a fact has (status, provenance, subject or \
             edge), so it is a question about facts — messages, entities and prose are all out \
             of it.",
        );
    }
    // **A `kind` filter excludes mail, silently and structurally.** A message
    // belongs to no entity, so it has no kind to match — the filter drops it
    // exactly as it drops prose in a doc that is nobody's. Saying `searched:
    // true` here was the field's one wrong answer, and a field a caller is told
    // to trust has to be right in every case rather than in most of them.
    if query.kind.is_some() {
        return excluded(
            "this query narrows to one entity kind, and a message belongs to no entity, so \
             mail was left out of it. Drop `kind` to search messages too.",
        );
    }
    match coverage {
        Coverage::Unread => excluded(
            "jojobot has not been able to read the mailbox world, so NO message is searchable \
             right now — this is not 'nothing matched'. The memory half of this answer is \
             complete. start_here's snapshot says whether the mailbox world is reachable at all.",
        ),
        // Searched, and said so — hits are real. But the board read failed, so
        // only what this server has handled since is in there, and a caller
        // hunting an older message has to be told rather than shown an empty
        // list. Reporting this as `searched: false` was an answer that carried
        // message hits and denied having searched any.
        Coverage::Partial => serde_json::json!({
            "searched": true,
            "note": "PARTIAL: jojobot could not read the mailbox world at startup, so only \
                     messages it has handled since are searchable. Any hit here is real, but an \
                     older message may be missing — this is not a complete answer over mail. \
                     start_here's snapshot says whether the mailbox world is reachable at all.",
        }),
        Coverage::Loaded => serde_json::json!({ "searched": true }),
    }
}

/// **Whether this answer covered the memory graph, and what is missing when it
/// didn't** — [`mail_coverage`]'s question, asked of the other store, in the
/// same shape and the same words.
///
/// There is no caller-side reason for a gap here: memory is searched by every
/// query, so the only way this half is incomplete is that the index is behind
/// the store — a boot scan that failed, or a document jojobot wrote and could
/// not re-read afterwards. The write landed either way. What a caller loses is
/// the guarantee that this answer reflects it, and the way back is `recall`,
/// which reads the store.
fn memory_coverage(coverage: Coverage) -> serde_json::Value {
    match coverage {
        Coverage::Unread => serde_json::json!({
            "searched": false,
            "note": "jojobot could not read the memory store at startup, so NO entity, fact or \
                     prose is searchable right now — this is not 'nothing matched'. The memory \
                     verbs are unaffected: recall reads the store directly and is complete.",
        }),
        Coverage::Partial => serde_json::json!({
            "searched": true,
            "note": "PARTIAL: at least one entity is indexed as it stood BEFORE a write that \
                     landed, because jojobot could not re-read it afterwards. Every hit here is \
                     real, but one of them may be a version the store has moved past, and a fact \
                     written since may be missing. recall reads the store directly and is \
                     complete — use it when the answer matters.",
        }),
        Coverage::Loaded => serde_json::json!({ "searched": true }),
    }
}

/// **Every note these two functions can put in an answer**, each labelled with
/// the state that produces it — the input to the surface's checks on the words
/// an agent is handed.
///
/// The `Coverage` values are written out rather than looped over a list, so a
/// state added later fails to compile here instead of quietly going unread.
#[cfg(test)]
pub(crate) fn coverage_notes() -> Vec<(String, String)> {
    let mut found = Vec::new();
    for coverage in [Coverage::Unread, Coverage::Partial, Coverage::Loaded] {
        found.push((
            format!("search's memory coverage note ({coverage:?})"),
            memory_coverage(coverage).to_string(),
        ));
        for (narrowing, query) in [
            ("unnarrowed", SearchQuery::default()),
            (
                "include_mail false",
                SearchQuery {
                    include_mail: false,
                    ..SearchQuery::default()
                },
            ),
            (
                "fact-scoped",
                SearchQuery {
                    status: Some(FactStatus::Active),
                    ..SearchQuery::default()
                },
            ),
            (
                "kind-filtered",
                SearchQuery {
                    kind: Some(EntityKind::Person),
                    ..SearchQuery::default()
                },
            ),
        ] {
            found.push((
                format!("search's mail coverage note ({narrowing}, {coverage:?})"),
                mail_coverage(&query, coverage).to_string(),
            ));
        }
    }
    found
}

/// The front door: one ranked list over entities, facts and prose.
#[tool_router(router = search_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "The front door — use it first, and any time you do not already hold the \
                       exact handle or address. One ranked list over entities, facts, free \
                       prose AND the messages in mailboxes at once. `query` is free text (ALL \
                       words must match) and is optional when a filter narrows it: kind · status \
                       (default active; superseded is excluded unless named) · provenance · \
                       subject · edge {shape, object} · include_mail; a call with neither query \
                       nor filter is refused. kind + edge answers a cross-entity question in one \
                       call (\"which people are in X\") by walking typed edges — prose that \
                       merely mentions X is not an answer. No hit comes back bare: a fact \
                       carries the whole claim, its address (feed that to update_fact), and who it \
                       is `about` and where it is `home`d (a null name there means the handle \
                       names nothing — a real defect worth reporting); an entity or prose hit \
                       carries that entity's names and the edges its facts draw; a message hit \
                       carries its box, its state (new/read/processed — an archived report is \
                       findable, and the state is how you tell it from live work), its sender \
                       and the id read_message takes, plus a snippet rather than the whole body. \
                       Mail is searched by default — pass include_mail: false to leave session \
                       traffic out, and note that a `kind` filter also leaves it out, since a \
                       message belongs to no entity and so has no kind to match. ALWAYS read the \
                       `mail` field of the answer, in BOTH directions: searched: false means no \
                       message was searched at all, which is not the same as nothing matching; \
                       and searched: true can still be partial after a degraded start, where the \
                       hits are real but anything older than this server's start is missing. \
                       Whenever `mail` carries a `note`, that note says which case you are in — \
                       read it before concluding a message does not exist. `memory` answers the \
                       same question about entities, facts and prose: searched: false means the \
                       memory store was never read, and searched: true with a note means at least \
                       one entity is indexed as it stood before a write that landed — the hits \
                       are real, one may be out of date, and `recall` reads the store itself. No \
                       pagination — raise `limit` or ask a better question."
    )]
    pub(crate) async fn search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let edge = args
            .edge
            .as_ref()
            .map(|e| -> Result<EdgeFilter, McpError> {
                Ok(EdgeFilter {
                    shape: e.shape.as_deref().map(parse_shape).transpose()?,
                    object: EntityId(e.object.trim().to_string()),
                })
            })
            .transpose()?;
        let query = SearchQuery {
            text: args.query,
            kind: args.kind.as_deref().map(parse_kind).transpose()?,
            status: args.status.as_deref().map(parse_status).transpose()?,
            provenance: args
                .provenance
                .as_deref()
                .map(parse_one_provenance)
                .transpose()?,
            subject: args.subject.as_deref().map(EntityId::person),
            edge,
            include_mail: args.include_mail.unwrap_or(true),
            limit: args.limit.map_or(DEFAULT_LIMIT, |l| l as usize),
        };
        // Checked here as well as in the index: a malformed query is the caller's
        // mistake, and it should read as one no matter which adapter is behind us.
        query.validate().map_err(memory_error)?;
        let hits = self.search.search(&query).map_err(memory_error)?;
        let body = serde_json::json!({
            "count": hits.len(),
            "memory": memory_coverage(self.search.memory_coverage()),
            "mail": mail_coverage(&query, self.search.mail_coverage()),
            "results": hits.iter().map(hit_json).collect::<Vec<_>>(),
        });
        json_result(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;
    use crate::orientation::essay::ORIENTATION;
    use jojobot_domain::memory::{Boot, FactId};

    /// Every argument reaches the port as the typed query it means — including the
    /// edge filter, which is the whole point of the verb.
    #[tokio::test]
    async fn search_translates_every_argument_into_the_query() {
        let spy = Arc::new(SpySearch::default());
        let jojobot = handler_with(spy.clone());
        jojobot
            .search(Parameters(SearchArgs {
                query: Some("winter".into()),
                kind: Some("person".into()),
                status: Some("superseded".into()),
                provenance: Some("testimony".into()),
                subject: Some("person:alpha".into()),
                edge: Some(EdgeFilterArgs {
                    shape: Some("location".into()),
                    object: "place:shelbyville".into(),
                }),
                include_mail: Some(false),
                limit: Some(5),
                sid: None,
            }))
            .await
            .expect("search ok");

        let query = spy.query();
        assert_eq!(query.terms(), Some("winter"));
        assert!(
            !query.include_mail,
            "the caller's exclusion must reach the port"
        );
        assert_eq!(query.kind, Some(EntityKind::Person));
        assert_eq!(query.status, Some(FactStatus::Superseded));
        assert_eq!(query.provenance, Some(Provenance::Testimony));
        assert_eq!(
            query.subject.as_ref().map(|s| s.as_str()),
            Some("person:alpha")
        );
        let edge = query
            .edge
            .expect("the edge filter must survive translation");
        assert_eq!(edge.shape, Some(EdgeShape::Location));
        assert_eq!(edge.object.as_str(), "place:shelbyville");
        assert_eq!(query.limit, 5);
    }

    /// An edge filter with no shape means any edge pointing at the object, and the
    /// limit defaults to twenty.
    #[tokio::test]
    async fn a_shapeless_edge_filter_and_the_default_limit_reach_the_port() {
        let spy = Arc::new(SpySearch::default());
        handler_with(spy.clone())
            .search(Parameters(SearchArgs {
                edge: Some(EdgeFilterArgs {
                    shape: None,
                    object: "event:winter-fest".into(),
                }),
                ..search_args()
            }))
            .await
            .expect("search ok");
        let query = spy.query();
        assert_eq!(query.edge.as_ref().map(|e| e.shape), Some(None));
        assert_eq!(query.limit, DEFAULT_LIMIT);
    }

    /// Neither text nor a filter is a request for everything, which is not a
    /// search — and it is the caller's mistake, whatever adapter is behind us.
    #[tokio::test]
    async fn search_with_neither_text_nor_a_filter_is_a_client_error() {
        let err = handler()
            .search(Parameters(search_args()))
            .await
            .expect_err("an unbounded search must be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    /// Bad tokens are client errors, not silent fallbacks: a mistyped `status`
    /// that quietly became `active` would answer a question about superseded
    /// rows with the live ones and look like a straight answer.
    ///
    /// **Every case carries query text**, so the refusal can only be the bad
    /// token. Without it, an implementation that dropped the filter entirely
    /// would still error — as an unbounded search — and this would pass green
    /// over a `search` that ignored its filters.
    #[tokio::test]
    async fn malformed_search_filters_are_client_errors() {
        let jojobot = handler();
        let searching = || SearchArgs {
            query: Some("winter".into()),
            ..search_args()
        };
        let bad = [
            SearchArgs {
                kind: Some("receipt".into()),
                ..searching()
            },
            SearchArgs {
                status: Some("retired".into()),
                ..searching()
            },
            SearchArgs {
                provenance: Some("maybe".into()),
                ..searching()
            },
            // A *bare* subject is read as a person, as everywhere else — so the
            // malformed case is one that can't be an id at all.
            SearchArgs {
                subject: Some("person:a|b".into()),
                ..searching()
            },
            SearchArgs {
                edge: Some(EdgeFilterArgs {
                    shape: Some("knows".into()),
                    object: "place:x".into(),
                }),
                ..searching()
            },
            SearchArgs {
                edge: Some(EdgeFilterArgs {
                    shape: None,
                    object: "place:a|b".into(),
                }),
                ..searching()
            },
            SearchArgs {
                limit: Some(0),
                ..searching()
            },
        ];
        for args in bad {
            let err = jojobot
                .search(Parameters(args))
                .await
                .expect_err("a malformed filter must be refused");
            assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        }
    }

    /// **Mail comes back in the one list, and unmistakably as mail.** A message
    /// hit says which box, which state, who sent it, and the id `read_message`
    /// takes — without those it is an anonymous paragraph and a reader cannot
    /// tell a live task from an archived report. The body is a snippet: taking
    /// the whole message is `read_message`'s job, and that is a deliberate act.
    #[tokio::test]
    async fn a_message_hit_arrives_with_its_whole_envelope() {
        let spy = Arc::new(SpySearch::answering(vec![Hit::Message {
            message: Message {
                id: MessageId("42".into()),
                mailbox: MailboxName("pm".into()),
                body: "The kiln rebuild landed; the damper is still hand-cut.".into(),
                subject: Some("the kiln slice".into()),
                sender: "dev (implementer)".into(),
                sent_at: jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant"),
                state: mailbox::MessageState::Processed,
                notes: Some("filed".into()),
                in_reply_to: None,
            },
            snippet: "…the damper is still hand-cut…".into(),
        }]));

        let body = json_of(
            &handler_with(spy)
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        let hit = &body["results"][0];
        assert_eq!(
            hit["hit"], "message",
            "a caller must not have to guess from the shape"
        );
        assert_eq!(hit["id"], "42", "the id read_message takes");
        assert_eq!(hit["mailbox"], "pm");
        assert_eq!(hit["state"], "processed", "an archive reads as one");
        assert_eq!(hit["sender"], "dev (implementer)");
        assert_eq!(hit["subject"], "the kiln slice");
        assert_eq!(hit["notes"], "filed");
        assert!(hit["sent_at"].is_string());
        assert_eq!(hit["snippet"], "…the damper is still hand-cut…");
        assert!(
            hit["body"].is_null(),
            "the whole body is read_message's to hand over, not a hit's: {hit}"
        );
        assert_eq!(body["mail"]["searched"], true);
    }

    /// **A search that could not see mail says so.** Coming back without mail
    /// hits and without a word reads as "no message says that", which is a
    /// different claim from "jojobot has read no messages" — and it is the one a
    /// caller acts on. The memory half is unaffected: degrade, don't error.
    #[tokio::test]
    async fn a_search_says_when_no_message_was_searched_at_all() {
        let body = json_of(
            &handler_with(Arc::new(SpySearch::with_no_mail_indexed()))
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    ..search_args()
                }))
                .await
                .expect("a down mailbox world must not break search"),
        );
        assert_eq!(body["mail"]["searched"], false);
        let note = body["mail"]["note"].as_str().expect("an absence says why");
        assert!(
            note.contains("not 'nothing matched'"),
            "the note has to draw the distinction it exists for: {note}"
        );

        // The caller's own exclusion is a different absence, and says so.
        let excluded = json_of(
            &handler_with(Arc::new(SpySearch::default()))
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    include_mail: Some(false),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(excluded["mail"]["searched"], false);
        assert!(
            excluded["mail"]["note"]
                .as_str()
                .expect("a note")
                .contains("include_mail"),
            "an exclusion the caller asked for must not read as an outage: {excluded}"
        );

        // …and so is a query that is about facts to begin with.
        let fact_scoped = json_of(
            &handler_with(Arc::new(SpySearch::default()))
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    provenance: Some("testimony".into()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(fact_scoped["mail"]["searched"], false);
        assert!(
            fact_scoped["mail"]["note"]
                .as_str()
                .expect("a note")
                .contains("only a fact has"),
            "got {fact_scoped}"
        );
    }

    /// **THE INVARIANT: no answer both returns a message hit and claims no
    /// message was searched.** After a failed boot board read, every verb still
    /// indexes the messages it touches and search still returns them — while the
    /// coverage flag stayed false for the life of the process. One answer said
    /// both things at once, and a caller reading the field it is told to trust
    /// would discard a hit that is real.
    ///
    /// The fix is a third state, not a flipped flag: hits are real, but the
    /// board was never read, so anything older than this process is missing —
    /// which a caller hunting an old message has to be told rather than shown an
    /// empty list.
    #[tokio::test]
    async fn an_answer_carrying_a_message_never_claims_no_mail_was_searched() {
        let hit = || {
            vec![Hit::Message {
                message: Message {
                    id: MessageId("42".into()),
                    mailbox: MailboxName("pm".into()),
                    body: "the damper is still hand-cut".into(),
                    subject: None,
                    sender: "dev".into(),
                    sent_at: jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant"),
                    state: mailbox::MessageState::New,
                    notes: None,
                    in_reply_to: None,
                },
                snippet: "…the damper…".into(),
            }]
        };

        for coverage in [Coverage::Partial, Coverage::Loaded] {
            let body = json_of(
                &handler_with(Arc::new(SpySearch::covering(coverage, hit())))
                    .search(Parameters(SearchArgs {
                        query: Some("damper".into()),
                        ..search_args()
                    }))
                    .await
                    .expect("search ok"),
            );
            assert!(
                body["results"]
                    .as_array()
                    .expect("results")
                    .iter()
                    .any(|h| h["hit"] == "message"),
                "the double answered with a message: {body}"
            );
            assert_eq!(
                body["mail"]["searched"], true,
                "an answer carrying a message hit cannot claim no message was searched \
                 ({coverage:?}): {body}"
            );
        }

        // …and the degraded one still says it is degraded, or the caller reads a
        // partial answer over mail as a complete one.
        let partial = json_of(
            &handler_with(Arc::new(SpySearch::covering(Coverage::Partial, hit())))
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert!(
            partial["mail"]["note"]
                .as_str()
                .expect("a partial answer says it is partial")
                .contains("PARTIAL"),
            "got {partial}"
        );
    }

    /// **An answer served from a memory half that is behind says so.**
    ///
    /// The mail half has said this since it shipped; the memory half never
    /// learned to, so a `search` answered out of an index holding the version
    /// before somebody's write read exactly like a complete one. Honesty to the
    /// caller who wrote does nothing for the sessions that read afterwards, and
    /// those are the ones with no way to tell.
    #[tokio::test]
    async fn an_answer_from_a_memory_half_that_is_behind_says_so() {
        let hit = || {
            vec![Hit::Entity {
                entity: Entity {
                    id: EntityId("person:alpha".into()),
                    kind: EntityKind::Person,
                    name: "Alpha".into(),
                    aliases: Vec::new(),
                    source: "user-named".into(),
                    crm: None,
                    parent: None,
                    boot: Boot::OnDemand,
                },
                doc_id: "doc-9".into(),
                edges: Vec::new(),
            }]
        };

        let complete = json_of(
            &handler_with(Arc::new(SpySearch::over_memory(Coverage::Loaded, hit())))
                .search(Parameters(SearchArgs {
                    query: Some("alpha".into()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(
            complete["memory"]["searched"], true,
            "a loaded half searched everything: {complete}"
        );
        assert!(
            complete["memory"]["note"].is_null(),
            "and has nothing to warn about: {complete}"
        );

        let behind = json_of(
            &handler_with(Arc::new(SpySearch::over_memory(Coverage::Partial, hit())))
                .search(Parameters(SearchArgs {
                    query: Some("alpha".into()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        // Searched, and said so — the hits are real. The claim is that something
        // is MISSING, which is a different thing from nothing being searched, and
        // collapsing the two would make an answer carry hits and deny searching.
        assert_eq!(
            behind["memory"]["searched"], true,
            "partial is not empty: {behind}"
        );
        assert!(
            behind["memory"]["note"]
                .as_str()
                .expect("a partial answer says it is partial")
                .contains("PARTIAL"),
            "got {behind}"
        );

        let unread = json_of(
            &handler_with(Arc::new(SpySearch::over_memory(
                Coverage::Unread,
                Vec::new(),
            )))
            .search(Parameters(SearchArgs {
                query: Some("alpha".into()),
                ..search_args()
            }))
            .await
            .expect("search ok"),
        );
        assert_eq!(
            unread["memory"]["searched"], false,
            "an index that never read the store searched nothing: {unread}"
        );
        assert!(
            unread["memory"]["note"]
                .as_str()
                .expect("an absence says why")
                .contains("recall"),
            "…and names the verb that reads the store instead: {unread}"
        );
    }

    /// **A `kind` filter excludes every message, and the answer has to say so.**
    /// The exclusion is structural and silent — a message doc carries no `kind`
    /// field, so the filter's own MUST clause drops it, exactly as it drops
    /// prose in nobody's doc. The coverage block knew three reasons and not this
    /// one, so `kind`-filtered answers claimed `searched: true` while the tool
    /// description tells a caller to trust that field. A field worth reading is
    /// a field that has to be right in every case, not in most of them.
    #[tokio::test]
    async fn a_kind_filter_reports_that_mail_was_left_out() {
        let body = json_of(
            &handler_with(Arc::new(SpySearch::default()))
                .search(Parameters(SearchArgs {
                    query: Some("damper".into()),
                    kind: Some("person".into()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(
            body["mail"]["searched"], false,
            "a kind filter leaves no message in the answer, so it cannot claim it searched them"
        );
        let note = body["mail"]["note"].as_str().expect("an absence says why");
        assert!(
            note.contains("kind"),
            "…and it says which filter did it, since the caller can drop that one: {note}"
        );

        // The tool description makes the same promise, so it names this case too.
        let tools = Jojobot::tool_router().list_all();
        let description = tools
            .iter()
            .find(|t| t.name == "search")
            .expect("search is a tool")
            .description
            .as_deref()
            .unwrap_or_default();
        assert!(
            description.contains("kind") && description.contains("mail"),
            "the description tells a caller kind and mail interact: {description}"
        );
    }

    /// `search`'s description must never claim mail is unreachable from
    /// here — that is false, and it would send a caller to a second verb
    /// that does not exist. Pinned rather than fixed once, because the
    /// sentence is exactly the kind that survives a rewrite by being
    /// plausible.
    #[test]
    fn the_search_description_no_longer_says_mail_is_unsearchable() {
        let tools = Jojobot::tool_router().list_all();
        let search = tools
            .iter()
            .find(|t| t.name == "search")
            .expect("search is a tool");

        // **All three surfaces, not the one that was noticed.** The claim was
        // written down in three places — the tool description, the orientation
        // `start_here` hands over, and the server instructions every
        // client loads before it calls anything — and fixing one leaves a
        // session reading either of the others exactly as misinformed as before.
        let instructions = handler().get_info().instructions.unwrap_or_default();
        for (surface, text) in [
            (
                "the search description",
                search.description.as_deref().unwrap_or_default(),
            ),
            ("the orientation", ORIENTATION),
            ("the server instructions", instructions.as_str()),
        ] {
            for stale in [
                "Messages and mailboxes are not searchable",
                "not searchable here",
                "sees memory only",
                "never messages",
            ] {
                assert!(
                    !text.contains(stale),
                    "{surface} still claims mail is out of reach ({stale:?})"
                );
            }
            assert!(
                text.contains("searchable") || text.contains("include_mail"),
                "{surface} has to say that mail IS reachable — silence reads as the old claim"
            );
        }
        assert!(
            search
                .description
                .as_deref()
                .unwrap_or_default()
                .contains("include_mail"),
            "…and the description has to name the parameter that takes mail back out"
        );
    }

    /// **One list, every hit typed — and none of them bare.** An entity, a fact
    /// and a prose match come back together, each saying what it is, carrying
    /// what makes it actionable, *and* carrying its surroundings: the fact names
    /// the entities it is about and sits on, the entity and the prose doc carry
    /// the edges that place them in the graph.
    #[tokio::test]
    async fn search_renders_a_mixed_list_of_typed_hits() {
        let entity = Entity {
            id: EntityId::new(EntityKind::Work, "first-mix"),
            kind: EntityKind::Work,
            name: "First Mix".into(),
            aliases: vec!["The First One".into()],
            source: "user-named".into(),
            crm: None,
            parent: None,
            boot: Boot::OnDemand,
        };
        let fact = Fact {
            id: FactId("f3".into()),
            home: EntityId::person("alpha"),
            subject: EntityId::person("alpha"),
            content: "spending the winter away".into(),
            details: Some("said so in June".into()),
            provenance: Provenance::Testimony,
            standing: Standing::Open,
            status: FactStatus::Active,
            date: jiff::civil::date(2026, 7, 1),
            edge: Some(Edge::new(
                EdgeShape::Membership,
                EntityId("org:guild".into()),
            )),
            event: None,
            derived_from: None,
        };
        let alpha = Entity {
            id: EntityId::person("alpha"),
            kind: EntityKind::Person,
            name: "Alpha".into(),
            aliases: vec!["Al".into()],
            source: "user-named".into(),
            crm: None,
            parent: None,
            boot: Boot::OnDemand,
        };
        let guild = Edge::new(EdgeShape::Membership, EntityId("org:guild".into()));
        let spy = Arc::new(SpySearch::answering(vec![
            Hit::Entity {
                entity,
                doc_id: "doc-9".into(),
                edges: vec![guild.clone()],
            },
            Hit::Fact {
                fact,
                subject: EntityRef::resolved(&alpha),
                home: EntityRef::resolved(&alpha),
            },
            Hit::Prose {
                doc_id: "doc-1".into(),
                title: "Alpha".into(),
                entity: Some(alpha.clone()),
                edges: vec![guild],
                snippet: "…allergic to penicillin…".into(),
            },
        ]));

        let body = json_of(
            &handler_with(spy)
                .search(Parameters(SearchArgs {
                    query: Some("winter".into()),
                    ..search_args()
                }))
                .await
                .expect("search ok"),
        );
        assert_eq!(body["count"], 3);
        let results = body["results"].as_array().expect("a list of results");

        assert_eq!(results[0]["hit"], "entity");
        assert_eq!(results[0]["id"], "work:first-mix");
        assert_eq!(results[0]["type"], "CreativeWork", "the schema.org name");
        // **And no id of the thing it was stored in.** It was the one place a
        // caller could learn that an entity has a document behind it, and it
        // addressed nothing — every verb here takes a handle or a fact address.
        assert!(
            results[0]["doc"].is_null(),
            "an entity hit says where it sits in the graph, never where it sits in a store"
        );
        assert_eq!(
            results[0]["edges"][0]["type"], "memberOf",
            "where it sits in the graph"
        );
        assert_eq!(results[0]["edges"][0]["object"], "org:guild");

        assert_eq!(results[1]["hit"], "fact");
        assert_eq!(
            results[1]["address"], "person:alpha#f3",
            "a fact hit is editable"
        );
        assert_eq!(
            results[1]["subject"], "person:alpha",
            "the row keeps one spelling across capture, recall and search"
        );
        assert_eq!(results[1]["content"], "spending the winter away");
        assert_eq!(results[1]["details"], "said so in June");
        assert_eq!(results[1]["provenance"], "testimony");
        assert_eq!(results[1]["status"], "active");
        assert_eq!(results[1]["date"], "2026-07-01");
        assert_eq!(results[1]["edge"]["type"], "memberOf");
        assert_eq!(results[1]["edge"]["object"], "org:guild");
        // …and the surroundings, resolved: who this is about, and whose page it
        // sits on. A handle alone costs the reader a call to find out.
        assert_eq!(results[1]["about"]["id"], "person:alpha");
        assert_eq!(results[1]["about"]["type"], "Person");
        assert_eq!(results[1]["about"]["name"], "Alpha");
        assert_eq!(results[1]["home"]["id"], "person:alpha");
        assert_eq!(results[1]["home"]["name"], "Alpha");
        // …under the same key an entity hit uses, so one shape means one thing.
        assert_eq!(
            results[1]["about"]["alternateName"][0], "Al",
            "a search on the nickname has to show the linkage on the hit itself"
        );
        assert_eq!(results[1]["home"]["alternateName"][0], "Al");

        assert_eq!(results[2]["hit"], "prose");
        assert!(
            results[2]["doc"].is_null(),
            "…and neither does a stretch of prose: what a caller can act on is its entity"
        );
        assert_eq!(results[2]["title"], "Alpha");
        assert_eq!(results[2]["entity"]["id"], "person:alpha");
        assert_eq!(results[2]["entity"]["name"], "Alpha");
        assert_eq!(
            results[2]["entity"]["alternateName"][0], "Al",
            "the names it answers to come with it"
        );
        assert_eq!(results[2]["edges"][0]["object"], "org:guild");
        assert_eq!(results[2]["snippet"], "…allergic to penicillin…");
    }
}
