//! **Order keys: a string between two neighbours, minted by the server.**
//!
//! A document's sections have an order, and inserting one between two others
//! must not renumber the rest. Each section carries a key, and the keys sort as
//! plain text. A caller says *insert after this section* and never picks a number
//! (rule 449): [`between`] mints a key that sorts strictly between the keys on
//! either side, so an insert mints one key and a move mints one key for the section
//! it moves. A move of a document section also writes the blank-line lead of each
//! section it puts beside another, and no word of any section changes.
//!
//! **Keys are fractions.** Each character is a digit of base 62, and a key read
//! as `0.d1d2d3…` is a number between 0 and 1; a missing digit is `0`. Text order
//! and numeric order agree because the digits are in byte order. **A key never
//! ends in `0`**, because `0` after the last digit changes nothing numerically
//! and would leave no key between a key and itself padded.

/// The digits, in byte order, so that comparing keys as text compares them as
/// numbers.
const DIGITS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Why no key can be minted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OrderError {
    /// A key that is empty, holds a character outside the digits, or ends in `0`.
    #[error("'{0}' is not an order key")]
    NotAKey(String),
    /// The lower key does not sort before the upper one, so nothing is between.
    #[error("no key sorts between '{lower}' and '{upper}'")]
    NoRoom {
        /// The lower bound.
        lower: String,
        /// The upper bound.
        upper: String,
    },
}

fn index(c: u8) -> usize {
    DIGITS.iter().position(|d| *d == c).unwrap_or(0)
}

fn is_a_key(key: &str) -> bool {
    !key.is_empty() && key.bytes().all(|b| DIGITS.contains(&b)) && !key.ends_with('0')
}

/// A key strictly between `lower` and `upper`. `None` below is the start of the
/// order and `None` above is its end, so `between(None, None)` is the first key
/// of an empty order.
pub fn between(lower: Option<&str>, upper: Option<&str>) -> Result<String, OrderError> {
    for key in lower.into_iter().chain(upper) {
        if !is_a_key(key) {
            return Err(OrderError::NotAKey(key.to_string()));
        }
    }
    if let (Some(lower), Some(upper)) = (lower, upper)
        && lower >= upper
    {
        return Err(OrderError::NoRoom {
            lower: lower.to_string(),
            upper: upper.to_string(),
        });
    }
    let minted = midpoint(lower.unwrap_or("").as_bytes(), upper.map(str::as_bytes));
    Ok(String::from_utf8(minted).expect("the digits are ASCII"))
}

/// The digits strictly between `a` (padded with `0`) and `b`. `b` of `None` is
/// the end of the order.
fn midpoint(a: &[u8], b: Option<&[u8]>) -> Vec<u8> {
    if let Some(b) = b {
        // The longest prefix the two share, with `a` read as padded by zeros.
        let mut shared = 0;
        while shared < b.len() && a.get(shared).copied().unwrap_or(b'0') == b[shared] {
            shared += 1;
        }
        if shared > 0 {
            let mut out = b[..shared].to_vec();
            out.extend(midpoint(a.get(shared..).unwrap_or(&[]), Some(&b[shared..])));
            return out;
        }
    }
    let low = a.first().map_or(0, |c| index(*c));
    let high = b.map_or(62, |b| index(b[0]));
    if high - low > 1 {
        return vec![DIGITS[(low + high).div_ceil(2)]];
    }
    // The first digits are neighbours. When `b` goes on, its first digit alone is
    // a key between them; otherwise keep `a`'s digit and look one place further.
    if let Some(b) = b
        && b.len() > 1
    {
        return vec![b[0]];
    }
    let mut out = vec![DIGITS[low]];
    out.extend(midpoint(a.get(1..).unwrap_or(&[]), None));
    out
}

