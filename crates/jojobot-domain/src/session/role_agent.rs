//! **A bot that says its role claims need a runtime agent.**
//!
//! A boot that names no agent cannot be told apart from a finished session's
//! revived agent: the refusal of a wrapped run's agent (rule 788) fires only
//! when the boot names that agent. A bot carries [`REQUIRES_AGENT`], beside
//! `claims_role`, to say a role claim it makes must come with one, and this is
//! the one predicate that reads it (rule 498).

use std::collections::BTreeMap;

/// **The key a bot holds to say its role claims name a runtime agent.** The
/// value `true` sets it. A different identity writes it, never the bot it
/// binds, as for `claims_role`. The spelling is stored in a fields bag and
/// nothing outside this process declares it, so a case pins the literal.
pub const REQUIRES_AGENT: &str = "requires_agent";

/// **Whether a role claim is refused for want of an agent.**
///
/// `bot` is the folded fields of the bot that claims, which is the bot that
/// carries the role, `None` when none could be read. `agent` is the agent the
/// claim names and `runs_agent` the agent the claiming run recorded at its
/// boot: the claim writes the first of them that is not blank, so a run that
/// recorded one at boot is not refused for naming none again. A claim that
/// presents the operator's word is never refused: the operator's word always
/// takes a role.
pub fn claim_lacks_required_agent(
    bot: Option<&BTreeMap<String, String>>,
    agent: Option<&str>,
    runs_agent: Option<&str>,
    by_operator_word: bool,
) -> bool {
    let requires = bot
        .and_then(|fields| fields.get(REQUIRES_AGENT))
        .is_some_and(|held| held.trim() == "true");
    let named = [agent, runs_agent]
        .into_iter()
        .flatten()
        .any(|agent| !agent.trim().is_empty() && !agent.trim().starts_with('$'));
    requires && !named && !by_operator_word
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot(value: &str) -> BTreeMap<String, String> {
        BTreeMap::from([(REQUIRES_AGENT.to_string(), value.to_string())])
    }

    /// **The key's spelling is pinned by its literal**: a bot written before a
    /// rename could not fail a case.
    #[test]
    fn the_key_a_bot_says_its_role_claims_need_an_agent_under_is_pinned() {
        assert_eq!(REQUIRES_AGENT, "requires_agent");
    }

    /// **One row for each branch of the refusal.** Columns: the key's value, the
    /// call's agent, the boot's recorded agent, the operator's word, refused.
    #[test]
    fn a_claim_with_no_agent_is_refused_only_where_the_bot_requires_one() {
        let rows = [
            ("none", "true", None, None, false, true),
            ("blank call", "true", Some("  "), None, false, true),
            ("blank both", "true", Some(""), Some(""), false, true),
            (
                "unexpanded",
                "true",
                Some("$PASEO_AGENT_ID"),
                None,
                false,
                true,
            ),
            ("call", "true", Some("a1"), None, false, false),
            ("boot", "true", None, Some("a1"), false, false),
            (
                "blank call, boot",
                "true",
                Some(""),
                Some("a1"),
                false,
                false,
            ),
            ("operator", "true", None, None, true, false),
            ("padded", " true ", None, None, false, true),
            ("false", "false", None, None, false, false),
            ("not true", "yes", None, None, false, false),
        ];
        for (name, held, agent, runs_agent, by_word, refused) in rows {
            let held = bot(held);
            let got = claim_lacks_required_agent(Some(&held), agent, runs_agent, by_word);
            assert_eq!(got, refused, "{name}");
        }
        // A bot with no such key, and fields that could not be read, refuse nothing.
        let none = BTreeMap::new();
        assert!(!claim_lacks_required_agent(Some(&none), None, None, false));
        assert!(!claim_lacks_required_agent(None, None, None, false));
    }
}
