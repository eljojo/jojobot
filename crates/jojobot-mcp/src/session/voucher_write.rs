//! **The one door through which jojobot writes a voucher.**
//!
//! A voucher is its own shipped kind and only the engine writes one: the
//! caller-facing verbs turn the write back (see
//! [`jojobot_domain::voucher::refuses_voucher_write`]), and this function writes
//! through the `Memory` trait directly, the way [`super::role_write`] writes a
//! role. The object and its one claim are made in one act, so a voucher is never
//! stored without its keys.
//!
//! Nothing calls this yet. The operator's code and the presenting path land in
//! later slices and write through it.

use jiff::civil::Date;
use jojobot_domain::memory::{
    EntityId, EntityKind, Guarded, MemoryError, NewEntity, NewFact, guard,
};
use jojobot_domain::voucher::Voucher;

use crate::Jojobot;

/// How many characters the slug of a voucher's handle has.
const SLUG_LEN: usize = 8;

/// How many slugs are drawn before the write gives up. A draw collides only with
/// a voucher already stored, so reaching this means the store is nearly full of
/// them, which no instance is.
const DRAWS: usize = 8;

impl Jojobot {
    /// **Write one voucher, whole, and return its handle.** The handle is
    /// `voucher:` and a drawn slug, so it says nothing about what it grants.
    ///
    /// The creation is jojobot's own, so a near miss against another voucher is
    /// not a question for a caller: the token the guard minted answers it. A
    /// slug that is already taken cannot be forced, so it is drawn again.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn issue_voucher(
        &self,
        voucher: &Voucher,
        today: Date,
    ) -> Result<EntityId, MemoryError> {
        for _ in 0..DRAWS {
            let slug = jojobot_domain::handle::draw(SLUG_LEN);
            let id = EntityId::new(EntityKind::VOUCHER, &slug);
            let new = NewEntity::new(id.clone(), format!("Voucher {slug}"), "jojobot");
            let first = NewFact {
                fields: voucher.fields(),
                ..NewFact::about(
                    id.clone(),
                    format!(
                        "a voucher for {} on {}",
                        voucher.right.as_str(),
                        voucher.target.as_str()
                    ),
                    today,
                )
            };
            match self
                .memory
                .add_entity_with_first_claim(new.clone(), first.clone())
                .await?
            {
                Guarded::Written(_) => return Ok(id),
                Guarded::Blocked {
                    attempted,
                    candidates,
                } => {
                    // **A block on another handle is a pointer at nothing**: the
                    // target or the parent does not exist. Drawing again cannot
                    // change that, so it is the answer.
                    if attempted != id {
                        return Err(MemoryError::InvalidFact(format!(
                            "a voucher points at '{attempted}', which jojobot does not know"
                        )));
                    }
                    let token = guard::override_token(&attempted, &candidates);
                    let new = NewEntity {
                        override_token: Some(token),
                        ..new
                    };
                    if let Guarded::Written(_) =
                        self.memory.add_entity_with_first_claim(new, first).await?
                    {
                        return Ok(id);
                    }
                }
            }
        }
        Err(MemoryError::InvalidFact(
            "no free handle could be drawn for a voucher".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use jojobot_domain::voucher::VoucherState;

    fn root(state: VoucherState) -> Voucher {
        Voucher {
            right: "boot-as".into(),
            target: "bot:marge".into(),
            holder: "abcd".into(),
            issuer: "wxyz".into(),
            parent: None,
            state,
        }
    }

    async fn marge(jojobot: &Jojobot) {
        make_bot(jojobot, "marge").await;
    }

    fn today() -> Date {
        jiff::civil::date(2026, 10, 9)
    }

    /// **The engine writes a voucher in each of the five states, and each reads
    /// back whole.** The state is the stored word, the six keys are the declared
    /// ones, and the object is of the shipped kind. A caller's write of the same
    /// record is turned back (the story), so this is the only path that lands.
    #[tokio::test]
    async fn the_engine_writes_a_voucher_in_every_state_and_reads_it_back_whole() {
        let jojobot = handler();
        marge(&jojobot).await;
        for state in VoucherState::ALL {
            let id = jojobot
                .issue_voucher(&root(state), today())
                .await
                .unwrap_or_else(|e| panic!("{state:?}: {e}"));
            assert_eq!(id.kind(), Some(EntityKind::VOUCHER), "{state:?}: {id}");
            let fields = jojobot.memory.fields(&id).await.expect("fields ok");
            assert_eq!(
                fields.get("state").map(String::as_str),
                Some(state.as_token())
            );
            assert_eq!(fields.get("right").map(String::as_str), Some("boot-as"));
            assert_eq!(fields.get("target").map(String::as_str), Some("bot:marge"));
            assert_eq!(fields.get("holder").map(String::as_str), Some("abcd"));
            assert_eq!(fields.get("issuer").map(String::as_str), Some("wxyz"));
            assert!(
                !fields.contains_key("parent"),
                "a root has none: {fields:?}"
            );
        }
        let all = jojobot
            .memory
            .list_entities(Some(EntityKind::VOUCHER))
            .await
            .expect("list ok");
        assert_eq!(all.len(), VoucherState::ALL.len(), "{all:?}");
    }

    /// **A voucher passed down names the one it descends from, and the link is
    /// served as that voucher's handle.** The positive of the walk back towards
    /// the root; the negative is that a parent which does not exist is refused,
    /// because a pointer at nothing is never stored.
    #[tokio::test]
    async fn a_voucher_names_its_parent_and_a_parent_that_is_nothing_is_refused() {
        let jojobot = handler();
        marge(&jojobot).await;
        let first = jojobot
            .issue_voucher(&root(VoucherState::Held), today())
            .await
            .expect("the root is written");
        let child = Voucher {
            parent: Some(first.clone()),
            holder: "efgh".into(),
            issuer: "abcd".into(),
            ..root(VoucherState::Issued)
        };
        let passed = jojobot
            .issue_voucher(&child, today())
            .await
            .expect("the child is written");
        let fields = jojobot.memory.fields(&passed).await.expect("fields ok");
        assert_eq!(
            fields.get("parent").map(String::as_str),
            Some(first.as_str()),
            "{fields:?}",
        );

        let orphan = Voucher {
            parent: Some(EntityId("voucher:nothing".into())),
            ..root(VoucherState::Issued)
        };
        let refused = jojobot.issue_voucher(&orphan, today()).await;
        let refused = refused.expect_err("a parent that is nothing is stored");
        assert!(
            refused.to_string().contains("voucher:nothing"),
            "the refusal names the parent that is nothing: {refused}",
        );
        let all = jojobot
            .memory
            .list_entities(Some(EntityKind::VOUCHER))
            .await
            .expect("list ok");
        assert_eq!(all.len(), 2, "the refused one left nothing: {all:?}");
    }
}
