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
#[cfg(test)]
use jojobot_domain::memory::NewFact;
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

/// **An error as a log line may carry it.** A write that landed behind the fold
/// reports the fact it wrote, and that fact holds the code's hash, so the
/// error's own text is never logged.
pub(crate) fn loggable(e: &MemoryError) -> String {
    match e {
        MemoryError::FoldBehind { source, .. } => {
            format!("the write landed but the fold could not be refreshed afterward: {source}")
        }
        other => other.to_string(),
    }
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
        // **Read off the claim itself, never off the fold.** The fold is a
        // projection that can lag a write that landed, and a challenge that
        // reads `issued` after its third wrong code is a code that works twice.
        Ok(Challenge::from_fields(&claim.fields).map(|read| (claim.address(), read)))
    }

    /// **Set keys on a challenge's claim**, as the bot it is for.
    async fn write_challenge(
        &self,
        address: &FactAddress,
        bot: &EntityId,
        fields: impl IntoIterator<Item = (&'static str, String)>,
    ) -> Result<(), MemoryError> {
        let written = self
            .memory
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
            .await;
        match written {
            Ok(_) => Ok(()),
            // **A write that landed is a write**, whatever the fold says: the
            // claim is read back from the claim, so the next read sees it.
            Err(MemoryError::FoldBehind { .. }) => Ok(()),
            Err(e) => Err(e),
        }
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
        let mut expired: Vec<FactAddress> = Vec::new();
        for entity in standing.into_iter().filter(|e| e.archived.is_none()) {
            // **A challenge that cannot be read fails the ask.** Making a new one
            // beside it would leave two standing, and both codes would work.
            let (address, held) = match self.read_challenge(&entity.id).await {
                Ok(Some(read)) => read,
                Ok(None) => continue,
                Err(e) => return Ok(self.ask_failed(bot, &e)),
            };
            if &held.bot == bot && held.state == ChallengeState::Issued {
                // **A pending challenge is not replaced.** While the window is
                // open the request is refused and names when it ends, so a
                // misfiring session cannot burn the operator's codes. The
                // refusal comes before any write, so it leaves nothing behind.
                let expires_at = held.issued_at + WINDOW;
                if now <= expires_at {
                    let mut body = serde_json::json!({
                        "status": "blocked",
                        "attempted": bot.as_str(),
                        "wrote": false,
                        "pending": {
                            "challenge": entity.id.as_str(),
                            "expires_at": expires_at.to_string(),
                        },
                    });
                    WayForward::retry(format!(
                        "A code for {bot} is already waiting in the operator's box and is good \
                         until {expires_at}. Use that code, or ask again after that moment. \
                         Three wrong codes void it sooner."
                    ))
                    .write_into(&mut body);
                    return Ok(CallToolResult::success(vec![crate::ContentBlock::text(
                        body.to_string(),
                    )]));
                }
                expired.push(address);
            }
        }
        // Only now, with no pending challenge in the way, do the expired ones end.
        for address in expired {
            let state = ChallengeState::Replaced.as_token().to_string();
            if let Err(e) = self
                .write_challenge(&address, bot, [(challenge::STATE, state)])
                .await
            {
                return Ok(self.ask_failed(bot, &e));
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
        tracing::warn!(error = %loggable(e), %bot, "a code could not be asked for");
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
                // **The boot is refused, and a write was made**: the wrong code
                // was counted, and the third voided the challenge. The answer
                // says both rather than "nothing was written".
                let mut body = serde_json::json!({
                    "status": "blocked",
                    "attempted": handle.as_str(),
                    "wrote": true,
                    "recorded": {
                        "wrong_tries": tries,
                        "challenge_state": if void {
                            ChallengeState::Void.as_token()
                        } else {
                            ChallengeState::Issued.as_token()
                        },
                    },
                });
                WayForward::from(if void {
                    format!(
                        "That code is not right, and {MAX_WRONG} wrong codes void the \
                         challenge. {anew}"
                    )
                } else {
                    format!(
                        "That code is not right. Check it against the operator's message. {} \
                         more wrong codes void the challenge, and then you must ask for a new \
                         code.",
                        MAX_WRONG - tries
                    )
                })
                .write_into(&mut body);
                Err(CallToolResult::success(vec![crate::ContentBlock::text(
                    body.to_string(),
                )]))
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
        // **The challenge is read again at the spend.** Between the check and
        // here a newer request may have replaced it, or a wrong code may have
        // voided it; only a challenge still standing is spent.
        let standing = matches!(
            self.read_challenge(&verified.handle).await?,
            Some((_, held)) if held.state == ChallengeState::Issued
        );
        if !standing {
            return Err(MemoryError::InvalidFact(
                "the challenge is no longer standing".into(),
            ));
        }
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
            let claim = self.voucher_claim(&entity.id).await?;
            if let Some((_, fields)) = claim
                && fields.get(voucher::RIGHT).map(String::as_str) == Some(voucher::ROOT)
            {
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
                tracing::warn!(error = %loggable(&e), "the gate could not be read at a boot");
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

    /// **A voucher's claim**, with the keys it was written with. Read off the
    /// claim, for the reason [`Jojobot::read_challenge`] is: the fold can lag.
    async fn voucher_claim(
        &self,
        id: &EntityId,
    ) -> Result<Option<(FactAddress, std::collections::BTreeMap<String, String>)>, MemoryError>
    {
        let facts = self.memory.recall(id).await?;
        Ok(facts
            .iter()
            .find(|fact| fact.fields.contains_key(voucher::RIGHT))
            .map(|claim| (claim.address(), claim.fields.clone())))
    }

    /// **A wrap ends every voucher its run holds.** Until escrow exists a voucher
    /// is not carried to a successor, so a run that is over holds nothing: a root
    /// and an ordinary voucher alike end with it, and a voucher another run holds
    /// is left alone.
    ///
    /// **It fails closed.** Called BEFORE the run closes: a listing, a read or a
    /// write that fails stops the wrap, with the step named, so a run is never
    /// closed over a voucher it may still hold. Nothing here is best-effort.
    pub(crate) async fn void_vouchers_held_by(
        &self,
        sid: &str,
        bot: &EntityId,
    ) -> Result<usize, RevocationFailed> {
        let failed = |step: &'static str, voided: usize, e: &MemoryError| {
            tracing::warn!(
                error = %loggable(e), sid, step, "a run's vouchers could not be revoked"
            );
            RevocationFailed { step, voided }
        };
        let listed = self
            .memory
            .list_entities(Some(EntityKind::VOUCHER))
            .await
            .map_err(|e| failed("list", 0, &e))?;
        let mut voided = 0;
        for entity in listed {
            let Some((address, fields)) = self
                .voucher_claim(&entity.id)
                .await
                .map_err(|e| failed("read", voided, &e))?
            else {
                continue;
            };
            let state = fields.get(voucher::STATE).map(String::as_str);
            let held = fields.get(voucher::HOLDER).map(String::as_str) == Some(sid)
                && [VoucherState::Held, VoucherState::Issued]
                    .iter()
                    .any(|live| state == Some(live.as_token()));
            if !held {
                continue;
            }
            let written = self
                .memory
                .update_fact(
                    &address,
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
            match written {
                Ok(_) | Err(MemoryError::FoldBehind { .. }) => voided += 1,
                Err(e) => return Err(failed("write", voided, &e)),
            }
        }
        Ok(voided)
    }
}

/// **Why a wrap could not end the vouchers its run holds**: the step that
/// failed, and how many it had ended before it did.
#[derive(Debug)]
pub(crate) struct RevocationFailed {
    /// `list`, `read` or `write`.
    pub(crate) step: &'static str,
    /// How many vouchers ended before the failure.
    pub(crate) voided: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::orientation::start_here::OrientArgs;
    use rmcp::handler::server::wrapper::Parameters;
    use std::sync::Arc;
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

    // ── the Codex read of 22a8c631: nine findings, each with its case ────────

    use crate::orientation::start_here::OrientArgs as Args;
    use crate::session::WrapSessionArgs;
    use crate::session::scripted::{Scripted, UnreachableRuns};
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::session::SessionState;

    fn day() -> Date {
        jiff::civil::date(2026, 10, 9)
    }

    /// A handler over a store that misbehaves on cue, an operator and a bot
    /// called `gamma`.
    async fn scripted_instance() -> (Jojobot, Arc<Scripted>) {
        let scripted = Arc::new(Scripted::over(Arc::new(InMemoryMemory::seeded())));
        let jojobot = Jojobot::new(
            scripted.clone(),
            Arc::new(crate::memory::testing::SpySearch::default()),
            Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
            Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            seeded_registry(),
        );
        crate::mailboxes::testing::name_the_operator(&jojobot, "milhouse").await;
        make_bot(&jojobot, "gamma").await;
        (jojobot, scripted)
    }

    /// Ask for a code for `gamma` as the engine does, and read it out of the
    /// operator's box. Returns the challenge's handle and the code.
    async fn asked(jojobot: &Jojobot) -> (EntityId, String) {
        let answer = jojobot
            .ask_for_code(&EntityId("bot:gamma".into()), day())
            .await
            .expect("ask answers");
        let body = json_of(&answer);
        let handle = EntityId(
            body["challenge"]["handle"]
                .as_str()
                .unwrap_or_else(|| panic!("a challenge was made: {body}"))
                .to_string(),
        );
        let mut mail = jojobot.mailboxes.scan_messages().await.expect("scan ok");
        mail.sort_by_key(|message| message.sent_at);
        let code = mail
            .iter()
            .rev()
            .find_map(|message| message.body.split("Code: ").nth(1))
            .and_then(|rest| rest.split_whitespace().next())
            .expect("the code is in the mail")
            .to_string();
        (handle, code)
    }

    fn gamma() -> EntityId {
        EntityId("bot:gamma".into())
    }

    /// **P1: the state of a challenge is read from the claim, never from the
    /// fold.** The third wrong code lands and voids the challenge, but the fold
    /// has not caught up and still says `issued` with two tries. The right code
    /// presented next is refused, because the claim says void.
    #[tokio::test]
    async fn a_wrong_code_that_landed_behind_a_lagging_fold_still_voids_the_challenge() {
        let (jojobot, scripted) = scripted_instance().await;
        let (handle, code) = asked(&jojobot).await;
        for _ in 0..2 {
            let refused = jojobot
                .verify_code(&gamma(), handle.as_str(), "00000000")
                .await;
            assert!(refused.is_err());
        }
        scripted.script(|s| {
            s.fold_lags_after_update = true;
            s.update_reports_fold_behind = true;
        });
        let third = jojobot
            .verify_code(&gamma(), handle.as_str(), "00000000")
            .await;
        let refusal = json_of(&third.err().expect("a wrong code is refused"));
        assert_eq!(
            refusal["recorded"]["challenge_state"], "void",
            "a write that landed behind the fold is still a write: {refusal}"
        );
        scripted.script(|s| s.update_reports_fold_behind = false);
        let right = jojobot.verify_code(&gamma(), handle.as_str(), &code).await;
        assert!(
            right.is_err(),
            "the right code was accepted for a challenge the third wrong code voided"
        );
        let facts = jojobot.memory.recall(&handle).await.expect("recall ok");
        let claim = facts
            .iter()
            .find(|fact| fact.fields.contains_key("code_hash"))
            .expect("the claim");
        assert_eq!(claim.fields["state"], "void", "{:?}", claim.fields);
    }

    /// **P1: asking for a code holds the redeem lock.** Two asks for one bot
    /// that ran side by side would each scan, each create, and leave two codes
    /// redeemable. The case holds the lock itself: the ask does not finish while
    /// it is held, and does once it is released.
    #[tokio::test]
    async fn asking_for_a_code_waits_for_the_redeem_lock() {
        let (jojobot, _) = scripted_instance().await;
        let ask = || Args {
            ask_for_code: Some(true),
            bot: Some("gamma".into()),
            ..boot_with_code("gamma", "", "")
        };
        let ask = ask();
        let held = jojobot.registry.redeem_lock().lock().await;
        let waiting = tokio::time::timeout(
            Duration::from_millis(300),
            jojobot.start_here(Parameters(ask)),
        )
        .await;
        assert!(waiting.is_err(), "an ask went through a held lock");
        drop(held);
        let again = Args {
            ask_for_code: Some(true),
            bot: Some("gamma".into()),
            ..boot_with_code("gamma", "", "")
        };
        let answered = tokio::time::timeout(
            Duration::from_secs(5),
            jojobot.start_here(Parameters(again)),
        )
        .await
        .expect("the lock is free")
        .expect("start_here answers");
        assert!(json_of(&answered)["challenge"]["handle"].is_string());
    }

    /// **P1: the spend re-reads the challenge.** A code was verified, then a
    /// newer request replaced its challenge. Spending the verified code is
    /// refused, no voucher is written and the challenge stays replaced.
    #[tokio::test]
    async fn a_verified_code_is_not_spent_once_a_newer_request_replaced_its_challenge() {
        let (jojobot, _) = scripted_instance().await;
        let (handle, code) = asked(&jojobot).await;
        let verified = jojobot
            .verify_code(&gamma(), handle.as_str(), &code)
            .await
            .unwrap_or_else(|_| panic!("the right code verifies"));
        // The first challenge expires, then a newer request replaces it.
        let later = jojobot
            .clone()
            .on_clock(jojobot_domain::clock::Clock::stating(jiff::civil::date(
                2026, 10, 11,
            )));
        later
            .ask_for_code(&gamma(), jiff::civil::date(2026, 10, 11))
            .await
            .expect("ask answers");
        let spent = jojobot.redeem_code(&verified, "abcd", day()).await;
        assert!(spent.is_err(), "a replaced challenge was spent: {spent:?}");
        let vouchers = jojobot
            .memory
            .list_entities(Some(EntityKind::VOUCHER))
            .await
            .expect("list ok");
        assert!(vouchers.is_empty(), "{vouchers:?}");
        let fields = jojobot.memory.fields(&handle).await.expect("fields ok");
        assert_eq!(fields["state"], "replaced", "{fields:?}");
    }

    /// **P2: an ask that cannot read the standing challenge makes no second
    /// one.** The read fails, and creating a new challenge would leave two issued.
    #[tokio::test]
    async fn an_ask_that_cannot_read_the_standing_challenge_makes_no_second_one() {
        let (jojobot, scripted) = scripted_instance().await;
        let (first, _) = asked(&jojobot).await;
        scripted.script(|s| s.recall_fails_for = Some(first.clone()));
        let refused = jojobot
            .ask_for_code(&gamma(), day())
            .await
            .expect("ask answers");
        assert_eq!(
            json_of(&refused)["status"],
            "blocked",
            "{}",
            json_of(&refused)
        );
        scripted.script(|s| s.recall_fails_for = None);
        let standing = jojobot
            .memory
            .list_entities(Some(EntityKind::CHALLENGE))
            .await
            .expect("list ok");
        assert_eq!(
            standing.len(),
            1,
            "a second challenge was made: {standing:?}"
        );
        let fields = jojobot.memory.fields(&first).await.expect("fields ok");
        assert_eq!(fields["state"], "issued", "{fields:?}");
    }

    /// **P2: a voucher that landed behind the fold is reported as written.**
    #[tokio::test]
    async fn a_voucher_that_landed_behind_the_fold_is_reported_as_written() {
        let (jojobot, scripted) = scripted_instance().await;
        let (handle, code) = asked(&jojobot).await;
        let verified = jojobot
            .verify_code(&gamma(), handle.as_str(), &code)
            .await
            .unwrap_or_else(|_| panic!("the right code verifies"));
        scripted.script(|s| s.creation_reports_fold_behind = true);
        let granted = jojobot
            .redeem_code(&verified, "abcd", day())
            .await
            .expect("a voucher that landed is a voucher written");
        let id = granted["voucher"].as_str().expect("the voucher is named");
        let fields = jojobot
            .memory
            .fields(&EntityId(id.into()))
            .await
            .expect("the voucher is there");
        assert_eq!(fields["state"], "held");
    }

    /// **P3: no log line carries the hash.** An error that reports a landed fact
    /// would print its fields, the code's hash among them.
    #[test]
    fn the_logged_form_of_an_error_leaves_the_hash_out() {
        let fact = crate::orientation::floor::prospective_fact(
            &NewFact {
                fields: [("code_hash".to_string(), "deadbeef".repeat(8))].into(),
                ..NewFact::about(EntityId("challenge:abcd".into()), "a code", day())
            },
            1,
            None,
        );
        let behind = MemoryError::FoldBehind {
            landed: jojobot_domain::memory::Landed::Fact(Box::new(fact)),
            behind: jojobot_domain::memory::search::Behind::Stale,
            source: Box::new(MemoryError::Store("the fold's re-read failed".into())),
        };
        assert!(behind.to_string().contains("deadbeef"), "the premise holds");
        assert!(
            !loggable(&behind).contains("deadbeef"),
            "{}",
            loggable(&behind)
        );
        let plain = MemoryError::Store("the store is down".into());
        assert_eq!(loggable(&plain), plain.to_string());
    }

    /// **A wrap voids every voucher its run holds, and only those.** A root and
    /// an ordinary `boot-as` voucher held by the run end with it, a voucher
    /// another run holds stays held, and the receipt says how many ended. Until
    /// escrow exists, a voucher is not carried to a successor.
    #[tokio::test]
    async fn a_wrap_voids_every_voucher_its_run_holds_and_no_other() {
        let (jojobot, _) = scripted_instance().await;
        let sid = booted_sid(&jojobot).await;
        let mut ids = Vec::new();
        for (right, holder, state) in [
            (voucher::ROOT, sid.as_str(), VoucherState::Held),
            ("boot-as", sid.as_str(), VoucherState::Held),
            ("claim-role", sid.as_str(), VoucherState::Issued),
            ("boot-as", sid.as_str(), VoucherState::Spent),
            ("boot-as", "someone-else", VoucherState::Held),
        ] {
            let held = Voucher {
                right: right.into(),
                target: "bot:gamma".into(),
                holder: holder.into(),
                issuer: "person:milhouse".into(),
                parent: None,
                state,
            };
            ids.push(jojobot.issue_voucher(&held, day()).await.expect("issued"));
        }
        let wrapped = wrap(&jojobot, &sid).await;
        let mut states = Vec::new();
        for id in &ids {
            states.push(jojobot.memory.fields(id).await.expect("fields")["state"].clone());
        }
        assert_eq!(
            states,
            ["void", "void", "void", "spent", "held"],
            "{states:?}"
        );
        assert_eq!(wrapped["trust"]["vouchers_voided"], 3, "{wrapped}");
        assert_eq!(wrapped["session"]["state"], "wrapped", "{wrapped}");
    }

    /// **A wrap that cannot revoke stops, and says which step failed.** The
    /// listing, a read and a write each fail in turn. The wrap is refused, the
    /// run stays active, nothing it held is claimed to have ended, and once the
    /// store answers again the same wrap goes through.
    #[tokio::test]
    async fn a_wrap_that_cannot_revoke_stops_and_says_which_step_failed() {
        for step in ["list", "read", "write"] {
            let (jojobot, scripted) = scripted_instance().await;
            let sid = booted_sid(&jojobot).await;
            let held = Voucher {
                right: "boot-as".into(),
                target: "bot:gamma".into(),
                holder: sid.clone(),
                issuer: "person:milhouse".into(),
                parent: None,
                state: VoucherState::Held,
            };
            let id = jojobot.issue_voucher(&held, day()).await.expect("issued");
            scripted.script(|s| match step {
                "list" => s.listing_fails_for = Some(EntityKind::VOUCHER),
                "read" => s.recall_fails_for = Some(id.clone()),
                _ => s.update_fails = true,
            });
            let refused = wrap(&jojobot, &sid).await;
            assert_eq!(refused["status"], "blocked", "{step}: {refused}");
            assert_eq!(refused["revocation"]["failed"], step, "{step}: {refused}");
            let state = jojobot.memory.fields(&id).await.expect("fields")["state"].clone();
            assert_eq!(state, "held", "{step}: the voucher cannot have ended");
            let runs = jojobot
                .sessions
                .all_sessions()
                .await
                .expect("the runs read");
            assert!(
                runs.iter().all(|run| run.state == SessionState::Active),
                "{step}: a refused wrap closed the run: {runs:?}"
            );
            scripted.script(|s| *s = Default::default());
            let wrapped = wrap(&jojobot, &sid).await;
            assert_eq!(wrapped["trust"]["vouchers_voided"], 1, "{step}: {wrapped}");
            let state = jojobot.memory.fields(&id).await.expect("fields")["state"].clone();
            assert_eq!(state, "void", "{step}");
        }
    }

    async fn booted_sid(jojobot: &Jojobot) -> String {
        let booted = json_of(
            &jojobot
                .start_here(Parameters(Args {
                    resume: Some("new".into()),
                    ..boot_with_code("gamma", "", "")
                }))
                .await
                .expect("boot answers"),
        );
        sid_of(&booted).expect("a handle")
    }

    async fn wrap(jojobot: &Jojobot, sid: &str) -> serde_json::Value {
        json_of(
            &jojobot
                .wrap_session(Parameters(WrapSessionArgs {
                    story: "done".into(),
                    sid: sid.into(),
                }))
                .await
                .expect("wrap answers"),
        )
    }

    /// **A pending challenge is not replaced.** A second request for the same
    /// bot while the first is good is refused and names when the first expires,
    /// so a misfiring session cannot burn the operator's codes. The first code
    /// still works. Once the window is over a new request stands, and the old
    /// challenge is replaced. A request for another bot is never in the way.
    #[tokio::test]
    async fn a_pending_challenge_is_not_replaced_and_the_refusal_names_its_expiry() {
        let (jojobot, _) = scripted_instance().await;
        make_bot(&jojobot, "delta").await;
        let (first, code) = asked(&jojobot).await;
        let issued_at = jojobot.memory.fields(&first).await.expect("fields")["issued_at"].clone();

        let refused = jojobot
            .ask_for_code(&gamma(), day())
            .await
            .expect("ask answers");
        let body = json_of(&refused);
        assert_eq!(body["status"], "blocked", "{body}");
        let until: jiff::Timestamp = body["pending"]["expires_at"]
            .as_str()
            .unwrap_or_else(|| panic!("the refusal names the expiry: {body}"))
            .parse()
            .expect("a moment");
        let began: jiff::Timestamp = issued_at.parse().expect("a moment");
        assert_eq!(until, began + WINDOW, "{body}");
        let standing = jojobot
            .memory
            .list_entities(Some(EntityKind::CHALLENGE))
            .await
            .expect("list ok");
        assert_eq!(
            standing.len(),
            1,
            "a second challenge was made: {standing:?}"
        );
        assert!(
            jojobot
                .verify_code(&gamma(), first.as_str(), &code)
                .await
                .is_ok(),
            "the pending code still works"
        );

        let other = jojobot
            .ask_for_code(&EntityId("bot:delta".into()), day())
            .await
            .expect("ask answers");
        assert!(json_of(&other)["challenge"]["handle"].is_string());

        let later = jojobot
            .clone()
            .on_clock(jojobot_domain::clock::Clock::stating(jiff::civil::date(
                2026, 10, 11,
            )));
        let renewed = later
            .ask_for_code(&gamma(), jiff::civil::date(2026, 10, 11))
            .await
            .expect("ask answers");
        assert!(json_of(&renewed)["challenge"]["handle"].is_string());
        let fields = jojobot.memory.fields(&first).await.expect("fields ok");
        assert_eq!(fields["state"], "replaced", "{fields:?}");
    }

    /// **P3: a boot whose session store is down says that, not that it was
    /// handed a choice.** The code is not spent either way.
    #[tokio::test]
    async fn a_coded_boot_names_an_unavailable_session_store() {
        let memory = Arc::new(InMemoryMemory::seeded());
        let jojobot = Jojobot::new(
            memory,
            Arc::new(crate::memory::testing::SpySearch::default()),
            Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
            Arc::new(UnreachableRuns(
                jojobot_domain::session::testing::InMemorySessions::new(),
            )),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            seeded_registry(),
        );
        crate::mailboxes::testing::name_the_operator(&jojobot, "milhouse").await;
        make_bot(&jojobot, "gamma").await;
        let (handle, code) = asked(&jojobot).await;
        let booted = json_of(
            &jojobot
                .start_here(Parameters(boot_with_code("gamma", handle.as_str(), &code)))
                .await
                .expect("boot answers"),
        );
        assert_eq!(booted["session"]["available"], false, "{booted}");
        assert_eq!(
            booted["session"]["trust"]["code"]["spent"], false,
            "{booted}"
        );
        assert_eq!(
            booted["session"]["trust"]["code"]["because"], "session_store_unavailable",
            "{booted}"
        );
        let fields = jojobot.memory.fields(&handle).await.expect("fields ok");
        assert_eq!(fields["state"], "issued", "nothing was spent: {fields:?}");
    }
}