/// `count` keys, in order, all strictly between `lower` and `upper`, spread so
/// that none is much longer than another. Minted by halves rather than one after
/// the other, because a key minted after each one's predecessor gains a character
/// every few keys.
pub fn spread(
    lower: Option<&str>,
    upper: Option<&str>,
    count: usize,
) -> Result<Vec<String>, OrderError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let middle = between(lower, upper)?;
    let before = count / 2;
    let mut keys = spread(lower, Some(&middle), before)?;
    keys.push(middle.clone());
    keys.extend(spread(Some(&middle), upper, count - before - 1)?);
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A key sorts strictly between its neighbours, in text order.** Checked on
    /// neighbours of every shape: the ends, adjacent digits, a longer key above a
    /// shorter, and keys that share a prefix.
    #[test]
    fn a_key_sorts_strictly_between_its_neighbours() {
        let cases: &[(Option<&str>, Option<&str>)] = &[
            (None, None),
            (Some("V"), None),
            (None, Some("V")),
            (Some("1"), Some("2")),
            (Some("1"), Some("11")),
            (Some("1z"), Some("2")),
            (Some("abc"), Some("abd")),
            (Some("abc"), Some("abcz")),
            (Some("zzz"), None),
            (None, Some("01")),
        ];
        for (lower, upper) in cases {
            let key = between(*lower, *upper).expect("room");
            assert!(is_a_key(&key), "{key} is not a key");
            if let Some(lower) = lower {
                assert!(*lower < key.as_str(), "{lower} < {key}");
            }
            if let Some(upper) = upper {
                assert!(key.as_str() < *upper, "{key} < {upper}");
            }
        }
    }

    /// **Keys can be minted between any two keys, however many times.** A seeded
    /// run of inserts at random places keeps the whole list strictly increasing,
    /// and every key a key that can be split again.
    #[test]
    fn many_inserts_at_random_places_keep_the_order() {
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut keys: Vec<String> = Vec::new();
        for _ in 0..2000 {
            let at = (next() % (keys.len() as u64 + 1)) as usize;
            let lower = at.checked_sub(1).map(|i| keys[i].as_str());
            let upper = keys.get(at).map(String::as_str);
            let key = between(lower, upper).expect("room");
            keys.insert(at, key);
        }
        assert!(
            keys.windows(2).all(|pair| pair[0] < pair[1]),
            "the order broke"
        );
        assert!(keys.iter().all(|key| is_a_key(key)));
    }

    /// **A spread of keys is ordered, inside its bounds, and balanced.** Five
    /// hundred keys appended one after the other would gain a character every few
    /// keys; spread by halves they stay short.
    #[test]
    fn a_spread_is_ordered_inside_its_bounds_and_short() {
        let keys = spread(Some("5"), Some("8"), 500).expect("room");
        assert_eq!(keys.len(), 500);
        assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(keys.first().is_some_and(|key| key.as_str() > "5"));
        assert!(keys.last().is_some_and(|key| key.as_str() < "8"));
        let longest = keys.iter().map(String::len).max().unwrap_or(0);
        assert!(longest <= 6, "the longest key is {longest} characters");
        assert_eq!(
            spread(None, None, 0).expect("nothing"),
            Vec::<String>::new()
        );
    }

    /// **Bounds that are not keys, or that leave no room, are refused by name.**
    #[test]
    fn bounds_that_are_not_keys_or_leave_no_room_are_refused() {
        assert_eq!(
            between(Some("10"), None),
            Err(OrderError::NotAKey("10".into()))
        );
        assert_eq!(between(Some(""), None), Err(OrderError::NotAKey("".into())));
        assert_eq!(
            between(Some("a-"), None),
            Err(OrderError::NotAKey("a-".into()))
        );
        assert_eq!(
            between(Some("b"), Some("a")),
            Err(OrderError::NoRoom {
                lower: "b".into(),
                upper: "a".into()
            })
        );
        assert_eq!(
            between(Some("a"), Some("a")),
            Err(OrderError::NoRoom {
                lower: "a".into(),
                upper: "a".into()
            })
        );
    }
}
