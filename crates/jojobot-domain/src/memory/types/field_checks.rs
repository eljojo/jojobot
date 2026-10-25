//! Literal constraints keep the original value; reference kinds may be resolved.

use super::*;

impl Field {
    /// **Whether a value holds what this key was declared to hold** — the value
    /// type, and for a reference the kind it points at.
    ///
    /// This rather than [`ValueType::holds`] is what a record is measured
    /// against, because the kind a reference names is part of the declaration
    /// and a check that read the value type alone would call
    /// `pet:santas-little-helper` a good venue.
    pub fn accepts(&self, value: &str) -> bool {
        self.accepts_with_kind(value, &|item| EntityId(item.trim().to_string()).kind())
    }

    pub(in crate::memory) fn accepts_with_kind(
        &self,
        value: &str,
        kind_of: &dyn Fn(&str) -> Option<EntityKind>,
    ) -> bool {
        // **Every item, and a list with nothing in it holds nothing badly.** A
        // key declared to hold zero or more is answered by zero.
        self.items(value)
            .into_iter()
            .all(|item| self.accepts_one(item, kind_of))
    }

    /// One item of what this key holds — the whole value for an ordinary key.
    fn accepts_one(&self, value: &str, kind_of: &dyn Fn(&str) -> Option<EntityKind>) -> bool {
        if !self.holds.holds(value) {
            return false;
        }
        // **Whole, trimmed and case-sensitive**, the same comparison
        // [`Compare::Equals`] makes. A set whose members differ only by case is
        // a set somebody wrote badly, and folding the case here would hide that
        // rather than help anybody.
        if let Some(values) = &self.one_of
            && !values.iter().any(|allowed| allowed == value.trim())
        {
            return false;
        }
        match self.points_at {
            None => true,
            Some(kind) => kind_of(value) == Some(kind),
        }
    }
}
