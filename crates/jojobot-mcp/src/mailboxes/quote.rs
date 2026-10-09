//! **A post to the operator quotes them by address.**
//!
//! `@kind:slug#fN` in a post to the operator names a record of their words. Each
//! must exist, be readable to the writer and be testimony: an inference is a
//! bot's guess and cannot be handed back as the operator's word. On acceptance
//! the words are written into the message once, beside the citation, so a later
//! correction of the source never changes what an old message says was sent.
//!
//! Only this shape is read. A plain `@kind:slug` is the mention it always was,
//! and quotation marks typed by hand are punctuation.

use super::*;
use jojobot_domain::mailbox::quote::{expanded, quoted_addresses};
use jojobot_domain::memory::{Fact, FactAddress, FactStatus, Provenance};

/// The form a refusal names, so a caller can fix the post from the answer alone.
const FORM: &str = "To quote the operator, write the address of a claim they said, as \
                    @kind:slug#fN, where the claim's provenance is testimony. To mention \
                    something without quoting it, leave the #fN off";

fn refused(address: &FactAddress, why: &str) -> MailboxError {
    MailboxError::InvalidMessage(format!(
        "nothing was written: '@{}#{}' {why}. {FORM}.",
        address.home, address.local
    ))
}

/// The words as they are stored: the claim's content in quotation marks, then
/// the citation, the provenance, the day the operator said it and, when it
/// stopped being current, that it is archived.
fn words_of(address: &FactAddress, fact: &Fact) -> String {
    let archived = match fact.status {
        FactStatus::Archived => ", archived",
        FactStatus::Active => "",
    };
    format!(
        "\"{}\" (@{}#{}, testimony, said {}{archived})",
        fact.content, address.home, address.local, fact.recorded_at
    )
}

impl Jojobot {
    /// `body` with every quoted address expanded, or the refusal that names the
    /// first address that cannot be quoted. A body with no address comes back as
    /// it was.
    pub(crate) async fn quotes_expanded(&self, body: &str) -> Result<String, MailboxError> {
        let quoted = quoted_addresses(body);
        if quoted.is_empty() {
            return Ok(body.to_string());
        }
        let mut words = Vec::with_capacity(quoted.len());
        for found in &quoted {
            let address = &found.address;
            let facts = self
                .memory
                .recall(&address.home)
                .await
                .map_err(|_| refused(address, "names no record jojobot can read"))?;
            let fact = facts
                .iter()
                .find(|fact| fact.home == address.home && fact.id == address.local)
                .ok_or_else(|| refused(address, "names no claim jojobot holds"))?;
            if fact.provenance != Provenance::Testimony {
                return Err(refused(
                    address,
                    &format!(
                        "is {}, not testimony, so it is not the operator's word",
                        fact.provenance.as_token()
                    ),
                ));
            }
            words.push(words_of(address, fact));
        }
        Ok(expanded(body, &quoted, &words))
    }
}
