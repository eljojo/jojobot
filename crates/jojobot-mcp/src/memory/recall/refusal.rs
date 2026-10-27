use super::*;

/// **The refusal for a name that is no view.** The views that exist ride in the
/// sentence rather than as candidates: a candidate carries a reason the guard
/// flagged it, and none of these was flagged, they are simply what there is.
/// A similarity screen would be worse still, since it answers a wild guess with
/// nothing, which reads as "there are no views".
pub(super) fn no_such_view(handle: &EntityId, known: &[Entity]) -> CallToolResult {
    let names = known
        .iter()
        .map(|e| e.id.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    blocked_body(
        "NoSuchView",
        handle,
        &[],
        match names.is_empty() {
            true => format!(
                "Nothing was read: no view is named '{}', and there are no views \
                 here yet. Declare one with add_entity of kind view.",
                handle.as_str(),
            ),
            false => format!(
                "Nothing was read: no view is named '{}'. The views here are: \
                 {names}. Ask for one of those, or declare your own with \
                 add_entity of kind view.",
                handle.as_str(),
            ),
        },
    )
}
