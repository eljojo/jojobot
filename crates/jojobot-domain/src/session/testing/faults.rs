use super::*;

impl InMemorySessions {
    /// Make a stored run's read fail as damaged data, or repair the fixture.
    /// Typed session structs cannot otherwise express an unreadable stored cell.
    pub fn set_unreadable_run(&self, run: Option<SessionId>) {
        *self.unreadable_run.lock().expect("fault lock") = run;
    }

    pub(super) fn check_readable(&self, asked: Option<&SessionId>) -> Result<(), SessionError> {
        if self
            .unreadable_run
            .lock()
            .expect("fault lock")
            .as_ref()
            .is_some_and(|run| asked.is_none_or(|asked| asked == run))
        {
            return Err(SessionError::Unreadable {
                what: "session.started_at is not a stored timestamp".into(),
            });
        }
        Ok(())
    }

    /// **A `served_reset` change whose value is not a whole number**, kept as a
    /// store holds one an older build or a hand wrote. The kinds of change a
    /// write makes carry numbers, so only a plant can make this one.
    pub fn plant_unreadable_reset(&self, id: &SessionId, value: &str) {
        let at = self.stamp();
        let mut changes = self.changes.lock().expect("change lock");
        let ordinal = changes
            .iter()
            .filter(|(held, _)| held == id)
            .map(|(_, c)| c.ordinal)
            .max()
            .map_or(1, |highest| highest + 1);
        changes.push((
            id.clone(),
            SessionChange {
                ordinal,
                changed: Some(crate::session::history::ChangeKind::ServedReset),
                value: Some(value.to_string()),
                written_at: Some(at),
            },
        ));
    }
}
