use super::*;

const R1_LOCATION_LIMIT: usize = 50;

pub(super) fn push_bounded(
    fields: &mut Vec<OwnedNowField>,
    left_out: &mut usize,
    handle: &str,
    key: &str,
    items: usize,
) {
    let shown = items.min(R1_LOCATION_LIMIT.saturating_sub(fields.len()));
    *left_out += items - shown;
    for _ in 0..shown {
        fields.push(OwnedNowField {
            entity: EntityId(handle.to_string()),
            key: key.to_string(),
        });
    }
}
