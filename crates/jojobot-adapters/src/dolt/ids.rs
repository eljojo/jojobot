//! **The ids this store mints** — drawn, never counted.
//!
//! A counted id is a fact about the store that rides out on every answer: it
//! says how many came before it, and it reads as a sequence a caller can walk.
//! A drawn one says nothing and continues nothing (rule 20), and it is the same
//! draw the session handle uses — one alphabet, one entropy source
//! ([`jojobot_domain::handle`]).
//!
//! **The store answers whether a candidate is free**, not this process: the
//! probe reads the primary key inside the caller's transaction, so the answer
//! covers every row rather than the ones one process happens to remember.
//!
//! **Six characters**, where the session handle is four. Handles address the
//! few dozen runs that are live; messages and chronology entries are never
//! removed and accumulate for as long as the operator has mail, so the space
//! has to stay far ahead of the board rather than of the moment.

use std::sync::Arc;

use sqlx::{MySql, Transaction};

/// How many characters a drawn id is on this store.
pub(super) const ID_LEN: usize = 6;

/// How many collisions a draw rides out before giving up.
///
/// Reaching it means the store holds a large fraction of a billion ids, which
/// is not a state this server gets into — but the alternative to a bound is a
/// loop with no way out, and an honest failure is better than a hang.
const ATTEMPTS: usize = 64;

/// Where a candidate id comes from. **A value rather than a call**, so a test
/// can supply the draw and watch the collision path: entropy will not produce a
/// collision on demand, and a retry nobody has watched happen is a retry nobody
/// knows works.
pub(super) type Draw = Arc<dyn Fn() -> String + Send + Sync>;

/// The production draw: OS entropy, at this store's length.
pub(super) fn drawing() -> Draw {
    Arc::new(|| jojobot_domain::handle::draw(ID_LEN))
}

/// Draw until the store says the candidate is free, or give up.
///
/// `taken` is a query this module's own callers write as a literal — never a
/// caller's string — that returns a row when the id is already there. `scope`
/// is bound first where a table's key has two parts (a chronology entry is
/// unique within its session), and left out where the id alone is the key.
///
/// `Ok(None)` means the draws ran out. It is not an error here because each
/// store says it in its own vocabulary.
pub(super) async fn draw_free(
    tx: &mut Transaction<'_, MySql>,
    draw: &Draw,
    taken: &'static str,
    scope: Option<&str>,
) -> Result<Option<String>, sqlx::Error> {
    for _ in 0..ATTEMPTS {
        let candidate = draw();
        let mut probe = sqlx::query_scalar::<_, i64>(taken);
        if let Some(scope) = scope {
            probe = probe.bind(scope);
        }
        let held: Option<i64> = probe.bind(&candidate).fetch_optional(&mut **tx).await?;
        if held.is_none() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}
