//! **The operator's code: asking for one, presenting it on a boot, and what a
//! boot is given for it.**
//!
//! A session the operator started by hand has no voucher. It asks for a code for
//! the bot it names; jojobot writes the code into the operator's own mailbox and
//! keeps a [`Challenge`] of it, written only here. The session is handed the
//! challenge's handle and never the code. The operator types the code into the
//! session, and the session presents it on its boot: a right code for the
//! assistant is given the root, and a right code for any other bot is given that
//! bot's ordinary voucher.
//!
//! **The hash is keyed** with a key the process draws at startup and never
//! stores, because any bot can read a challenge and a plain hash of eight
//! characters would fall to a guessing run inside the ten-minute window. **The
//! third wrong code voids the challenge.**
//!
//! Nothing here refuses an act for want of a voucher. The gate word the boot
//! answers is read from the vouchers that exist; a later release enforces it.

use hmac::{Hmac, Mac};
use jiff::civil::Date;
use jojobot_domain::challenge::{
    self, CODE_LEN, Challenge, ChallengeState, MAX_WRONG, Redemption, WINDOW, decide_redemption,
};
use jojobot_domain::mailbox::NewMessage;
use jojobot_domain::memory::{EntityId, EntityKind, FactAddress, FactPatch, MemoryError};
use jojobot_domain::voucher::{self, Voucher, VoucherState};
use rmcp::{ErrorData as McpError, model::CallToolResult};
use sha2::Sha256;

use crate::Jojobot;
use crate::answer::{WayForward, json_result};
use crate::caller::handle_declined;

/// The bot a boot is given the root for.
const ASSISTANT: &str = "bot:assistant";

/// **A challenge whose code was checked and matched**, ready to be spent once
/// the boot has a session to give the voucher to.
pub(crate) struct Verified {
    /// The challenge.
    pub(crate) handle: EntityId,
    /// The claim that holds its keys.
    address: FactAddress,
    /// The bot the code is for.
    pub(crate) bot: EntityId,
}

/// Lower-case hex of the bytes.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// **Two hashes compared without stopping at the first difference**, so the
/// time a refusal takes says nothing about how much of a guess was right.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |different, (x, y)| different | (x ^ y))
            == 0
}

/// A code as it is compared: the operator may type it in either case.
fn normal(code: &str) -> String {
    code.trim().to_lowercase()
}

