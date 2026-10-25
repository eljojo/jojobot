use super::*;

impl Story {
    /// **Read the stores again, as a restart would.** A story that wrote to the
    /// raw stores past the server (`memory_store`, `sessions_store`, `mail_store`) changed
    /// something this server holds in memory and did not see; this is the one
    /// way to say that happened.
    pub async fn the_store_changed_underneath(&self) {
        self.runs_image.rebuild().await.expect("the runs load");
        self.mail_image.rebuild().await.expect("the board loads");
        jojobot::wiring::fill_memory(&self.memory_fold, &self.memory_index).await;
    }

    /// Damage the fixture's run read, invalidating the image as a failed refresh
    /// does, or repair it and restore the image. No boot changes the run's state.
    pub async fn the_run_read_is_unreadable(&self, run: &str, unreadable: bool) {
        self.runs.set_unreadable_run(
            unreadable.then(|| jojobot_domain::session::SessionId(run.to_string())),
        );
        let loaded = self.runs_image.rebuild().await;
        if unreadable {
            assert!(
                matches!(
                    loaded,
                    Err(jojobot_domain::session::SessionError::Unreadable { .. })
                ),
                "the damaged run prevents a readable image: {loaded:?}"
            );
        } else {
            loaded.expect("the repaired runs load");
        }
    }
}
