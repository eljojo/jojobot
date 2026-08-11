//! `recall` — The graph query: say the shape you want, and get that shape back.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.
//!
//! **It answers with objects, always** — one when a handle was named, several
//! when a filter chose them. A verb whose answer changed shape with its
//! arguments would make every caller branch on which question they had asked.

use super::*;
use jojobot_domain::memory::graph;

/// One key filter of a `recall` — a key, and optionally the value it holds.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct KeyFilterArgs {
    /// The key a record carries, exactly as the record spells it. Matching is
    /// structural, so nothing has to have declared it.
    pub key: String,
    /// The value it must hold. **Omit it to ask only that the key is there** —
    /// a different question, and the one to ask when you want everything that
    /// records a thing rather than everything that records it one way.
    #[serde(default)]
    pub value: Option<String>,
    /// **How the value is compared**, and what a key was DECLARED to hold is
    /// what licenses it. `equals` is the default and needs no declaration.
    /// `before` and `after` need a type declaring the key a `date`; `less` and
    /// `greater` need one declaring it a `number`. Asking for an ordering a
    /// declaration does not license comes back blocked, rather than quietly
    /// answering the equality question instead.
    #[serde(default)]
    pub compare: Option<String>,
}

/// The `follow` argument of a `recall` — which edges to walk, and how far.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FollowArgs {
    /// Narrow to one shape (`location` · `membership` · `attendance` · `about`
    /// · `connection`). Omit for **any** edge — "whatever it is connected to".
    #[serde(default)]
    pub shape: Option<String>,
    /// **A declared relation to walk instead of an edge.** A key some type
    /// declared to hold a `reference` is a link, because the declaration says
    /// the value is another entity rather than a string that looks like one.
    ///
    /// **The name says which way it goes**, so do not pass a direction beside
    /// it. Forward is the key itself — from a record, `owner` reaches the
    /// person it points at. Reverse is `type.key` — from that person,
    /// `pet.owner` reaches every pet pointing back, which is the has-many and
    /// which nobody declares an inverse for. The reverse carries its type
    /// because one type may declare two reference keys onto the same kind.
    #[serde(default)]
    pub relation: Option<String>,
    /// **Which end of the edge to leave by**, and it is two different
    /// questions. `out` (the default) follows the edges this object's own
    /// records draw — from a guest, the party they are attending. `in` follows
    /// the edges other objects draw AT this one — from the party, its guests.
    #[serde(default)]
    pub direction: Option<String>,
    /// How many hops: 1 is the neighbours, 2 is the neighbours' neighbours.
    /// Defaults to 1.
    #[serde(default)]
    pub depth: Option<u32>,
    /// **What the walk keeps of what it reaches.** The same key filters the
    /// selection takes, applied at every hop rather than to the roots — so
    /// "this person's pets" narrows to "this person's pets born before a date".
    /// Omit to keep everything. An object kept this way arrives carrying the
    /// records that answered, and one that was reached and not kept leaves the
    /// object that points at it marked as having edges nobody followed.
    #[serde(default)]
    pub keeping: Option<Vec<KeyFilterArgs>>,
}

/// Arguments to `recall`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RecallArgs {
    /// One entity, as `kind:slug` (a bare handle is read as a person).
    ///
    /// **A named subject always comes back**, even when the filters below keep
    /// none of its records: naming a handle asks for that object, and a filter
    /// asks which objects. A handle that names nothing comes back blocked with
    /// the nearest handles, never as an empty answer.
    #[serde(default)]
    pub subject: Option<String>,
    /// Every entity of one kind.
    #[serde(default)]
    pub kind: Option<String>,
    /// Objects holding a record that answers this type, by name. Matching is
    /// STRUCTURAL — a record carrying the type's keys answers it whether or not
    /// anybody declared it one. A name no type answers to comes back blocked,
    /// naming the types that do exist.
    #[serde(default)]
    pub answers_type: Option<String>,
    /// Objects holding a record that carries these keys, and the values named.
    /// **Every filter must hold on ONE record**: two filters describe a single
    /// record, not two separate questions.
    #[serde(default)]
    pub fields: Option<Vec<KeyFilterArgs>>,
    /// Whether each object's facts come back. **True by default.** Turn it off
    /// when you want the shape of the graph and not its contents.
    #[serde(default)]
    pub facts: Option<bool>,
    /// Whether each object's **prose** comes back — the human half of its page,
    /// whole. Off by default, because a page is bigger than a claim and shipping
    /// every one of them unasked is a cost the caller cannot decline.
    #[serde(default)]
    pub prose: Option<bool>,
    /// Which edges to walk. Omit to walk none, and the answer is flat.
    #[serde(default)]
    pub follow: Option<FollowArgs>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// The key filters of a call, wherever they sit: a selection describes the
