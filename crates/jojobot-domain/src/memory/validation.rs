//! The shape checks of a handle a write names.

use super::*;

/// **The shape half of [`validate_subject`]**, which needs no loaded kinds: the
/// slug grammar and the length. A configuration checked before the kinds load
/// asks this, so it holds a handle to the same grammar a write does.
pub fn validate_shape(subject: &EntityId) -> Result<(), MemoryError> {
    let s = subject.as_str();
    let shaped = s.len() <= 128 && slug_shaped(subject.slug());
    if !shaped {
        return Err(MemoryError::InvalidSubject(format!(
            "'{s}' is no handle: a handle is kind:slug, slug [a-z0-9-]+, at most 128 characters"
        )));
    }
    Ok(())
}

/// **A handle a new thing may be given**: the plain grammar, with no slash. A
/// `<collection>/<N>` handle names a member that already holds a number; a
/// thing made or renamed to one would be found before the member (a direct
/// match wins), so nothing may be given one.
pub fn validate_new_handle(id: &EntityId) -> Result<(), MemoryError> {
    if id.slug().contains('/') {
        return Err(MemoryError::InvalidEntity(format!(
            "'{id}' cannot be made: a slash and a number is how a member of a numbered \
             collection is read, not a handle to give a thing. Use letters, digits and hyphens"
        )));
    }
    Ok(())
}
