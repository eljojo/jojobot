//! **The receipt a promotion to settled carries.**
//!
//! Settling an open claim says the doubt is over. The receipt says why, in one
//! of two shapes that are told apart: a check somebody ran, or the operator's
//! own confirmation. It is stored on the claim and on each write of it, and it
//! never changes the claim's provenance: a settled inference reads back as an
//! inference.

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use super::{MemoryError, Standing};

/// **A receipt rides only a claim that will stand settled** (rule 498). It
/// records why a claim is settled, so it goes with a move to settled, a claim
/// that is settled already, or a capture that lands settled, and never on its
/// own on a claim that stays open. The one question every write asks of a
/// receipt, on the standing the claim will have: an edit and a capture alike,
/// in every store.
pub fn rides_only_settled(
    receipt: Option<&Receipt>,
    standing: Standing,
) -> Result<(), MemoryError> {
    if receipt.is_some() && standing != Standing::Settled {
        return Err(MemoryError::ReceiptWithoutSettling);
    }
    Ok(())
}

/// What a promotion to settled carries. The serialized form is what a store
/// keeps in its `receipt` column, so the two tags are stored spellings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Receipt {
    /// **A check that ran**, written so another agent can run it again.
    Check(String),
    /// **The operator confirmed it**, on a day, in a place a reader can find:
    /// a message id or a session.
    Confirmed {
        /// The day the operator confirmed it.
        on: Date,
        /// Where: a message id or the session.
        at: String,
    },
}

impl Receipt {
    /// This receipt with its text trimmed, or the refusal naming what is
    /// missing. A blank check or a blank place is no receipt: the field exists
    /// so a later reader can run the check again or find the confirmation.
    pub fn validated(self) -> Result<Receipt, MemoryError> {
        match self {
            Receipt::Check(text) => {
                let text = text.trim().to_string();
                if text.is_empty() {
                    return Err(MemoryError::InvalidFact(
                        "a check receipt says what was checked, in enough detail that another \
                         agent can run the check again; it was empty"
                            .to_string(),
                    ));
                }
                Ok(Receipt::Check(text))
            }
            Receipt::Confirmed { on, at } => {
                let at = at.trim().to_string();
                if at.is_empty() {
                    return Err(MemoryError::InvalidFact(
                        "a confirmed receipt says where the operator confirmed it, a message id \
                         or the session, beside the day; the place was empty"
                            .to_string(),
                    ));
                }
                Ok(Receipt::Confirmed { on, at })
            }
        }
    }
}
