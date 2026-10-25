//! The origin a stored type cell reads back as.

use super::*;

/// Where a stored type came from.
///
/// **A token nothing can read is treated as shipped**, which is the safe
/// branch here (rule 62). The two mistakes are not equal: reading a shipped
/// type as a caller's lets the next declaration overwrite it and nobody sees
/// it happen, while reading a caller's type as shipped refuses one write and
/// tells the caller exactly what to do instead. This is the one place `holds`
/// takes the other branch, and for the same reason — there, the permissive
/// answer is the one that loses nothing.
pub(super) fn read_origin(token: &str) -> Origin {
    Origin::of_token(token).unwrap_or(Origin::Shipped)
}
