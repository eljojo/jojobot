//! **A bot quotes the operator by address, never from memory.**
//!
//! `@kind:slug#fN` written into a post to the operator cites a record. This finds
//! those addresses in a body and splices their expansions back in; whether an
//! address may be quoted at all is the caller's question, because it needs the
//! store.
//!
//! **Only the exact shape is an address.** `@kind:slug` alone is a mention and
//! stays one; `#` followed by anything but `f` and digits is prose. Plain
//! quotation marks are never read as a quote: punctuation is not attribution.

use std::ops::Range;

use crate::memory::FactAddress;

/// One address found in a body, and where it sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quoted {
    /// The bytes of the body the address occupies, `@` included.
    pub span: Range<usize>,
    /// The record it names.
    pub address: FactAddress,
}

fn is_handle_byte(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b':' || b == b'-'
}

/// Every `@kind:slug#fN` in `body`, in order of appearance.
///
/// **An address inside code is not one.** Text in an inline span or a fenced
/// block is literal (rule 450), and the one definition of where code is belongs
/// to the mention walk, so a body cannot be literal to a mention and a quote to
/// this scan.
pub fn quoted_addresses(body: &str) -> Vec<Quoted> {
    let bytes = body.as_bytes();
    let mut found = Vec::new();
    let mut resume = 0;
    for start in crate::memory::mention::outside_code(body) {
        if start < resume {
            continue;
        }
        let mut j = start + 1;
        while j < bytes.len() && is_handle_byte(bytes[j]) {
            j += 1;
        }
        if j + 1 < bytes.len() && bytes[j] == b'#' && bytes[j + 1] == b'f' {
            let mut k = j + 2;
            while k < bytes.len() && bytes[k].is_ascii_digit() {
                k += 1;
            }
            let ends_there = bytes
                .get(k)
                .is_none_or(|b| !(b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_'));
            if k > j + 2
                && ends_there
                && let Ok(address) = FactAddress::parse(&body[start + 1..k])
            {
                found.push(Quoted {
                    span: start..k,
                    address,
                });
                resume = k;
            }
        }
    }
    found
}

/// `body` with each address replaced by the matching entry of `expansions`,
/// which the caller built in the order [`quoted_addresses`] returned them.
pub fn expanded(body: &str, quoted: &[Quoted], expansions: &[String]) -> String {
    debug_assert_eq!(quoted.len(), expansions.len());
    let mut out = String::with_capacity(body.len());
    let mut last = 0;
    for (found, words) in quoted.iter().zip(expansions) {
        out.push_str(&body[last..found.span.start]);
        out.push_str(words);
        last = found.span.end;
    }
    out.push_str(&body[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The shipped kinds are loaded first**, because finding an address asks
    /// the process-wide kind set whether its subject names a kind. A case that
    /// relied on another test having loaded it answers differently alone.
    fn quoted(body: &str) -> Vec<Quoted> {
        crate::memory::kinds::load_shipped();
        quoted_addresses(body)
    }

    fn addresses(body: &str) -> Vec<String> {
        quoted(body)
            .iter()
            .map(|q| format!("{}#{}", q.address.home, q.address.local))
            .collect()
    }

    #[test]
    fn an_address_is_found_with_punctuation_around_it() {
        assert_eq!(
            addresses("You said (@person:alpha#f12), and @topic:all-rules#f3."),
            ["person:alpha#f12", "topic:all-rules#f3"]
        );
    }

    #[test]
    fn a_mention_and_other_hashes_are_not_addresses() {
        for body in [
            "see @person:alpha",
            "see @person:alpha#heading",
            "see @person:alpha#f",
            "see @person:alpha#f12x",
            "see @alpha#f1",
            "a #f1 on its own, and mail@host#f2",
            "plain \"quotation marks\" around words",
        ] {
            assert_eq!(addresses(body), Vec::<String>::new(), "{body}");
        }
    }

    /// **An address inside code is literal**, as a mention is: code is where a
    /// writer shows an address rather than cites one. The same address outside
    /// code in the same body is still found, in its own place.
    #[test]
    fn an_address_inside_code_is_not_quoted_and_one_outside_it_is() {
        for body in [
            "the form is `@person:alpha#f1`",
            "```\n@person:alpha#f1\n```\n",
            "~~~\n@person:alpha#f1\n~~~\n",
        ] {
            assert_eq!(addresses(body), Vec::<String>::new(), "{body}");
        }
        let body = "`@person:alpha#f1` is the form, and @person:alpha#f1 is the quote";
        let found = quoted(body);
        assert_eq!(found.len(), 1, "{body}");
        assert_eq!(&body[found[0].span.clone()], "@person:alpha#f1");
        assert_eq!(
            Some(found[0].span.start),
            body.rfind('@'),
            "the one outside the span: {body}"
        );
    }

    #[test]
    fn the_same_address_twice_is_found_twice_in_place() {
        let body = "@person:alpha#f1 and @person:alpha#f1";
        let found = quoted(body);
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].span, 21..37);
        let spliced = expanded(body, &found, &["A".into(), "B".into()]);
        assert_eq!(spliced, "A and B");
    }
}
