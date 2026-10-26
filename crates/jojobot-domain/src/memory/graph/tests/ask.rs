//! Builders for the queries the cases of `graph/tests.rs` ask. They set the one thing a
//! case is about and leave every other field at `GraphQuery::default()`. They are test code
//! and add nothing to the production API; at 9.3.8 they fold into the constructors of
//! the prepared question.

use super::*;

/// The query for one entity's facts.
pub(super) fn about(handle: impl AsRef<str>) -> GraphQuery {
    GraphQuery::subject(EntityId(handle.as_ref().to_string()))
}

/// The query for every object of one kind.
pub(super) fn browse(kind: EntityKind) -> GraphQuery {
    selecting(Selection {
        kind: Some(kind),
        ..Selection::default()
    })
}

/// The query for a selection built by hand.
pub(super) fn selecting(select: Selection) -> GraphQuery {
    GraphQuery {
        select,
        ..GraphQuery::default()
    }
}

impl GraphQuery {
    /// Asks for the handles and edges only: no facts.
    pub(super) fn bare(mut self) -> Self {
        self.include.facts = false;
        self
    }

    /// Asks for facts, or for none.
    pub(super) fn facts(mut self, on: bool) -> Self {
        self.include.facts = on;
        self
    }

    /// Asks for the records with this status only.
    pub(super) fn listing(mut self, status: Option<FactStatus>) -> Self {
        self.include.status = status;
        self
    }

    /// Serves a shape's sources beside it.
    pub(super) fn with_sources(mut self) -> Self {
        self.include.stood_for = true;
        self
    }

    /// Asks for the objects that hold what these filters say.
    pub(super) fn filtering(mut self, fields: Vec<FieldFilter>) -> Self {
        self.select.fields = fields;
        self
    }

    /// Asks for the records near a day.
    pub(super) fn near(mut self, near: Nearness) -> Self {
        self.select.near = Some(near);
        self
    }

    /// Asks for each object's prose beside its facts.
    pub(super) fn with_prose(mut self) -> Self {
        self.include.prose = true;
        self
    }

    /// Replaces the whole `Include`.
    pub(super) fn including(mut self, include: Include) -> Self {
        self.include = include;
        self
    }

    /// Asks as this identity.
    pub(super) fn by(mut self, who: impl AsRef<str>) -> Self {
        self.select.asked_by = Some(EntityId(who.as_ref().to_string()));
        self
    }

    /// Replaces the whole walk, as a case that holds one in a variable has it.
    pub(super) fn following(mut self, follow: Option<Follow>) -> Self {
        self.follow = follow;
        self
    }

    /// One hop along every edge, outbound, unless a later builder says otherwise.
    pub(super) fn walking(self) -> Self {
        self.walk(Along::AnyEdge)
    }

    /// Walks along this edge shape, relation or every edge.
    pub(super) fn walk(mut self, along: Along) -> Self {
        self.walk_mut().along = along;
        self
    }

    /// The end of an edge the walk leaves by.
    pub(super) fn going(mut self, direction: Direction) -> Self {
        self.walk_mut().direction = Some(direction);
        self
    }

    /// How many hops the walk makes.
    pub(super) fn deep(mut self, depth: usize) -> Self {
        self.walk_mut().depth = depth;
        self
    }

    /// The filters the walk applies to what it reaches.
    pub(super) fn keeping(mut self, keeping: Vec<FieldFilter>) -> Self {
        self.walk_mut().keeping = keeping;
        self
    }

    /// The type what the walk reaches has to fit.
    pub(super) fn fitting(mut self, fits_type: Option<types::DeclaredType>) -> Self {
        self.walk_mut().fits_type = fits_type;
        self
    }

    fn walk_mut(&mut self) -> &mut Follow {
        self.follow.get_or_insert_with(Follow::hop)
    }
}
