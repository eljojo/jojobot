//! **A claim whose words name another known thing — said on the write that
//! filed it** (card 1833).
//!
//! A fact about an object can land on the company that services it, with the
//! object named only in the claim's own words. The write is not refused: the
//! claim may be exactly where it belongs. What the receipt owes the writer is
//! the other candidate and the route that moves a claim.

use super::*;

/// **The ledger row a thing's note spends**, one per thing and session. The
/// key is the thing's permanent id, so a rename between two writes does not
/// make a second row.
const NOTED_DOMAIN: &str = "possible-subject:";

/// **How many things one note names.** A claim whose words carry a dozen rare
/// words would otherwise bury the receipt. Things past it are not named and
/// spend no row, so a later claim can still name them.
const NAMED_AT_MOST: usize = 3;

impl Jojobot {
    /// **Note it on the receipt when this claim's words share a name word with
    /// another known thing** that is not its subject, a ref or its edge object.
    ///
    /// The comparison is the one `search` and the creation screen already make
    /// ([`jojobot_domain::memory::words::sharing_a_word`]): a word of four
    /// letters or more, rare among the things of its kind. The write has
    /// landed either way, and a read that fails adds nothing.
    pub(crate) async fn note_possible_subject(
        &self,
        fact: &Fact,
        caller: &Caller,
        body: &mut serde_json::Value,
    ) {
        let Ok(known) = self.memory.list_entities(None).await else {
            return;
        };
        let describing = crate::seed::describing_keys();
        let mut text = fact.content.clone();
        for (key, value) in &fact.fields {
            if !describing.contains(key) {
                text.push('\n');
                text.push_str(value);
            }
        }
        // **The subject's parent is where a claim on its child belongs**: a
        // check-in on a rhythm says what was done to the thing the rhythm keeps
        // up. A parent's children are not left out; a claim on the parent that
        // names one may belong on the child.
        let parent = known
            .iter()
            .find(|entity| entity.id == fact.subject)
            .and_then(|entity| entity.parent.as_ref());
        let points_at = |id: &EntityId| {
            id == &fact.subject
                || parent == Some(id)
                || fact.refs.contains(id)
                || fact.edge.as_ref().is_some_and(|edge| &edge.object == id)
                // A handle written into the words is a link to that thing.
                || text.contains(id.as_str())
        };
        let mut named: Vec<&EntityId> = Vec::new();
        for (entity, _shared) in jojobot_domain::memory::words::sharing_a_word(&text, &known) {
            if named.len() == NAMED_AT_MOST {
                break;
            }
            if points_at(&entity.id) {
                continue;
            }
            // **A thing the build supplies has no permanent id**, so there is
            // nothing to key its ledger row on, and it is part of the software
            // rather than a place the operator's claim could belong.
            let Some(badge) = &entity.badge else {
                continue;
            };
            if self
                .first_contact(&format!("{NOTED_DOMAIN}{badge}"), Some(caller))
                .await
            {
                named.push(&entity.id);
            }
        }
        let Some(first) = named.first() else {
            return;
        };
        let things: Vec<&str> = named.iter().map(|id| id.as_str()).collect();
        let note = format!(
            "This claim is filed on {} and its words also name {}. It is recorded where you \
             filed it and nothing was refused. If it belongs on {first} instead, capture it \
             there with derived_from {}, then archive this one with update_fact (status: \
             archived).",
            fact.subject,
            things.join(", "),
            fact.address(),
        );
        if let Some(object) = body.as_object_mut() {
            object.insert("possible_subject_note".into(), note.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    const WORDS: &str = "The furnace's service is due in March";

    /// A session that knows the furnace and the company that services it.
    async fn known(jojobot: &Jojobot) {
        ensure(jojobot, "thing:the-furnace").await;
        ensure(jojobot, "org:mr-plow").await;
    }

    async fn capture_on(jojobot: &Jojobot, args: CaptureArgs) -> serde_json::Value {
        json_of(&jojobot.capture(Parameters(args)).await.expect("capture ok"))
    }

    fn noted(receipt: &serde_json::Value) -> Option<&str> {
        receipt["possible_subject_note"].as_str()
    }

    /// **A claim filed on one thing that names another in its words says so.**
    /// `thing:the-furnace` is known and `org:mr-plow` holds the claim; the words
    /// carry `furnace`. The note names the furnace, carries the claim's address
    /// to move from, and the write still lands.
    ///
    /// Each negative sits beside this positive in the same read, on a fresh
    /// session each so that the once-per-session rule is not what silences it:
    /// the furnace as the claim's edge object, as a ref, as a mention in the
    /// words, as the subject itself, and words that match nothing.
    #[tokio::test]
    async fn a_claim_naming_another_thing_is_noted_unless_the_claim_already_points_at_it() {
        let jojobot = handler();
        known(&jojobot).await;
        for slug in ["gamma", "delta", "epsilon", "omega", "sigma", "psi"] {
            make_bot(&jojobot, slug).await;
        }
        let session = |slug: &'static str| {
            let jojobot = &jojobot;
            async move { booted(jojobot, slug).await }
        };

        let positive = session("gamma").await;
        let filed = capture_on(
            &jojobot,
            CaptureArgs {
                sid: Some(positive),
                ..capture_args("org:mr-plow", WORDS)
            },
        )
        .await;
        assert_ne!(filed["status"], "blocked", "the write lands: {filed}");
        let note = noted(&filed).unwrap_or_else(|| panic!("a note names the furnace: {filed}"));
        assert!(note.contains("thing:the-furnace"), "{note}");
        let address = filed["address"].as_str().expect("an address");
        assert!(note.contains(address), "the note names the claim: {note}");
        assert!(note.contains("derived_from"), "the route is named: {note}");
        assert!(
            note.contains("archive"),
            "the route ends in archive: {note}"
        );

        let cases: Vec<(&str, CaptureArgs)> = vec![
            (
                "delta",
                CaptureArgs {
                    shape: Some("about".into()),
                    object: Some("thing:the-furnace".into()),
                    ..capture_args("org:mr-plow", WORDS)
                },
            ),
            (
                "epsilon",
                CaptureArgs {
                    refs: Some(vec!["thing:the-furnace".into()]),
                    ..capture_args("org:mr-plow", WORDS)
                },
            ),
            (
                "omega",
                capture_args(
                    "org:mr-plow",
                    "@thing:the-furnace is due for service in March",
                ),
            ),
            ("sigma", capture_args("thing:the-furnace", WORDS)),
            (
                "psi",
                capture_args("org:mr-plow", "Invoices are paid on the first of the month"),
            ),
        ];
        for (slug, args) in cases {
            let sid = session(slug).await;
            let receipt = capture_on(
                &jojobot,
                CaptureArgs {
                    sid: Some(sid),
                    ..args
                },
            )
            .await;
            assert_ne!(receipt["status"], "blocked", "{slug}: {receipt}");
            assert!(
                noted(&receipt).is_none(),
                "{slug} carries no note: {receipt}"
            );
        }
    }

    /// **A thing is named once per session, and a new session is told again.**
    /// The second claim in the session names the furnace too and carries no
    /// note; a claim that names a different thing in the same session still
    /// does; another session meets the furnace's note afresh, because the
    /// ledger is the session's own.
    #[tokio::test]
    async fn a_thing_is_named_once_a_session() {
        let jojobot = handler();
        known(&jojobot).await;
        ensure(&jojobot, "pet:snowball").await;
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        let (first, second) = (
            booted(&jojobot, "gamma").await,
            booted(&jojobot, "delta").await,
        );
        let file = |sid: &str, words: &str| {
            let args = CaptureArgs {
                sid: Some(sid.to_string()),
                ..capture_args("org:mr-plow", words)
            };
            let jojobot = &jojobot;
            async move { capture_on(jojobot, args).await }
        };

        let once = file(&first, WORDS).await;
        assert!(noted(&once).is_some(), "the first claim is noted: {once}");
        let again = file(&first, "The furnace was cleaned in April").await;
        assert!(
            noted(&again).is_none(),
            "the furnace is not named twice: {again}"
        );
        let other = file(&first, "The snowball sleeps by the old stove").await;
        let other_note = noted(&other).unwrap_or_else(|| panic!("a new thing is named: {other}"));
        assert!(other_note.contains("pet:snowball"), "{other_note}");
        assert!(!other_note.contains("thing:the-furnace"), "{other_note}");

        let fresh = file(&second, WORDS).await;
        assert!(noted(&fresh).is_some(), "another session is told: {fresh}");
    }

    /// **New content on an existing claim is read the same way.** Another
    /// session filed the claim with the furnace in its words, so this session's
    /// ledger is unspent. An edit that leaves the content alone carries no note
    /// although the claim names the furnace; the edit that sends new content
    /// carries it.
    #[tokio::test]
    async fn an_edit_that_sends_content_is_noted_and_one_that_does_not_is_not() {
        let jojobot = handler();
        known(&jojobot).await;
        make_bot(&jojobot, "gamma").await;
        let filing = booted(&jojobot, "gamma").await;
        let filed = capture_as(&jojobot, &filing, capture_args("org:mr-plow", WORDS)).await;
        let address = filed["address"].as_str().expect("an address").to_string();

        let untouched = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    details: Some("A note about the invoices".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        assert_ne!(untouched["status"], "blocked", "{untouched}");
        assert!(noted(&untouched).is_none(), "no new content: {untouched}");

        let edited = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("The furnace's service moved to April".into()),
                    provenance: Some("inference".into()),
                    ..update_args(&address)
                }))
                .await
                .expect("update ok"),
        );
        let note = noted(&edited).unwrap_or_else(|| panic!("the edit is noted: {edited}"));
        assert!(note.contains("thing:the-furnace"), "{note}");
        assert!(note.contains(&address), "{note}");
    }

    /// **The things the build supplies are never named.** `view:ready` is known
    /// to the shipped handler and carries the rare word `ready`; a claim saying
    /// the order is ready names no place it could belong. The positive sits
    /// beside it: the same handler names a stored thing from the same kind of
    /// words.
    #[tokio::test]
    async fn a_thing_the_build_supplies_is_never_named() {
        let jojobot = handler_shipped();
        let listed = jojobot.memory.list_entities(None).await.expect("list");
        assert!(
            listed.iter().any(|e| e.id.as_str() == "view:ready"),
            "the build supplies view:ready: {listed:?}"
        );
        known(&jojobot).await;
        let supplied = capture_on(
            &jojobot,
            capture_args("org:mr-plow", "The order is ready for pickup"),
        )
        .await;
        assert_ne!(supplied["status"], "blocked", "{supplied}");
        assert!(noted(&supplied).is_none(), "{supplied}");
        let stored = capture_on(&jojobot, capture_args("org:mr-plow", WORDS)).await;
        assert!(
            noted(&stored).is_some(),
            "a stored thing is named: {stored}"
        );
    }

    /// **A claim on a child that names its parent is where it belongs.** A
    /// rhythm lives under the thing it keeps up, and a check-in says what was
    /// done to that thing. Over the rooms' seeded claims every note was this
    /// one. The negative sits beside its positives in the same read: the same
    /// words on an unrelated subject name the parent, and a claim on the parent
    /// that names a child still names it, because that claim may belong on the
    /// child.
    #[tokio::test]
    async fn a_claim_naming_its_subjects_parent_is_not_noted() {
        let jojobot = handler();
        ensure(&jojobot, "person:randy").await;
        ensure(&jojobot, "org:mr-plow").await;
        let created = json_of(
            &jojobot
                .add_entity(Parameters(AddEntityArgs {
                    parent: Some("person:randy".into()),
                    sets: Some(
                        [
                            ("name", "Call Randy"),
                            ("cadence_days", "7"),
                            ("advances_from", "check_in_date"),
                            ("counts_from", "2025-12-30"),
                            ("last_check_in", "2025-12-30"),
                        ]
                        .into_iter()
                        .map(|(key, value)| (key.to_string(), value.to_string()))
                        .collect(),
                    ),
                    sid: Some(writing_as(&jojobot)),
                    ..add_args("rhythm", "call-randy", "Call Randy")
                }))
                .await
                .expect("add ok"),
        );
        assert_ne!(created["status"], "blocked", "{created}");

        let words = "Called Randy about the schedule";
        let on_the_child = capture_on(&jojobot, capture_args("rhythm:call-randy", words)).await;
        assert_ne!(on_the_child["status"], "blocked", "{on_the_child}");
        assert!(noted(&on_the_child).is_none(), "{on_the_child}");

        let unrelated = capture_on(&jojobot, capture_args("org:mr-plow", words)).await;
        let note = noted(&unrelated).unwrap_or_else(|| panic!("an unrelated subject: {unrelated}"));
        assert!(note.contains("person:randy"), "{note}");

        let on_the_parent = capture_on(
            &jojobot,
            capture_args("person:randy", "The weekly call is overdue"),
        )
        .await;
        let note =
            noted(&on_the_parent).unwrap_or_else(|| panic!("the child is named: {on_the_parent}"));
        assert!(note.contains("rhythm:call-randy"), "{note}");
    }
}
