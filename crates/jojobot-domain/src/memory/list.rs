//! **How a stored list names a thing once.**
//!
//! A merge repoints every item that names the folded thing at the survivor, and
//! a list that named both ends up naming the survivor twice. The list keeps the
//! first item for each thing, in the order the list held them. **The merge
//! applies the rule when it rewrites a value, and a read applies it when the
//! merge left the value in place**, so both go through this one function and
//! cannot disagree.

/// **The items of a list with each thing named once**, at the place its first
/// item stood. `thing` says which thing an item names; two items naming one
/// thing are one, whatever else differs between them.
pub fn name_each_once<T, K: PartialEq>(items: Vec<T>, thing: impl Fn(&T) -> K) -> Vec<T> {
    let mut seen: Vec<K> = Vec::with_capacity(items.len());
    let mut kept: Vec<T> = Vec::with_capacity(items.len());
    for item in items {
        let named = thing(&item);
        if seen.contains(&named) {
            continue;
        }
        seen.push(named);
        kept.push(item);
    }
    kept
}

/// **A field value served from its items**, each with the thing it names or
/// `None` for loose text: the list names each thing once ([`name_each_once`]),
/// loose text is never merged with other loose text, and the items are joined
/// the way a list is written.
pub fn serve_values<T: PartialEq + Clone>(items: Vec<(Option<T>, String)>) -> String {
    let mut at = 0;
    name_each_once(
        items
            .into_iter()
            .map(|(thing, text)| {
                at += 1;
                (thing.ok_or(at), text)
            })
            .collect(),
        |(thing, _)| thing.clone(),
    )
    .into_iter()
    .map(|(_, text)| text)
    .collect::<Vec<_>>()
    .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_item_for_a_thing_keeps_its_place_and_its_spelling() {
        let kept = name_each_once(vec!["a:1", "b:2", "A:1", "c:3", "b:2"], |item| {
            item.to_lowercase()
        });
        assert_eq!(kept, ["a:1", "b:2", "c:3"]);
    }

    #[test]
    fn a_list_naming_each_thing_once_is_returned_as_it_was() {
        let kept = name_each_once(vec!["c", "a", "b"], |item| *item);
        assert_eq!(kept, ["c", "a", "b"]);
    }
}
