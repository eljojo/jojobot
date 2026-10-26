//! **How the gate reads what the recorded store held**, before the current binary
//! served it. The recording comes from the last deployed build, so the stored text
//! is in whatever shape that build wrote.

/// **A stored value with each item that names a thing replaced by the handle that
/// thing answers to.** An item is one piece of a comma list. It names a thing when
/// an entity wears it as a badge, with or without the `@#` mark: a field a type
/// declared a reference holds the bare id, and any other key holds the marked one.
/// **A record the build supplies wears no badge**: its id is its handle, and the
/// rule that is on stores the mark in front of that handle. A marked item in the
/// shape of a handle is therefore read as the handle. Separators and every other
/// item are untouched. An id nothing wears, which is no handle, is left as it is, so
/// the comparison against it fails loudly.
pub(super) fn with_badges_as_handles(
    value: &str,
    badges: &std::collections::BTreeMap<String, String>,
) -> String {
    value
        .split(',')
        .map(|piece| {
            let item = piece.trim();
            let marked = item.strip_prefix("@#");
            let bare = marked.unwrap_or(item);
            match badges.get(bare) {
                Some(handle) => piece.replacen(item, handle, 1),
                None if marked.is_some() && is_a_handle(bare) => piece.replacen(item, bare, 1),
                None => piece.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// `kind:slug`, as a handle is written: a kind of letters and hyphens, a slug of
/// lowercase letters, digits, hyphens and slashes.
fn is_a_handle(text: &str) -> bool {
    text.split_once(':').is_some_and(|(kind, slug)| {
        !kind.is_empty()
            && kind.chars().all(|c| c.is_ascii_lowercase() || c == '-')
            && !slug.is_empty()
            && slug
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '/')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn badges() -> std::collections::BTreeMap<String, String> {
        [("b4d63", "person:homer"), ("c9a01", "work:gamma")]
            .into_iter()
            .map(|(badge, handle)| (badge.to_string(), handle.to_string()))
            .collect()
    }

    /// **A thing that wears a badge is read as its handle**, marked or bare, in a
    /// list, with the spacing of the list kept.
    #[test]
    fn an_item_a_thing_wears_is_read_as_its_handle() {
        assert_eq!(with_badges_as_handles("@#b4d63", &badges()), "person:homer");
        assert_eq!(with_badges_as_handles("c9a01", &badges()), "work:gamma");
        assert_eq!(
            with_badges_as_handles("kitchen, @#b4d63, c9a01", &badges()),
            "kitchen, person:homer, work:gamma"
        );
    }

    /// **A record the build supplies is stored behind the mark with its handle
    /// for an id**, as the recorded store holds `looks_at` on the recorded person.
    #[test]
    fn a_marked_handle_no_badge_wears_is_read_as_the_handle() {
        assert_eq!(
            with_badges_as_handles("@#view:colleagues", &badges()),
            "view:colleagues"
        );
        assert_eq!(
            with_badges_as_handles("kitchen, @#view:colleagues, @#b4d63", &badges()),
            "kitchen, view:colleagues, person:homer"
        );
    }

    /// **What no thing wears and what is no handle stays as written**, so the
    /// comparison against it fails: an id of no entity, and a bare handle.
    #[test]
    fn an_id_nothing_wears_stays_as_written() {
        assert_eq!(with_badges_as_handles("@#e77f0", &badges()), "@#e77f0");
        assert_eq!(with_badges_as_handles("e77f0", &badges()), "e77f0");
        assert_eq!(
            with_badges_as_handles("view:colleagues", &badges()),
            "view:colleagues"
        );
        assert_eq!(
            with_badges_as_handles("@#View:Colleagues", &badges()),
            "@#View:Colleagues"
        );
    }
}
