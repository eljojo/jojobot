//! `update_entity` — Edit what an entity is called, and its other metadata, in place.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `update_entity`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateEntityArgs {
    /// The entity's handle. Not editable — renaming a handle is a separate
    /// operation.
    pub handle: String,
    /// New display name.
    #[serde(default)]
    pub name: Option<String>,
    /// The whole alias set, replaced. Omit to leave it alone; pass `[]` to clear
    /// it. No commas.
    #[serde(default)]
    pub aliases: Option<Vec<String>>,
    /// New source.
    #[serde(default)]
    pub source: Option<String>,
    /// New cross-link to this entity in the task layer, in whatever form that
    /// layer addresses things. One reference, no space and no comma.
    #[serde(default)]
    pub crm: Option<String>,
    /// The token a previous call's refusal handed you, sent back after you read
    /// its candidates for the name or alias you are claiming here and judged
    /// them a different entity. It lifts only the refusal that minted it. Any
    /// change to what this entity is CALLED is screened exactly as a creation
    /// is.
    #[serde(default)]
    pub override_token: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Edit an entity's metadata in place. The handle itself never changes, and
/// any change to what it is CALLED — name or aliases — is screened by the
/// write guard just as a creation is.
#[tool_router(router = update_entity_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Edit what an entity is called and where it came from (name/aliases/source/\
                       crm), in place. The handle never changes — there is no rename. THIS VERB \
                       DOES NOT TOUCH MAILBOXES: a box is not a property of an entity that can \
                       be edited or reassigned — it belongs to the bot it is named for and opens \
                       with it, in add_entity, so there is nothing here to point at a different \
                       one. Any change to what it is CALLED — name or aliases — faces the same \
                       check a creation does, because an alias is a name: it can come back \
                       status: blocked with candidates, and the override_token that refusal \
                       carries is how you confirm a genuinely shared name — a token lifts the one \
                       refusal that minted it and no other. Passing `aliases` REPLACES the whole \
                       set ([] clears \
                       it); source and crm edits are never questioned. A handle that names \
                       nothing comes back blocked with the nearest handles — it never creates."
    )]
    pub(crate) async fn update_entity(
        &self,
        Parameters(args): Parameters<UpdateEntityArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        if let Err(refused) = self.identified(args.sid.as_deref()) {
            return Ok(refused);
        }
        let handle = EntityId::person(&args.handle);
        let patch = EntityPatch {
            name: args.name,
            aliases: args.aliases,
            source: args.source,
            crm: args.crm,
            override_token: args.override_token.clone(),
        };
        let written = match self.memory.update_entity(&handle, patch).await {
            Ok(written) => written,
            Err(e) => return memory_declined("update_entity", e),
        };
        match written {
            Guarded::Written(entity) => {
                self.beat("update_entity", entity.id.as_str(), args.sid.as_deref())
                    .await;
                json_result(&entity_json(&entity))
            }
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(
                &attempted,
                &candidates,
                Blocked::Relabelling,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    /// `update_entity` edits metadata and leaves the handle alone.
    #[tokio::test]
    async fn update_entity_edits_metadata() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("thing", "red-bike", "Red Bike")))
            .await
            .expect("add ok");
        let updated = jojobot
            .update_entity(Parameters(UpdateEntityArgs {
                handle: "thing:red-bike".into(),
                name: Some("Red Bike (the gravel one)".into()),
                aliases: None,
                source: None,
                crm: Some("card:551".into()),
                override_token: None,
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("update ok");
        let body = json_of(&updated);
        assert_eq!(body["id"], "thing:red-bike", "the handle is immutable");
        assert_eq!(body["name"], "Red Bike (the gravel one)");
        assert_eq!(
            body["source"], "user-named",
            "an omitted field is left alone"
        );
    }

    /// A rename onto a name the index already holds comes back as the same
    /// error-flagged candidates response a blocked creation does — the guard
    /// cannot be side-stepped by creating under a throwaway name and renaming.
    #[tokio::test]
    async fn a_rename_onto_an_existing_name_is_blocked() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "alpha", "Alpha")))
            .await
            .expect("add ok");
        jojobot
            .add_entity(Parameters(add_args("person", "zenith", "Zenith")))
            .await
            .expect("add ok");

        let rename = |override_token: Option<String>| UpdateEntityArgs {
            handle: "person:zenith".into(),
            name: Some("Alpha".into()),
            aliases: None,
            source: None,
            crm: None,
            override_token,
            sid: Some(crate::harness::TEST_SID.into()),
        };

        let result = jojobot
            .update_entity(Parameters(rename(None)))
            .await
            .expect("the call succeeds; the guard answers in the body");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "person:zenith");
        assert_eq!(body["candidates"][0]["handle"], "person:alpha");
        let token = token_in(body["how_to_proceed"].as_str().expect("advice is a string"));

        // …and the name did not move.
        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("person".into()),
                    sid: Some(crate::harness::TEST_SID.into()),
                }))
                .await
                .expect("list ok"),
        );
        let names: Vec<&str> = listed["entities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["Alpha", "Zenith"]);

        // A token nobody minted must not move it. Without this half, the
        // assertion below holds identically on a build that accepts any string.
        let invented = json_of(
            &jojobot
                .update_entity(Parameters(rename(Some("0000000000000000".into()))))
                .await
                .expect("the call succeeds; the guard answers in the body"),
        );
        assert_eq!(
            invented["status"], "blocked",
            "a token nobody minted moves nothing: {invented}"
        );

        let forced = json_of(
            &jojobot
                .update_entity(Parameters(rename(Some(token))))
                .await
                .expect("confirmed rename ok"),
        );
        assert_ne!(forced["status"], "blocked");
        assert_eq!(forced["name"], "Alpha");
    }

    /// **The token as a caller gets it: out of the refusal's own advice.**
    ///
    /// Recomputing it from the guard would prove the guard agrees with itself
    /// and would pass on a build that mints a token and tells nobody, which is
    /// a secret rather than a mechanism (rule 68). Sixteen hex digits is the
    /// token's shape, not a sentence somebody wrote, so this survives the
    /// advice being reworded.
    fn token_in(advice: &str) -> String {
        let chars: Vec<char> = advice.chars().collect();
        chars
            .windows(16)
            .find(|w| w.iter().all(char::is_ascii_hexdigit))
            .map(|w| w.iter().collect())
            .unwrap_or_else(|| panic!("the refusal must carry the token it minted: {advice}"))
    }

    /// The guard's last door, through the real handler: a patch carrying
    /// only aliases renames nothing, so it must still be screened, and the
    /// advice it gets back must not describe a rename the caller never made.
    #[tokio::test]
    async fn an_alias_onto_a_taken_name_is_blocked_and_says_so_in_its_own_words() {
        let jojobot = handler();
        for (handle, name) in [("homer-simpson", "Homer Simpson"), ("zenith", "Zenith")] {
            jojobot
                .add_entity(Parameters(add_args("person", handle, name)))
                .await
                .expect("add ok");
        }

        let result = jojobot
            .update_entity(Parameters(UpdateEntityArgs {
                handle: "person:zenith".into(),
                name: None,
                aliases: Some(vec!["Homer Simpson".into()]),
                source: None,
                crm: None,
                override_token: None,
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("the call succeeds; the guard answers in the body");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "person:zenith");
        assert_eq!(body["candidates"][0]["handle"], "person:homer-simpson");
        let advice = body["how_to_proceed"].as_str().expect("advice is a string");
        assert!(
            advice.contains("alias"),
            "the advice must name the thing that was actually refused: {advice}"
        );
        assert!(
            !advice.contains("renamed"),
            "nothing was renamed — telling them so sends them hunting for a rename: {advice}"
        );

        // …and the alias did not land.
        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("person".into()),
                    sid: Some(crate::harness::TEST_SID.into()),
                }))
                .await
                .expect("list ok"),
        );
        let zenith = listed["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == "person:zenith")
            .expect("zenith is still there");
        assert_eq!(
            zenith["alternateName"].as_array().map(Vec::len),
            Some(0),
            "a blocked alias write lands nothing: {zenith}"
        );
    }

    /// **Alternate names go in and come back**, under schema.org's word for
    /// them. `update_entity` replaces the set whole — including with nothing,
    /// because "it has none" is a thing a caller must be able to say.
    #[tokio::test]
    async fn an_entity_carries_its_alternate_names_through_the_handler() {
        let jojobot = handler();
        let added = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    aliases: Some(vec!["Cosme Fulanito".into(), "H.".into()]),
                    ..add_args("person", "homer-simpson", "Homer Simpson")
                }))
                .await
                .expect("add ok"),
        );
        assert_eq!(added["alternateName"][0], "Cosme Fulanito");
        assert_eq!(added["alternateName"][1], "H.");

        let patch = |aliases: Vec<String>| UpdateEntityArgs {
            handle: "person:homer-simpson".into(),
            name: None,
            aliases: Some(aliases),
            source: None,
            crm: None,
            override_token: None,
            sid: Some(crate::harness::TEST_SID.into()),
        };

        let replaced = json_of(
            &jojobot
                .update_entity(Parameters(patch(vec!["Cosme Fulanito".into()])))
                .await
                .expect("update ok"),
        );
        assert_eq!(
            replaced["alternateName"].as_array().expect("a list").len(),
            1,
            "the set is replaced, not appended to: {replaced}"
        );

        let cleared = json_of(
            &jojobot
                .update_entity(Parameters(patch(Vec::new())))
                .await
                .expect("update ok"),
        );
        assert!(
            cleared["alternateName"]
                .as_array()
                .expect("a list")
                .is_empty()
        );

        // An alias carrying the separator is refused, not silently split — and
        // refused as a blocked answer, because it is the caller's mistake
        // (rule 68).
        let refused = blocked(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    aliases: Some(vec!["one, two".into()]),
                    ..add_args("person", "comma-carrier", "Comma Carrier")
                }))
                .await
                .expect("a caller mistake is an answer, not a protocol failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
    }

    /// Updating an entity that isn't there is a client error naming near misses
    /// — it never creates one.
    #[tokio::test]
    async fn update_entity_unknown_handle_is_a_client_error() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("thing", "red-bike", "Red Bike")))
            .await
            .expect("add ok");
        let err = jojobot
            .update_entity(Parameters(UpdateEntityArgs {
                handle: "thing:red-bikee".into(),
                name: Some("nope".into()),
                aliases: None,
                source: None,
                crm: None,
                override_token: None,
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("an unknown handle is an answer, not a protocol failure");
        let body = blocked(&err);
        assert_eq!(body["attempted"], "thing:red-bikee");
        assert_eq!(
            body["candidates"][0]["handle"], "thing:red-bike",
            "must name the near miss: {body}"
        );
    }
}