/// roots and a walk's describe what it reaches, and they are the same filter.
fn key_filters(args: &[KeyFilterArgs]) -> Result<Vec<graph::FieldFilter>, McpError> {
    args.iter()
        .map(|f| {
            Ok(graph::FieldFilter {
                key: f.key.trim().to_string(),
                value: f.value.as_ref().map(|v| v.trim().to_string()),
                compare: parse_compare(f.compare.as_deref())?,
            })
        })
        .collect()
}

/// One object on the wire, and everything it reached.
///
/// **Absence means "not asked for"** on both halves that can be turned off:
/// `facts` and `prose` are missing when the caller declined them, rather than
/// rendered empty, so nothing has to tell an empty page from a page nobody
/// wanted.
fn object_json(object: &graph::Object, include: graph::Include) -> serde_json::Value {
    let mut body = entity_json(&object.entity);
    let Some(fields) = body.as_object_mut() else {
        return body;
    };
    if include.facts {
        fields.insert("facts".into(), object.facts.iter().map(fact_json).collect());
    }
    if let Some(prose) = object.prose.as_ref() {
        fields.insert("prose".into(), prose.as_str().into());
    }
    // How the walk got here. Absent on a root, which nothing reached.
    //
    // An edge names its shape and a relation names itself, under different
    // keys: they are different things, and one key holding either would leave
    // a reader to work out which by looking at the value.
    if let Some(via) = object.via.as_ref() {
        let link = match &via.link {
            graph::Link::Edge(shape) => serde_json::json!({ "type": shape.as_name() }),
            graph::Link::Relation(name) => serde_json::json!({ "relation": name }),
        };
        let mut link = link;
        if let Some(fields) = link.as_object_mut() {
            fields.insert("direction".into(), via.direction.as_token().into());
        }
        fields.insert("via".into(), link);
    }
    fields.insert(
        "connected".into(),
        object
            .connected
            .iter()
            .map(|o| object_json(o, include))
            .collect(),
    );
    // **Eliding is never silent.** An empty `connected` otherwise means both
    // "the walk stopped here" and "there is nothing there", and the note says
    // which as well as how to get the rest.
    if object.unwalked {
        fields.insert(
            "unwalked".into(),
            "this object draws edges the walk did not follow — recall it by handle, or ask again \
             with a deeper follow"
                .into(),
        );
    }
    body
}

