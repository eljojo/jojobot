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

    /// **A message in a box the store cannot read**, planted past the served
    /// surface and then seen by the server, as a restart would see it: the card
    /// is quarantined, so no verb lists it and a count reports it.
    pub async fn mail_is_unreadable_in(&self, mailbox: &str, id: &str) {
        self.mail.quarantine_by_damage(
            &jojobot_domain::mailbox::MailboxName(mailbox.to_string()),
            &jojobot_domain::mailbox::MessageId(id.to_string()),
            "its row on the page cannot be read",
        );
        self.the_store_changed_underneath().await;
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
