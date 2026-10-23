use super::*;

impl Jojobot {
    /// Whether this call is the first time `domain` has reached this
    /// session's handle.
    ///
    /// `false` for an anonymous caller — there is no handle to remember it
    /// against, so there is nobody to teach. A store failure is silent and
    /// answers `false` too, for the reason [`Jojobot::beat`] is silent on
    /// one: a teaching that could not be recorded must not fail the verb it
    /// rides on, and answering `true` on a write that did not land would
    /// teach nothing while believing it had.
    pub(crate) async fn first_contact(&self, domain: &str, caller: Option<&Caller>) -> bool {
        // The small form cannot deliver teaching, so it must not spend it.
        if crate::write_receipts::saving() {
            return false;
        }
        let Some(caller) = caller else {
            return false;
        };
        // **A batch holds its spends back until it delivers**: the ledger row is
        // not written here, and a domain the batch already found untaught is told
        // once. See [`BATCH_SPENDS`].
        if let Ok(held) = BATCH_SPENDS.try_with(std::sync::Arc::clone) {
            if !matches!(
                self.teachings.has_contact(&caller.sid, domain).await,
                Ok(false)
            ) {
                return false;
            }
            let mut found = held.lock().expect("a batch's spends are poisoned");
            if found.iter().any(|spent| spent == domain) {
                return false;
            }
            found.push(domain.to_string());
            return true;
        }
        match self
            .teachings
            .first_contact(&caller.sid, domain, self.clock().now())
            .await
        {
            Ok(first) => first,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    domain,
                    "a teaching could not be recorded — the verb it rides on still succeeded"
                );
                false
            }
        }
    }
}