impl Jojobot {
    /// **The keyed hash of a code**, in hex.
    fn hash_of_code(&self, code: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.registry.challenge_key())
            .expect("a keyed hash takes a key of any length");
        mac.update(normal(code).as_bytes());
        hex(&mac.finalize().into_bytes())
    }

    /// **A challenge and the claim that holds it**, or `None` when the handle is
    /// no challenge. A handle of another kind is none: a caller cannot point the
    /// check at a record that is not one.
    async fn read_challenge(
        &self,
        handle: &EntityId,
    ) -> Result<Option<(FactAddress, Challenge)>, MemoryError> {
        let named = handle.as_str().split_once(':').map(|(kind, _)| kind);
        if named != Some(EntityKind::CHALLENGE.as_token()) {
            return Ok(None);
        }
        let facts = match self.memory.recall(handle).await {
            Ok(facts) => facts,
            Err(MemoryError::UnknownEntity { .. }) => return Ok(None),
            Err(e) => return Err(e),
        };
        let Some(claim) = facts
            .iter()
            .find(|fact| fact.fields.contains_key(challenge::CODE_HASH))
        else {
            return Ok(None);
        };
        let folded = self.memory.fields(handle).await?;
        Ok(Challenge::from_fields(&folded).map(|read| (claim.address(), read)))
    }

    /// **Set keys on a challenge's claim**, as the bot it is for.
    async fn write_challenge(
        &self,
        address: &FactAddress,
        bot: &EntityId,
        fields: impl IntoIterator<Item = (&'static str, String)>,
    ) -> Result<(), MemoryError> {
        self.memory
            .update_fact(
                address,
                FactPatch {
                    fields: fields
                        .into_iter()
                        .map(|(key, value)| (key.to_string(), value))
                        .collect(),
                    ..FactPatch::default()
                },
                bot,
            )
            .await
            .map(|_| ())
    }

    /// **Ask for a code for `bot`.** The code goes into the operator's box and
    /// nowhere else; the answer names the challenge and carries no code. The
    /// call starts no session.
    pub(crate) async fn ask_for_code(
        &self,
        bot: &EntityId,
        today: Date,
    ) -> Result<CallToolResult, McpError> {
        let declined = |text: String| Ok(handle_declined(bot.as_str(), text));
        let Some(operator) = self.instance_operator().await else {
            return declined(format!(
                "Nothing was made. No person is the operator yet, so there is nobody to send a \
                 code to. The operator is named on the instance's record, topic:instance: {}.",
                crate::orientation::instance_zone::no_operator_yet(),
            ));
        };
        match self.memory.fields(bot).await {
            Ok(_) if bot.kind() == Some(EntityKind::BOT) => {}
            Ok(_) | Err(MemoryError::UnknownEntity { .. }) => {
                return declined(format!(
                    "Nothing was made. '{bot}' is not a bot jojobot knows, so no code is for it. \
                     Name a bot from the snapshot start_here returns."
                ));
            }
            Err(e) => return Ok(self.ask_failed(bot, &e)),
        }
        let mailbox = match self.operators_box(&operator).await {
            Ok(mailbox) => mailbox,
            Err(refused) => return Ok(refused),
        };
        let now = self.clock().now();

        // **One challenge stands per bot**: a new request replaces the old one.
        let standing = match self.memory.list_entities(Some(EntityKind::CHALLENGE)).await {
            Ok(standing) => standing,
            Err(e) => return Ok(self.ask_failed(bot, &e)),
        };
        for entity in standing.into_iter().filter(|e| e.archived.is_none()) {
            let Ok(Some((address, held))) = self.read_challenge(&entity.id).await else {
                continue;
            };
            if &held.bot == bot && held.state == ChallengeState::Issued {
                let state = ChallengeState::Replaced.as_token().to_string();
                if let Err(e) = self
                    .write_challenge(&address, bot, [(challenge::STATE, state)])
                    .await
                {
                    return Ok(self.ask_failed(bot, &e));
                }
            }
        }

        let code = jojobot_domain::handle::draw(CODE_LEN);
        let made = Challenge {
            bot: bot.clone(),
            code_hash: self.hash_of_code(&code),
            state: ChallengeState::Issued,
            issued_at: now,
            wrong_tries: 0,
        };
        let handle = match self
            .issue_engine_record(
                EntityKind::CHALLENGE,
                made.fields(),
                format!("a code for {bot}"),
                today,
            )
            .await
        {
            Ok(handle) => handle,
            Err(e) => return Ok(self.ask_failed(bot, &e)),
        };
        let minutes = WINDOW.as_secs() / 60;
        let sent = self
            .mailboxes
            .post_message(NewMessage {
                mailbox,
                body: format!(
                    "Code: {code}\nFor: {bot}\nGood for {minutes} minutes. Type it into the \
                     session that asked for it. That session presents it with the challenge \
                     {handle}.\nIf you did not start a session as {bot}, ignore this message."
                ),
                subject: Some(format!("A code to start a session as {bot}")),
                sender: bot.as_str().to_string(),
                sent_at: now,
                in_reply_to: None,
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
            })
            .await;
        if !matches!(sent, Ok(jojobot_domain::mailbox::Guarded::Written(_))) {
            // The code reached nobody, so the challenge is of no use to anybody:
            // it is voided rather than left standing for a code nobody holds.
            if let Ok(Some((address, _))) = self.read_challenge(&handle).await {
                let state = ChallengeState::Void.as_token().to_string();
                let _ = self
                    .write_challenge(&address, bot, [(challenge::STATE, state)])
                    .await;
            }
            return Ok(handle_declined(
                bot.as_str(),
                WayForward::mailbox_store_failure(
                    "Nothing was sent, so the challenge was voided. The code could not be put \
                     in the operator's box. Ask again in a moment.",
                ),
            ));
        }
        json_result(&serde_json::json!({
            "challenge": {
                "handle": handle,
                "bot": bot,
                "valid_for_minutes": minutes,
                "next": format!(
                    "A code is in the operator's box. It is not shown here. When the operator \
                     gives it to you, call start_here with bot {bot}, challenge {handle} and \
                     code set to what they typed. It works once, and the third wrong code voids \
                     the challenge."
                ),
            },
        }))
    }

    /// The refusal for a store that could not be read while a code was asked for.
    fn ask_failed(&self, bot: &EntityId, e: &MemoryError) -> CallToolResult {
        tracing::warn!(error = %e, %bot, "a code could not be asked for");
        handle_declined(
            bot.as_str(),
            WayForward::memory_store_failure(
                "Nothing was made: the store could not be read or written just now. Ask again \
                 in a moment.",
            ),
        )
    }

    /// **Check a presented code, before the boot begins anything.** A right code
    /// comes back [`Verified`]. Every other outcome is a refusal that started no
    /// session, and a wrong code is counted: the third voids the challenge.
    ///
    /// The caller holds [`Jojobot::redeem_lock`] across this and the spending, so
    /// two boots presenting one code cannot both pass.
    pub(crate) async fn verify_code(
        &self,
        bot: &EntityId,
        reference: &str,
        code: &str,
    ) -> Result<Verified, CallToolResult> {
        let handle = EntityId(reference.trim().to_string());
        let anew = "To go on, ask for a new code with start_here and ask_for_code.";
        let read = match self.read_challenge(&handle).await {
            Ok(Some(read)) => read,
            Ok(None) => {
                return Err(handle_declined(
                    handle.as_str(),
                    format!("'{handle}' is no challenge jojobot holds. {anew}"),
                ));
            }
            Err(e) => return Err(self.ask_failed(bot, &e)),
        };
        let (address, held) = read;
        if &held.bot != bot {
            return Err(handle_declined(
                handle.as_str(),
                format!(
                    "That challenge is for {}, not for {bot}. A code boots only the bot it was \
                     asked for.",
                    held.bot
                ),
            ));
        }
        let matches = same(&self.hash_of_code(code), &held.code_hash);
        match decide_redemption(&held, self.clock().now(), matches) {
            Redemption::Granted => Ok(Verified {
                handle,
                address,
                bot: held.bot,
            }),
            Redemption::WrongCode { void } => {
                let tries = held.wrong_tries + 1;
                let mut fields = vec![(challenge::WRONG_TRIES, tries.to_string())];
                if void {
                    fields.push((
                        challenge::STATE,
                        ChallengeState::Void.as_token().to_string(),
                    ));
                }
                if let Err(e) = self.write_challenge(&address, bot, fields).await {
                    return Err(self.ask_failed(bot, &e));
                }
                Err(handle_declined(
                    handle.as_str(),
                    if void {
                        format!(
                            "That code is not right, and {MAX_WRONG} wrong codes void the \
                             challenge. {anew}"
                        )
                    } else {
                        format!(
                            "That code is not right. Check it against the operator's message. \
                             {} more wrong codes void the challenge, and then you must ask for \
                             a new code.",
                            MAX_WRONG - tries
                        )
                    },
                ))
            }
            Redemption::Spent => Err(handle_declined(
                handle.as_str(),
                format!("That code was already used, and a code works once. {anew}"),
            )),
            Redemption::Replaced => Err(handle_declined(
                handle.as_str(),
                format!("A newer request for {bot} replaced this challenge. {anew}"),
            )),
            Redemption::Expired => Err(handle_declined(
                handle.as_str(),
                format!(
                    "That code expired: it is good for {} minutes. {anew}",
                    WINDOW.as_secs() / 60
                ),
            )),
            Redemption::Void => Err(handle_declined(
                handle.as_str(),
                format!("Too many wrong codes voided this challenge. {anew}"),
            )),
        }
    }

    /// **Spend a verified code and give the session its voucher.** The challenge
    /// is spent first, so a failure after it costs a new request and never lets
    /// one code be used twice. The bot the code is for decides the voucher: the
    /// assistant is given the root, every other bot its own `boot-as`. The
    /// operator issues it, and the session that booted holds it.
    pub(crate) async fn redeem_code(
        &self,
        verified: &Verified,
        sid: &str,
        today: Date,
    ) -> Result<serde_json::Value, MemoryError> {
        let operator = self
            .instance_operator()
            .await
            .ok_or_else(|| MemoryError::InvalidFact("no person is the operator".into()))?;
        let state = ChallengeState::Redeemed.as_token().to_string();
        self.write_challenge(
            &verified.address,
            &verified.bot,
            [(challenge::STATE, state)],
        )
        .await?;
        let right = if verified.bot.as_str() == ASSISTANT {
            voucher::ROOT
        } else {
            "boot-as"
        };
        let granted = Voucher {
            right: right.to_string(),
            target: verified.bot.to_string(),
            holder: sid.to_string(),
            issuer: operator.to_string(),
            parent: None,
            state: VoucherState::Held,
        };
        let id = self.issue_voucher(&granted, today).await?;
        Ok(serde_json::json!({
            "voucher": id,
            "right": granted.right,
            "target": granted.target,
            "state": granted.state.as_token(),
            "challenge": verified.handle,
        }))
    }

    /// **Whether the gate is on**: some voucher with the root right exists, in
    /// any state. A root that has been spent or voided still counts, so the gate
    /// does not open again when the assistant's session ends. A store holding no
    /// voucher, as every store does before the first root, is open.
    pub(crate) async fn gate_is_on(&self) -> Result<bool, MemoryError> {
        for entity in self.memory.list_entities(Some(EntityKind::VOUCHER)).await? {
            let fields = self.memory.fields(&entity.id).await?;
            if fields.get(voucher::RIGHT).map(String::as_str) == Some(voucher::ROOT) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// **What a named boot says about trust**: the gate word, and the voucher it
    /// was given when it presented a code. A store that could not be read says
    /// so rather than naming a gate it did not read.
    pub(crate) async fn trust_of_a_boot(
        &self,
        redeemed: Option<serde_json::Value>,
    ) -> serde_json::Value {
        let mut trust = match self.gate_is_on().await {
            Ok(true) => serde_json::json!({
                "gate": "gated",
                "note": "A root voucher exists, so the gate is on. Enforcement is a later release.",
            }),
            Ok(false) => serde_json::json!({
                "gate": "ungated",
                "note": "No root has been granted, so every boot is ungated. The operator's \
                         first code for the assistant turns the gate on.",
            }),
            Err(e) => {
                tracing::warn!(error = %e, "the gate could not be read at a boot");
                serde_json::json!({
                    "gate": "unreadable",
                    "note": "The vouchers could not be read just now.",
                })
            }
        };
        if let (Some(redeemed), Some(obj)) = (redeemed, trust.as_object_mut()) {
            obj.insert("redeemed".into(), redeemed);
        }
        trust
    }

    /// **A root lasts one session**: wrapping voids every root the wrapping
    /// session holds. Best-effort and after the close, like the release of a
    /// role: the wrap already landed. A voucher that is not a root, or is held by
    /// another session, is left alone.
    pub(crate) async fn void_roots_held_by(&self, sid: &str, bot: &EntityId) {
        let listed = match self.memory.list_entities(Some(EntityKind::VOUCHER)).await {
            Ok(listed) => listed,
            Err(e) => {
                tracing::warn!(error = %e, sid, "a wrapped session's roots could not be listed");
                return;
            }
        };
        for entity in listed {
            let Ok(fields) = self.memory.fields(&entity.id).await else {
                continue;
            };
            let held = fields.get(voucher::HOLDER).map(String::as_str) == Some(sid)
                && fields.get(voucher::RIGHT).map(String::as_str) == Some(voucher::ROOT)
                && fields.get(voucher::STATE).map(String::as_str)
                    == Some(VoucherState::Held.as_token());
            if !held {
                continue;
            }
            let Ok(facts) = self.memory.recall(&entity.id).await else {
                continue;
            };
            let Some(claim) = facts
                .iter()
                .find(|fact| fact.fields.contains_key(voucher::RIGHT))
            else {
                continue;
            };
            let voided = self
                .memory
                .update_fact(
                    &claim.address(),
                    FactPatch {
                        fields: [(
                            voucher::STATE.to_string(),
                            VoucherState::Void.as_token().to_string(),
                        )]
                        .into(),
                        ..FactPatch::default()
                    },
                    bot,
                )
                .await;
            if let Err(e) = voided {
                tracing::warn!(error = %e, sid, "a wrapped session's root could not be voided");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::orientation::start_here::OrientArgs;
    use rmcp::handler::server::wrapper::Parameters;
    use std::time::Duration;

    fn boot_with_code(bot: &str, challenge: &str, code: &str) -> OrientArgs {
        OrientArgs {
            claim: None,
            timezone: None,
            bot: Some(bot.into()),
            brief: None,
            skill: None,
            section: None,
            resume: None,
            sid: None,
            today: None,
            ask_for_code: None,
            challenge: Some(challenge.into()),
            code: Some(code.into()),
        }
    }

    /// **The hash is keyed, and the key belongs to the process.** The same code
    /// hashes the same for the same registry and differently for another, which
    /// is what makes a restart void every outstanding challenge. A code is
    /// compared as typed in either case and with stray spaces, and two codes that
    /// differ do not hash alike.
    #[test]
    fn the_hash_is_keyed_by_the_registry_and_ignores_case_and_spaces() {
        let one = handler();
        let again = one.clone();
        let other = handler();
        let hash = one.hash_of_code("abcd2345");
        assert_eq!(hash.len(), 64, "a sha256 in hex: {hash}");
        assert!(
            hash.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
        assert_eq!(hash, again.hash_of_code(" ABCD2345 "));
        assert_ne!(hash, one.hash_of_code("abcd2346"));
        assert_ne!(
            hash,
            other.hash_of_code("abcd2345"),
            "another registry has another key"
        );
    }

    /// **The comparison finds every difference.** Equal strings match; a string
    /// that differs in its first, middle or last byte, or in its length, does
    /// not.
    #[test]
    fn two_hashes_match_only_when_every_byte_does() {
        let base = "0123456789abcdef";
        assert!(same(base, base));
        for differing in [
            "x123456789abcdef",
            "01234567x9abcdef",
            "0123456789abcdex",
            "0123456789abcde",
            "0123456789abcdef0",
        ] {
            assert!(!same(base, differing), "{differing}");
        }
    }

    /// **A coded boot waits for the redeem lock, and an ordinary boot does
    /// not.** The lock is held from the check of a code to the spending of it,
    /// so a second boot presenting the same code cannot pass the check before the
    /// first has spent it. The case holds the lock itself: the coded boot does
    /// not finish while it is held and does once it is released, and a boot
    /// that presents no code goes straight through.
    #[tokio::test]
    async fn a_coded_boot_waits_for_the_redeem_lock_and_an_ordinary_boot_does_not() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let held = jojobot.registry.redeem_lock().lock().await;

        let ordinary = tokio::time::timeout(
            Duration::from_secs(5),
            jojobot.start_here(Parameters(OrientArgs {
                bot: Some("gamma".into()),
                resume: Some("new".into()),
                ..boot_with_code("gamma", "", "")
            })),
        )
        .await;
        assert!(
            ordinary.is_ok(),
            "a boot with no code never waits on the lock"
        );

        let coded = tokio::time::timeout(
            Duration::from_millis(300),
            jojobot.start_here(Parameters(boot_with_code(
                "gamma",
                "challenge:nothing",
                "abcd2345",
            ))),
        )
        .await;
        assert!(coded.is_err(), "a coded boot went through a held lock");

        drop(held);
        let released = tokio::time::timeout(
            Duration::from_secs(5),
            jojobot.start_here(Parameters(boot_with_code(
                "gamma",
                "challenge:nothing",
                "abcd2345",
            ))),
        )
        .await
        .expect("the lock is free, so the boot answers")
        .expect("start_here answers");
        let body = json_of(&released);
        assert_eq!(
            body["status"], "blocked",
            "no such challenge is refused: {body}"
        );
    }
}
