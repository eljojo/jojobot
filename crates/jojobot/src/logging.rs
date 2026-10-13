//! **What the process logs, and what it leaves out.** The binary starts its
//! subscriber from here, and the cases that read the log back build the same
//! one, so a filter is proven on the path the real events take.

use tracing_subscriber::{Layer, layer::SubscriberExt};

/// **The subscriber the binary logs through**, over any writer. Start-up and the
/// case that reads the log back build it the same way.
pub fn logging<W>(
    writer: W,
    level: tracing_subscriber::EnvFilter,
) -> impl tracing::Subscriber + Send + Sync
where
    W: for<'a> tracing_subscriber::fmt::MakeWriter<'a> + Send + Sync + 'static,
{
    tracing_subscriber::registry().with(level).with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer)
            .with_filter(without_the_lines_the_log_says_elsewhere()),
    )
}

/// **Two lines left out of the log, because each says nothing the log does not
/// already say.** Tantivy warns when a merge ends over segments a commit has
/// already removed, and that is benign — see
/// [`jojobot_adapters::search::is_the_benign_merge_race`] — but it came to the log
/// hundreds of times a day, which is how a warning starts to be ignored. The
/// transport reports an answer it could not send to a closed connection — see
/// [`is_the_unsent_answer_at_drain`] — and the request log already names the
/// request that closed.
fn without_the_lines_the_log_says_elsewhere<S>() -> impl tracing_subscriber::layer::Filter<S> {
    tracing_subscriber::filter::filter_fn(|metadata| {
        !jojobot_adapters::search::is_the_benign_merge_race(metadata)
            && !is_the_unsent_answer_at_drain(metadata)
    })
}

/// **The transport's ERROR for a response it could not send while its service
/// closed**, "failed to send pending response during drain". It fires for any
/// connection that closes with a response still owed, a held-open listen
/// included, so it carries no fault of ours. It is matched by the place it is
/// logged from, because a filter sees no message text and the target alone would
/// also hide the transport's other drain errors, which are real.
///
/// **The line number is the locked version's** (`rmcp` 3.1.4, `service.rs`). A
/// version that moves the line stops matching, and the ERROR returns to the log
/// where the case on a closed connection reads it, rather than a real error being
/// hidden by a stale match.
pub fn is_the_unsent_answer_at_drain(metadata: &tracing::Metadata<'_>) -> bool {
    metadata.target() == "rmcp::service"
        && *metadata.level() == tracing::Level::ERROR
        && metadata
            .file()
            .is_some_and(|f| f.ends_with("rmcp-3.1.4/src/service.rs"))
        && metadata.line() == Some(1748)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// A writer the test reads back.
    #[derive(Clone, Default)]
    struct Kept(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Kept {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("log buffer").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Kept {
        type Writer = Kept;
        fn make_writer(&'a self) -> Kept {
            self.clone()
        }
    }

    /// **The benign merge warning is left out of the log and every other
    /// warning is kept.** The subscriber is the one the binary starts with. Each event is sent under tantivy's own target, so the
    /// case travels the path the real events take: another tantivy module's
    /// warning, and an error from the segment manager itself, still come
    /// through.
    #[test]
    fn the_benign_merge_warning_is_filtered_and_other_tantivy_events_are_not() {
        let kept = Kept::default();
        let subscriber = logging(kept.clone(), tracing_subscriber::EnvFilter::new("info"));
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(target: "tantivy::indexer::segment_manager", "couldn't find segment in SegmentManager");
            tracing::warn!(target: "tantivy::indexer::segment_updater", "another tantivy warning");
            tracing::error!(target: "tantivy::indexer::segment_manager", "a segment manager error");
            tracing::warn!(target: "jojobot_adapters::search", "a warning of ours");
        });
        let text = String::from_utf8(kept.0.lock().expect("log buffer").clone()).expect("utf8");
        assert!(
            !text.contains("couldn't find segment"),
            "the benign warning was logged: {text}"
        );
        for kept_line in [
            "another tantivy warning",
            "a segment manager error",
            "a warning of ours",
        ] {
            assert!(
                text.contains(kept_line),
                "{kept_line:?} was dropped: {text}"
            );
        }
    }

    /// **The same holds for the path tantivy's warnings really take.** Tantivy
    /// logs through the `log` crate, and the binary's subscriber bridges it into
    /// tracing; a bridged event's own target is "log", with tantivy's module in
    /// a field. So the events here are sent through `log` over the bridge, and
    /// another tantivy module's warning still has to come through.
    #[test]
    fn the_benign_merge_warning_is_filtered_when_tantivy_logs_it_through_the_log_crate() {
        // The bridge `init()` installs in the binary. Once per process, so a
        // second test that installed it first is not a failure.
        let _ = tracing_log::LogTracer::init();
        let kept = Kept::default();
        let subscriber = logging(kept.clone(), tracing_subscriber::EnvFilter::new("info"));
        tracing::subscriber::with_default(subscriber, || {
            tracing_log::log::warn!(target: "tantivy::indexer::segment_manager", "couldn't find segment in SegmentManager");
            tracing_log::log::warn!(target: "tantivy::indexer::segment_updater", "another tantivy warning");
        });
        let text = String::from_utf8(kept.0.lock().expect("log buffer").clone()).expect("utf8");
        assert!(
            text.contains("another tantivy warning"),
            "the bridge carried nothing, so this case proves nothing: {text}"
        );
        assert!(
            !text.contains("couldn't find segment"),
            "the benign warning was logged: {text}"
        );
    }
}
