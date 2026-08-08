//! **A drawn handle** — the one place a server-minted id comes from.
//!
//! A handle is drawn from OS entropy, never counted and never derived from what
//! it names (rule 20). Three properties, each load-bearing and each the reason
//! there is one implementation rather than one per rail:
//!
//! * **short** — it rides on calls a person reads and repeats, so it is a few
//!   characters rather than a UUID;
//! * **hard to confuse** — the alphabet leaves out the glyphs that read as one
//!   another, so a handle copied by eye survives the trip;
//! * **opaque** — it says nothing about what it names, and nothing about how
//!   many came before it. A counted id answers "how much mail is there" to
//!   anybody who sees one, and reads as a sequence a caller can walk.
//!
//! **Length is the caller's**, because the rails do not accumulate alike: a
//! session handle addresses one of a few dozen live runs, and a message id is
//! never removed and accumulates for as long as the operator has mail.

/// The alphabet: **Crockford's base32, lowercased** — the digits and the
/// letters, minus `i`, `l`, `o` and `u`.
///
/// Those are the glyphs a reader mistakes for one another (`i`/`l`/`1`, `o`/`0`,
/// `u`/`v`), and a mistaken handle is one jojobot must refuse rather than
/// correct — correcting it means guessing what somebody meant. Every symbol is
/// inside the `[a-z0-9-]` an id accepts, so a drawn handle is always storable
/// wherever an id is.
pub const ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// Draw one candidate handle of `len` characters from OS entropy.
///
/// `256 % 32 == 0`, so the fold is uniform — no rejection sampling and no bias
/// toward the front of the alphabet.
///
/// **A candidate, not an id.** Whether it is free is the caller's question:
/// the registry asks its own map, and a store asks its primary key. Nothing
/// here can answer it, and a draw that pretended to would be answering for a
/// space it cannot see.
pub fn draw(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    // The OS entropy source failing is not a condition this server can serve
    // through: every handle it mints after that would be one it cannot promise
    // is unguessable.
    getrandom::fill(&mut bytes).expect("the OS entropy source is readable");
    bytes
        .iter()
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
        .collect()
}

/// Whether this string has the shape of a handle jojobot draws.
///
/// **Shape only** — a well-shaped handle may still name nothing, and the two
/// are told apart where they are answered, because "you mistyped it" and "that
/// one is gone" send a caller to different places.
pub fn is_drawn(candidate: &str, len: usize) -> bool {
    candidate.len() == len && candidate.bytes().all(|b| ALPHABET.contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The alphabet leaves out every glyph pair a reader confuses, and a drawn
    /// handle is storable wherever an id is.
    #[test]
    fn the_alphabet_excludes_the_confusable_glyphs_and_nothing_else() {
        assert_eq!(ALPHABET.len(), 32, "base32, or the fold below is biased");
        for confusable in [b'i', b'l', b'o', b'u'] {
            assert!(
                !ALPHABET.contains(&confusable),
                "{} reads as another glyph and must not be drawn",
                confusable as char
            );
        }
        let whole = String::from_utf8(ALPHABET.to_vec()).expect("ascii");
        assert!(
            whole
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
            "every symbol must be storable inside [a-z0-9-]: {whole}"
        );
    }

    /// A draw is the length it was asked for, in the alphabet, and it is not a
    /// constant — the last of those is what tells a draw from a stub.
    #[test]
    fn a_draw_has_the_shape_it_promises_and_is_not_a_constant() {
        for len in [4, 6] {
            let one = draw(len);
            assert!(is_drawn(&one, len), "{one:?} is not a handle of {len}");
            let many: std::collections::HashSet<String> =
                (0..64).map(|_| draw(len)).collect::<HashSet<_>>();
            assert!(
                many.len() > 1,
                "64 draws of {len} came back identical, which is a stub rather than entropy"
            );
        }
    }

    use std::collections::HashSet;

    /// Shape is checked against the length asked for, so a handle of one rail's
    /// length is not accepted where another's is expected.
    #[test]
    fn a_handle_of_the_wrong_length_is_not_this_rails_handle() {
        assert!(is_drawn("abcd", 4));
        assert!(!is_drawn("abcd", 6));
        assert!(!is_drawn("abcdef", 4));
        assert!(!is_drawn("abcdei", 6), "a confusable glyph is no handle");
        assert!(!is_drawn("ABCDEF", 6), "the alphabet is lowercase");
    }
}