/// The graph query — objects, what is on them, and what they reach.
#[tool_router(router = recall_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "The graph query: say the shape you want and get that shape back. Use it \
                       when you can describe what you are after — a handle, a kind, a type, a key \
                       and its value — and use search when you are looking for something and only \
                       have words for it. Three axes, and they COMBINE into one question rather \
                       than three. WHICH OBJECTS: subject (one handle), kind, answers_type \
                       (structural — a record answers a type by the keys it carries, whether or \
                       not anybody declared it one), and fields (a key, and the value it holds; \
                       omit the value to ask only that the key is there). WHAT OF EACH: facts, on \
                       by default, each carrying the address that makes it editable through \
                       update_fact; and prose, off by default, which is the human half of the \
                       object's page, whole. WHICH EDGES: follow {shape, direction, depth}, and \
                       THE ANSWER NESTS — a walked object carries the objects it reached, each \
                       carrying its own. Direction is two different questions: `out` follows the \
                       edges this object's records draw, `in` follows the edges drawn AT it, so \
                       from a party `in` reaches its guests and from a guest `out` reaches the \
                       party. A RELATION is the other kind of link, and a DECLARATION is what \
                       makes one: a key some type declared to hold a `reference` points at \
                       another entity, so it is walkable. Its NAME says which way it goes and you \
                       pass no direction beside it — forward is the key ('owner' reaches the \
                       person a record points at), reverse is `type.key` ('pet.owner' reaches \
                       every pet pointing back, which is the has-many, and nobody declares an \
                       inverse). `keeping` narrows what the walk reaches, taking the same key \
                       filters, so 'this person's pets' becomes 'this person's pets born before a \
                       date'. A key's DECLARED value type also licenses how you compare it: \
                       `before` and `after` on a declared date, `less` and `greater` on a \
                       declared number, `equals` always and on anything. That is what declaring a \
                       type buys you — reach — and nothing is gated by it: an undeclared record \
                       is still found by the keys it carries. That nesting is what a flat list cannot express: one call answers \
                       'these objects, and what each of them is connected to' instead of one call \
                       per object. Unlike search this returns claims of EVERY status, superseded \
                       included. A named subject always comes back, even when the filters keep \
                       none of its records — naming a handle asks for that object, a filter asks \
                       which objects — while a handle that names nothing comes back blocked with \
                       the nearest handles, never as an empty answer. An object that draws edges \
                       the walk did not follow says so; an empty `connected` with no such note is \
                       an object with nothing beyond it. A query that narrows nothing is refused: \
                       name at least one of subject, kind, answers_type or fields."
    )]
    pub(crate) async fn recall(
        &self,
        Parameters(args): Parameters<RecallArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Resolved before the read runs — see [`Jojobot::attributable`]. This
        // verb publishes a `sid` and says it is what tells jojobot who is
        // asking, so a handle that addresses nothing is refused rather than
        // dropped.
        if let Err(refused) = self.attributable(args.sid.as_deref()) {
            return Ok(refused);
        }
        // **The name is resolved to its declaration here, once**, exactly as
        // `search` resolves it: everything below takes the keys rather than
        // the name, and a name nobody declared is answered where the roster to
        // offer instead is in reach.
        let answers_type = match &args.answers_type {
            None => None,
            Some(wanted) => match self.declared(wanted, "recalled").await {
                Ok(declared) => Some(declared),
                Err(refused) => return Ok(refused),
            },
        };
        // **Two link vocabularies, and a call names one of them.** An edge
        // shape and a relation describe different things — a link nobody typed,
        // and a link a declaration made — so a call carrying both is refused
        // rather than one of them being picked.
        if let Some(f) = args.follow.as_ref()
            && f.shape.is_some()
            && f.relation.is_some()
        {
            return memory_declined(
                "recall",
                MemoryError::InvalidQuery(
                    "follow a shape or a relation, not both: a shape is one of the five names for \
                     a link nobody typed, and a relation is a key some type declared to hold a \
                     reference"
                        .into(),
                ),
            );
        }
        let follow = args
            .follow
            .as_ref()
            .map(|f| -> Result<graph::Follow, McpError> {
                Ok(graph::Follow {
                    along: match (&f.relation, &f.shape) {
                        (Some(relation), _) => graph::Along::Relation(relation.trim().to_string()),
                        (None, Some(shape)) => graph::Along::Edge(parse_shape(shape)?),
                        (None, None) => graph::Along::AnyEdge,
                    },
                    // Kept as an Option all the way down, because a relation
                    // already says which way it goes and the domain refuses the
                    // pair — which it can only do if "unset" and "out" are
                    // still telling apart by the time it looks.
                    direction: f
                        .direction
                        .as_deref()
                        .map(|d| parse_direction(Some(d)))
                        .transpose()?,
                    depth: f.depth.map_or(1, |d| d as usize),
                    keeping: key_filters(f.keeping.as_deref().unwrap_or_default())?,
                })
            })
            .transpose()?;
        let include = graph::Include {
            facts: args.facts.unwrap_or(true),
            prose: args.prose.unwrap_or(false),
        };
        let query = graph::GraphQuery {
            select: graph::Selection {
                subject: args.subject.as_deref().map(EntityId::person),
                kind: args.kind.as_deref().map(parse_kind).transpose()?,
                answers_type,
                fields: key_filters(args.fields.as_deref().unwrap_or_default())?,
            },
            include,
            follow,
        };

        let found = match graph::walk(self.memory.as_ref(), &query).await {
            Ok(found) => found,
            Err(e) => return memory_declined("recall", e),
        };
        let body = serde_json::json!({
            "count": found.len(),
            "objects": found
                .iter()
                .map(|o| object_json(o, include))
                .collect::<Vec<_>>(),
        });
        json_result(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    /// The whole-page query, spelled once: one handle, facts and nothing else.
    fn of(subject: &str) -> RecallArgs {
        RecallArgs {
            subject: Some(subject.into()),
            kind: None,
            answers_type: None,
            fields: None,
            facts: None,
            prose: None,
            follow: None,
            sid: None,
        }
    }

    /// Every recalled fact carries its address, and that address is what
    /// `update_fact` takes — the pairing that makes editing possible.
    #[tokio::test]
    async fn recall_returns_addresses_that_update_fact_accepts() {
        let jojobot = handler();
        capture_ok(&jojobot, capture_args("alpha", "works at the old place")).await;

        let body = json_of(
            &jojobot
                .recall(Parameters(of("alpha")))
                .await
                .expect("recall ok"),
        );
        let address = body["objects"][0]["facts"][0]["address"]
            .as_str()
            .expect("every fact carries an address");
        assert_eq!(address, "person:alpha#f1");

        let updated = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("works at the new place".into()),
                    details: Some("changed jobs in July".into()),
                    ..update_args(address)
                }))
                .await
                .expect("update ok"),
        );
        assert_eq!(updated["content"], "works at the new place");
        assert_eq!(updated["details"], "changed jobs in July");
        assert_eq!(updated["address"], "person:alpha#f1");
    }

    /// **An unknown handle is a miss at the wire too.** The production smoke
    /// test asked for a nonexistent person and was told "reads fine, no facts"
    /// — the same answer an empty page gives, so a caller can never repair a
    /// bad handle. The miss now comes back as an error naming the handle and
    /// its near candidates, while an empty-but-real entity still reads fine.
    #[tokio::test]
    async fn recall_of_an_unknown_entity_is_a_miss_with_candidates() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "zenith", "Zenith")))
            .await
            .expect("add ok");

        let missed = blocked(
            &jojobot
                .recall(Parameters(of("person:zenit")))
                .await
                .expect("a handle that names nothing is an answer, not a protocol failure"),
        );
        assert_eq!(missed["attempted"], "person:zenit");
        assert_eq!(
            missed["candidates"][0]["handle"], "person:zenith",
            "the near candidate surfaces: {missed}"
        );

        let body = json_of(
            &jojobot
                .recall(Parameters(of("person:zenith")))
                .await
                .expect("an existing entity's empty page still reads"),
        );
        assert_eq!(
            body["objects"][0]["facts"]
                .as_array()
                .expect("a list")
                .len(),
            0
        );
    }

    /// **`recall` shows the edges too.** Search grew a neighborhood; a recall
    /// that answered with the same rows stripped of their edges would make the
    /// graph a thing you can only see by searching for it, and reading an
    /// entity's own page is the commonest way anyone looks.
    #[tokio::test]
    async fn recall_returns_the_edge_a_fact_draws() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("org", "guild", "The Guild")))
            .await
            .expect("add_entity ok");
        capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some("membership".into()),
                object: Some("org:guild".into()),
                ..capture_args("alpha", "joined in the spring")
            },
        )
        .await;

        let body = json_of(
            &jojobot
                .recall(Parameters(of("alpha")))
                .await
                .expect("recall ok"),
        );
        let edged = body["objects"][0]["facts"]
            .as_array()
            .expect("recall returns a list")
            .iter()
            .find(|f| f["content"] == "joined in the spring")
            .unwrap_or_else(|| panic!("the captured fact must come back: {body}"));
        assert_eq!(edged["edge"]["type"], "memberOf", "got {edged}");
        assert_eq!(edged["edge"]["object"], "org:guild");
    }

    /// **Objects of a kind, with their pages** — through the wire, and with
    /// nothing in the call naming what the page is for.
    ///
    /// The negative it rests on is the same query without `prose`: the key is
    /// then absent rather than empty, so a caller can tell a page nobody asked
    /// for from a page with nothing on it.
    #[tokio::test]
    async fn a_kind_comes_back_with_each_objects_prose() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("bot", "gamma", "Gamma")))
            .await
            .expect("add_entity ok");
        let charter = "Keeps the roster.\n\nHard line: never writes to the ledger.";
        jojobot
            .set_charter(Parameters(SetCharterArgs {
                bot: "gamma".into(),
                prose: charter.into(),
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("set_charter ok");

        let asked = RecallArgs {
            kind: Some("bot".into()),
            prose: Some(true),
            facts: Some(false),
            ..of_nothing()
        };
        let body = json_of(&jojobot.recall(Parameters(asked)).await.expect("recall ok"));
        let gamma = body["objects"]
            .as_array()
            .expect("a list of objects")
            .iter()
            .find(|o| o["id"] == "bot:gamma")
            .unwrap_or_else(|| panic!("the bot must be in a query for its kind: {body}"));
        assert_eq!(
            gamma["prose"], charter,
            "the page comes back whole: {gamma}"
        );
        assert!(
            gamma.get("facts").is_none(),
            "facts were declined, so the key is absent: {gamma}"
        );

        let unasked = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    kind: Some("bot".into()),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        assert!(
            unasked["objects"][0].get("prose").is_none(),
            "prose nobody asked for is absent rather than empty: {unasked}"
        );
    }

    /// **A key's value selects, and the walk nests** — the two halves this
    /// verb grew, through the surface a caller uses.
    #[tokio::test]
    async fn a_value_selects_and_a_walk_nests() {
        let jojobot = handler();
        for (kind, slug, name) in [
            ("event", "birthday-party", "Birthday Party"),
            ("person", "patana", "Patana"),
        ] {
            jojobot
                .add_entity(Parameters(add_args(kind, slug, name)))
                .await
                .expect("add_entity ok");
        }
        capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some("attendance".into()),
                object: Some("event:birthday-party".into()),
                event_type: Some("rsvp".into()),
                metadata: Some(
                    [("answer".to_string(), "yes".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("patana", "coming to the party")
            },
        )
        .await;
        capture_ok(&jojobot, capture_args("patana", "vegetarian")).await;

        let selected = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    fields: Some(vec![KeyFilterArgs {
                        key: "answer".into(),
                        value: Some("yes".into()),
                        compare: None,
                    }]),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(selected["objects"][0]["id"], "person:patana", "{selected}");
        assert_eq!(
            selected["objects"][0]["facts"].as_array().map(Vec::len),
            Some(1),
            "the object carries the record that answered, not its whole page: {selected}"
        );

        let walked = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: Some("event:birthday-party".into()),
                    follow: Some(FollowArgs {
                        shape: Some("attendance".into()),
                        relation: None,
                        direction: Some("in".into()),
                        depth: None,
                        keeping: None,
                    }),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        let guest = &walked["objects"][0]["connected"][0];
        assert_eq!(guest["id"], "person:patana", "{walked}");
        assert_eq!(guest["via"]["direction"], "in", "{walked}");
        assert!(
            guest["facts"]
                .as_array()
                .expect("the guest's own facts")
                .iter()
                .any(|f| f["content"] == "vegetarian"),
            "a reached object arrives carrying its own page: {walked}"
        );
    }

    /// **A query that narrows nothing is refused**, with a way forward rather
    /// than a protocol failure — and the same call with one filter is served,
    /// so the refusal is about the argument and not about the verb.
    #[tokio::test]
    async fn a_query_that_narrows_nothing_is_blocked() {
        let jojobot = handler();
        let refused = blocked(
            &jojobot
                .recall(Parameters(of_nothing()))
                .await
                .expect("a malformed query is an answer, not a protocol failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");

        jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("person".into()),
                ..of_nothing()
            }))
            .await
            .expect("one filter is enough");
    }

    /// **A declared reference key is walkable through the wire**, both ways,
    /// and the ordering a declaration licenses narrows what the walk reaches.
    ///
    /// The whole slice through the surface a caller holds: nothing here calls
    /// the resolver, so it fails on a build where the arguments never reach it.
    #[tokio::test]
    async fn a_relation_is_walkable_and_a_walk_can_filter_what_it_reaches() {
        let jojobot = handler();
        for (kind, slug, name) in [
            ("person", "bart", "Bart"),
            ("thing", "santas-little-helper", "Santa's Little Helper"),
            ("thing", "snowball", "Snowball"),
        ] {
            jojobot
                .add_entity(Parameters(add_args(kind, slug, name)))
                .await
                .expect("add_entity ok");
        }
        jojobot
            .declare_type(Parameters(DeclareTypeArgs {
                name: "pet".into(),
                fields: vec![
                    FieldArgs {
                        key: "born".into(),
                        holds: Some("date".into()),
                    },
                    FieldArgs {
                        key: "owner".into(),
                        holds: Some("reference".into()),
                    },
                ],
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("declare_type ok");
        for (slug, born) in [
            ("santas-little-helper", "2019-04-15"),
            ("snowball", "2024-11-02"),
        ] {
            capture_ok(
                &jojobot,
                CaptureArgs {
                    event_type: Some("pet".into()),
                    metadata: Some(
                        [
                            ("born".to_string(), born.to_string()),
                            ("owner".to_string(), "person:bart".to_string()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    ..capture_args(&format!("thing:{slug}"), "one of the pets")
                },
            )
            .await;
        }

        let has_many = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: Some("person:bart".into()),
                    facts: Some(false),
                    follow: Some(FollowArgs {
                        relation: Some("pet.owner".into()),
                        ..no_follow()
                    }),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        let reached: Vec<&str> = has_many["objects"][0]["connected"]
            .as_array()
            .expect("the pets hang off the owner")
            .iter()
            .filter_map(|o| o["id"].as_str())
            .collect();
        assert_eq!(
            reached,
            vec!["thing:santas-little-helper", "thing:snowball"],
            "the reverse of a declared reference key is the has-many: {has_many}"
        );
        assert_eq!(
            has_many["objects"][0]["connected"][0]["via"]["relation"], "pet.owner",
            "and a reached object says which relation carried it: {has_many}"
        );

        let older = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: Some("person:bart".into()),
                    facts: Some(false),
                    follow: Some(FollowArgs {
                        relation: Some("pet.owner".into()),
                        keeping: Some(vec![KeyFilterArgs {
                            key: "born".into(),
                            value: Some("2020-01-01".into()),
                            compare: Some("before".into()),
                        }]),
                        ..no_follow()
                    }),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        let kept: Vec<&str> = older["objects"][0]["connected"]
            .as_array()
            .expect("a filtered walk still answers with a list")
            .iter()
            .filter_map(|o| o["id"].as_str())
            .collect();
        assert_eq!(
            kept,
            vec!["thing:santas-little-helper"],
            "the ordering the declaration licenses narrows what the walk reaches: {older}"
        );
    }

    /// **An ordering no declaration licenses is refused**, and so is a
    /// direction beside a relation. Both come back blocked with a way forward,
    /// rather than as an equality answer wearing an ordering's name.
    #[tokio::test]
    async fn an_unlicensed_ordering_and_a_directed_relation_are_blocked() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "bart", "Bart")))
            .await
            .expect("add_entity ok");

        let unlicensed = blocked(
            &jojobot
                .recall(Parameters(RecallArgs {
                    fields: Some(vec![KeyFilterArgs {
                        key: "born".into(),
                        value: Some("2020-01-01".into()),
                        compare: Some("before".into()),
                    }]),
                    ..of_nothing()
                }))
                .await
                .expect("an unlicensed ordering is an answer, not a protocol failure"),
        );
        assert_eq!(unlicensed["wrote"], false, "{unlicensed}");

        let directed = blocked(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: Some("person:bart".into()),
                    follow: Some(FollowArgs {
                        relation: Some("pet.owner".into()),
                        direction: Some("in".into()),
                        ..no_follow()
                    }),
                    ..of_nothing()
                }))
                .await
                .expect("a direction beside a relation is an answer, not a protocol failure"),
        );
        assert_eq!(directed["wrote"], false, "{directed}");

        // The positive both refusals rest on: the same shape of call, with a
        // shape rather than a relation, is served.
        jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("person:bart".into()),
                follow: Some(FollowArgs {
                    shape: Some("connection".into()),
                    direction: Some("in".into()),
                    ..no_follow()
                }),
                ..of_nothing()
            }))
            .await
            .expect("an edge shape takes a direction");
    }

    /// A `follow` naming nothing — the base the walk cases vary.
    fn no_follow() -> FollowArgs {
        FollowArgs {
            shape: None,
            relation: None,
            direction: None,
            depth: None,
            keeping: None,
        }
    }

    /// A call naming nothing at all — the base every case above varies.
    fn of_nothing() -> RecallArgs {
        RecallArgs {
            subject: None,
            kind: None,
            answers_type: None,
            fields: None,
            facts: None,
            prose: None,
            follow: None,
            sid: None,
        }
    }
}
