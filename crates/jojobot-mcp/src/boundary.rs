//! Where an adapter's words stop and a caller's words begin: jojobot's
//! storage is never an agent's business. The adapter's text is logged,
//! never returned; what crosses is written for the caller only.

/// A store failure. `detail` is logged, never returned; the sentence is
/// the same for every failure, because the caller's next move never changes.
pub(crate) fn store_failed(verb: &str, detail: &str) -> String {
    tracing::error!(verb, detail, "store failure reached the boundary");
    format!(
        "{verb} could not be completed — jojobot's own storage failed, which is not something \
         your call did wrong and not something you can fix by calling differently. Nothing was \
         written that you should rely on. Try once more; if it fails again, tell the operator, \
         because it needs a person."
    )
}

/// A record jojobot cannot read, told the same way: logged, not returned.
pub(crate) fn unreadable(what: &str, detail: &str) -> String {
    tracing::error!(what, detail, "unreadable record reached the boundary");
    format!(
        "{what} exists but cannot be read as a record. Something about how it is stored was \
         changed by hand, and only a person can put it back."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What crosses must not contain the adapter's own account.
    #[test]
    fn nothing_crossing_the_boundary_names_the_store() {
        let leaky = "the page for gamma has no table, and the row vanished from the document";
        for said in [
            store_failed("post_message", leaky),
            unreadable("message gamma-4", leaky),
        ] {
            assert!(
                !said.contains(leaky),
                "the adapter's own words crossed: {said}"
            );
        }
    }

    /// **And it still tells the caller what to do**, which is the half a
    /// scrubber loses: stripping the store's words out of the store's sentence
    /// leaves a sentence about nothing.
    #[test]
    fn what_crosses_is_still_an_answer() {
        let said = store_failed("post_message", "the page vanished");
        assert!(said.contains("post_message"), "{said}");
        assert!(
            said.contains("Try once more"),
            "a caller needs its next move: {said}"
        );
        assert!(
            said.contains("tell the operator"),
            "…and the way out when it repeats: {said}"
        );

        let unread = unreadable("message gamma-4", "the state cell is not a state");
        assert!(unread.contains("message gamma-4"), "{unread}");
        assert!(
            unread.contains("only a person"),
            "the caller must learn a retry is pointless: {unread}"
        );
    }

    /// A directory this test owns alone, removed when it is done.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(what: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "jojobot-mcp-boundary-{}-{what}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("a clock after 1970")
                    .as_nanos()
            ));
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Scratch(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// **The two tests above feed the scrubber a string an author invented.**
    /// Nothing proves it generalises to what a real driver actually says — a
    /// driver's errors carry structure (error codes, quoted identifiers, the
    /// product's own vocabulary) an authored string does not.
    ///
    /// This starts one real store, asks it for a table that does not exist,
    /// and captures the genuine [`sqlx::Error`] text that comes back — then
    /// runs THAT through both scrubbers. If the store's own words ever
    /// defeated the scrubbing, this is where a real message would prove it.
    #[tokio::test]
    async fn a_real_store_failures_own_words_are_still_scrubbed() {
        let scratch = Scratch::new("golden");
        let port = jojobot_adapters::testing::free_port();
        let mut store = jojobot_adapters::dolt::Dolt::start(&scratch.0, port)
            .await
            .expect("the real store comes up");

        let failure = sqlx::query("SELECT 1 FROM jojobot_boundary_scrubber_missing_table")
            .execute(store.pool())
            .await
            .expect_err("that table was never created");
        let captured = failure.to_string();
        // Proves the capture is the real thing rather than an empty string:
        // the driver's own error names the table it could not find.
        assert!(
            captured.contains("jojobot_boundary_scrubber_missing_table"),
            "not a real driver message: {captured}"
        );

        let said = store_failed("post_message", &captured);
        assert!(
            !said.contains(&captured),
            "a real driver failure crossed the boundary: {said}"
        );
        assert!(said.contains("post_message"), "{said}");
        assert!(said.contains("Try once more"), "{said}");

        let unread = unreadable("message gamma-4", &captured);
        assert!(
            !unread.contains(&captured),
            "a real driver failure crossed the boundary: {unread}"
        );
        assert!(unread.contains("message gamma-4"), "{unread}");

        store.stop().await;
    }
}
