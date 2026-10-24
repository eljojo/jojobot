//! **A boundary is one blob for the whole room; a subject's own facts are a
//! slice of it.**
//!
//! [`crate::run::Boundary::world`] is a fixed, coarse capture — a listing plus
//! one broad search, concatenated as text — taken the same way at every
//! boundary regardless of what a lock asks. Matching a needle against the
//! whole blob risks a false answer that has nothing to do with the subject a
//! lock names: two subjects in the same phase can each hold a value the
//! other's lock must say it lacks, and a flat `contains` check cannot tell
//! them apart. This is what makes a `lacks` needle over the raw blob
//! unreliable in a way a `carries` needle over it is merely lucky.
//!
//! This module is the fix, and it is deliberately narrow: given the blob and
//! a subject, return only the text of that subject's own facts — every
//! rendered fact carries its subject under `subject.id`, or `parent.id` when
//! they are the same (see
//! `wire::hit_json`'s `Hit::Fact` arm), so no second read is needed to know
//! which ones are whose.

use serde_json::Value;

/// **The text a windowed lock should match against.**
///
/// `subject` is the lock's own `"subject"` argument, when it has one. A lock
/// that names no subject asks a kind- or store-wide question that a flat
/// listing already answers without ambiguity — an entity's existence and its
/// own metadata are not folded or filtered the way a fact's fields are — so
/// the whole blob is returned unchanged.
///
/// Named for a subject, only that subject's own fact hits survive: every
/// other entity's facts, and every other entity's own listing, are dropped.
/// **Nothing is invented if the blob will not parse** — the fallback is the
/// whole blob, the same answer a lock asking no subject gets, because a
/// windowed lock that cannot isolate is no worse off than one that never
/// tried.
pub fn boundary_text(world: &str, subject: Option<&str>) -> String {
    let Some(subject) = subject else {
        return world.to_string();
    };
    // `Boundary::world` is `format!("{entities}\n{everything}")` — two
    // complete JSON documents joined by one literal newline. A JSON string
    // value can only contain a raw newline when the string itself does, and
    // serde_json escapes those as `\n`, so the first line break is reliably
    // the seam between them.
    let Some((_entities, everything)) = world.split_once('\n') else {
        return world.to_string();
    };
    let Ok(parsed) = serde_json::from_str::<Value>(everything) else {
        return world.to_string();
    };
    let empty: Vec<Value> = Vec::new();
    let results = parsed["results"].as_array().unwrap_or(&empty);
    let mine: Vec<&Value> = results
        .iter()
        .filter(|hit| fact_subject(hit) == Some(subject))
        .collect();
    serde_json::to_string(&mine).unwrap_or_else(|_| world.to_string())
}

/// The subject of a fact hit; a distinct subject takes precedence over its home.
pub fn fact_subject(hit: &Value) -> Option<&str> {
    (hit["hit"] == "fact")
        .then(|| {
            hit["subject"]["id"]
                .as_str()
                .or(hit["parent"]["id"].as_str())
        })
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::boundary_text;
    use serde_json::json;

    #[test]
    fn compact_search_subject_is_explicit_or_its_parent_without_cross_attribution() {
        let world = format!(
            "{{}}\n{}",
            json!({"results": [
                {"hit": "fact", "parent": {"id": "person:milhouse"},
                 "content": "own-marker"},
                {"hit": "fact", "parent": {"id": "person:milhouse"},
                 "subject": {"id": "person:bart"}, "content": "foreign-marker"},
                {"hit": "fact", "parent": {"id": "person:bart"},
                 "subject": {"id": "person:milhouse"}, "content": "elsewhere-marker"},
                {"hit": "entity", "parent": {"id": "person:milhouse"},
                 "content": "entity-marker"}
            ]})
        );
        let mine = boundary_text(&world, Some("person:milhouse"));
        assert!(
            mine.contains("own-marker") && mine.contains("elsewhere-marker"),
            "{mine}"
        );
        assert!(
            !mine.contains("foreign-marker") && !mine.contains("entity-marker"),
            "{mine}"
        );
        let other = boundary_text(&world, Some("person:bart"));
        assert!(other.contains("foreign-marker"), "{other}");
        assert!(
            !other.contains("own-marker") && !other.contains("elsewhere-marker"),
            "{other}"
        );
    }
}
