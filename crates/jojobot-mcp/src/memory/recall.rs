//! `recall` — Read back every fact about one subject, each with its address.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `recall`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RecallArgs {
    /// The entity to read facts about — any `kind:slug` id (a bare handle is
    /// read as a person).
    pub subject: String,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Read back every fact about an entity, each with its address.
#[tool_router(router = recall_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Read every fact about one entity, each with the address that makes it \
                       editable through update_fact. Use it when you already hold the handle \
                       and want the whole picture; use search when you don't. Unlike search, \
                       this returns claims of every status, superseded included. An entity that \
                       exists with nothing recorded comes back as an empty list; a handle that \
                       names nothing comes back status: blocked with the nearest handles, never \
                       as an empty list. A fact recorded under this entity that claims to be \
                       about someone else comes back too — that mismatch is worth surfacing, and \
                       the address is how it gets repaired."
    )]
    pub(crate) async fn recall(
        &self,
        Parameters(args): Parameters<RecallArgs>,
    ) -> Result<CallToolResult, McpError> {
        let subject = EntityId::person(&args.subject);
        let facts = match self.memory.recall(&subject).await {
            Ok(facts) => facts,
            Err(e) => return memory_declined("recall", e),
        };
        let body = serde_json::json!({
            "subject": subject.as_str(),
            "facts": facts.iter().map(fact_json).collect::<Vec<_>>(),
        });
        json_result(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    /// Every recalled fact carries its address, and that address is what
    /// `update_fact` takes — the pairing that makes editing possible.
    #[tokio::test]
    async fn recall_returns_addresses_that_update_fact_accepts() {
        let jojobot = handler();
        capture_ok(&jojobot, capture_args("alpha", "works at the old place")).await;

        let body = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: "alpha".into(),
                    sid: None,
                }))
                .await
                .expect("recall ok"),
        );
        let address = body["facts"][0]["address"]
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
                .recall(Parameters(RecallArgs {
                    subject: "person:zenit".into(),
                    sid: None,
                }))
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
                .recall(Parameters(RecallArgs {
                    subject: "person:zenith".into(),
                    sid: None,
                }))
                .await
                .expect("an existing entity's empty page still reads"),
        );
        assert_eq!(body["facts"].as_array().expect("a list").len(), 0);
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
                .recall(Parameters(RecallArgs {
                    subject: "alpha".into(),
                    sid: None,
                }))
                .await
                .expect("recall ok"),
        );
        let edged = body["facts"]
            .as_array()
            .expect("recall returns a list")
            .iter()
            .find(|f| f["content"] == "joined in the spring")
            .unwrap_or_else(|| panic!("the captured fact must come back: {body}"));
        assert_eq!(edged["edge"]["type"], "memberOf", "got {edged}");
        assert_eq!(edged["edge"]["object"], "org:guild");
    }
}
