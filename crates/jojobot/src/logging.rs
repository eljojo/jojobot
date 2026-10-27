//! **What the process logs, and what it leaves out.** The binary starts its
//! subscriber from here, and the cases that read the log back build the same
//! one, so a filter is proven on the path the real events take.

use tracing_subscriber::{Layer, layer::SubscriberExt};

/// **Whether a log written to `stream` is coloured.** One rule: only when the
/// stream is an interactive terminal. The service under systemd writes to a
/// pipe, and colour there puts escape codes inside a field's text, so a search of
/// the log for `tool="capture"` finds nothing.
pub fn colour_for(stream: &impl std::io::IsTerminal) -> bool {
    stream.is_terminal()
}

/// **The directives a process logs under when the operator sets none.** Info
/// for every crate, and the search index at warn: at info it writes thousands
/// of lines a boot about its own commits, merges and deleted segment files, and
/// the lines that say what jojobot is doing are lost among them.
pub const DEFAULT_DIRECTIVES: &str = "info,tantivy=warn";

/// **The filter the binary logs under, from the value of `RUST_LOG`.** The one
/// place a filter is built. An unset or empty value is no filter and gets
/// [`DEFAULT_DIRECTIVES`] (an empty string parses to no directives and would
/// drop every event, the serving line included; a build sandbox exports exactly
/// that). A value the operator sets wins whole, and one that does not parse
/// falls back to the default.
pub fn filter_for(rust_log: Option<&str>) -> tracing_subscriber::EnvFilter {
    match rust_log.filter(|raw| !raw.trim().is_empty()) {
        Some(raw) => tracing_subscriber::EnvFilter::try_new(raw)
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(DEFAULT_DIRECTIVES)),
        None => tracing_subscriber::EnvFilter::new(DEFAULT_DIRECTIVES),
    }
}

/// **The subscriber the binary logs through**, over any writer. Start-up and the
/// case that reads the log back build it the same way. `ansi` is the answer of
/// [`colour_for`] for the stream the writer ends in.
pub fn logging<W>(
    writer: W,
    ansi: bool,
    level: tracing_subscriber::EnvFilter,
) -> impl tracing::Subscriber + Send + Sync
where
    W: for<'a> tracing_subscriber::fmt::MakeWriter<'a> + Send + Sync + 'static,
{
    tracing_subscriber::registry().with(level).with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer)
            .with_ansi(ansi)
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

/// **The transport's ERROR for a response it could not send to a connection
/// that closed**: "failed to send pending response during drain" while its
/// service closes, and "fail to response message" when a listen ends as its
/// connection goes (see [`UNSENT_ANSWER_LINES`]). Each fires for any connection
/// that closes with a response still owed, a held-open listen included, so it
/// carries no fault of ours. It is matched by the place it is
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
        && metadata
            .line()
            .is_some_and(|line| UNSENT_ANSWER_LINES.contains(&line))
}

/// **The two places `rmcp` 3.1.4 logs an answer the connection could not take.**
/// Line 1748 is the drain after the connection closed ("failed to send pending
/// response during drain"). Line 1488 is the send of an answer, "fail to
/// response message", which a held-open listen reaches when its connection is
/// gone as the listen ends. Neither names a request, so each is matched by its
/// place and a version that moves a line stops matching.
const UNSENT_ANSWER_LINES: [u32; 2] = [1488, 1748];

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// The metadata of an event as `rmcp` 3.1.4 would log it from `line` of its
    /// `service.rs`. A filter sees only metadata, so the case builds exactly that.
    macro_rules! rmcp_event {
        ($line:expr, $level:expr, $file:expr) => {{
            struct Site;
            static SITE: Site = Site;
            impl tracing::Callsite for Site {
                fn set_interest(&self, _: tracing::subscriber::Interest) {}
                fn metadata(&self) -> &tracing::Metadata<'_> {
                    &META
                }
            }
            static META: tracing::Metadata<'static> = tracing::Metadata::new(
                "event",
                "rmcp::service",
                $level,
                Some($file),
                Some($line),
                Some("rmcp::service"),
                tracing::field::FieldSet::new(&[], tracing::callsite::Identifier(&SITE)),
                tracing::metadata::Kind::EVENT,
            );
            &META
        }};
    }

    /// **Both places the transport logs an answer it could not send to a closed
    /// connection are left out, and nothing near them is.** The drain's line and
    /// the send's line match; the line after each, the same line in another
    /// version's file and a WARN at the matching line do not, so a moved line or
    /// a wider match reddens a case here.
    #[test]
    fn the_transports_two_unsent_answer_lines_are_matched_by_their_place_only() {
        const FILE: &str = "/registry/rmcp-3.1.4/src/service.rs";
        for (what, event) in [
            ("1488", rmcp_event!(1488, tracing::Level::ERROR, FILE)),
            ("1748", rmcp_event!(1748, tracing::Level::ERROR, FILE)),
        ] {
            assert!(is_the_unsent_answer_at_drain(event), "line {what}");
        }
        for (what, event) in [
            ("1489", rmcp_event!(1489, tracing::Level::ERROR, FILE)),
            ("1747", rmcp_event!(1747, tracing::Level::ERROR, FILE)),
            (
                "another version",
                rmcp_event!(
                    1488,
                    tracing::Level::ERROR,
                    "/registry/rmcp-3.2.0/src/service.rs"
                ),
            ),
            ("a WARN", rmcp_event!(1488, tracing::Level::WARN, FILE)),
        ] {
            assert!(!is_the_unsent_answer_at_drain(event), "{what}");
        }
    }

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
        let subscriber = logging(
            kept.clone(),
            false,
            tracing_subscriber::EnvFilter::new("info"),
        );
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

    /// **A log written to something that is not a terminal carries no escape
    /// byte, and the text a search looks for is whole.** The default formatter
    /// colours a field's name, which puts escape codes inside `tool="capture"`
    /// and hides it from a search of the log.
    #[test]
    fn a_log_that_is_not_a_terminal_holds_plain_text() {
        let kept = Kept::default();
        let subscriber = logging(
            kept.clone(),
            false,
            tracing_subscriber::EnvFilter::new("info"),
        );
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(tool = "capture", outcome = "answered", "mcp request ended");
        });
        let bytes = kept.0.lock().expect("log buffer").clone();
        assert!(
            !bytes.contains(&0x1b),
            "an escape byte was written: {:?}",
            String::from_utf8_lossy(&bytes)
        );
        let text = String::from_utf8(bytes).expect("utf8");
        assert!(
            text.contains("tool=\"capture\""),
            "the field is whole in the text: {text}"
        );
    }

    /// **Colour is a choice made once, from the stream.** A file is not a
    /// terminal, so the rule says no; and the flag it feeds is what turns colour
    /// on, so a build that ignored it would pass the case above for the wrong
    /// reason.
    #[test]
    fn colour_is_on_only_when_the_stream_is_a_terminal() {
        let not_a_terminal = std::fs::File::open("/dev/null").expect("a device that exists");
        assert!(!colour_for(&not_a_terminal));

        let kept = Kept::default();
        let subscriber = logging(
            kept.clone(),
            true,
            tracing_subscriber::EnvFilter::new("info"),
        );
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(tool = "capture", "mcp request ended");
        });
        let bytes = kept.0.lock().expect("log buffer").clone();
        assert!(bytes.contains(&0x1b), "colour on writes escape codes");
    }

    /// **With `RUST_LOG` unset the search index's info lines are filtered and
    /// jojobot's own are kept.** Events go in under tantivy's own target and
    /// through the `log` bridge tantivy really logs through, so the case travels
    /// both paths. A tantivy warning still comes through.
    #[test]
    fn the_default_filter_drops_tantivy_info_and_keeps_ours() {
        let _ = tracing_log::LogTracer::init();
        let kept = Kept::default();
        let subscriber = logging(kept.clone(), false, filter_for(None));
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "tantivy::indexer::index_writer", "tantivy housekeeping");
            tracing_log::log::info!(target: "tantivy::directory::managed_directory", "tantivy deleted a segment file");
            tracing::warn!(target: "tantivy::indexer::segment_updater", "tantivy warning");
            tracing::info!(target: "jojobot::wiring", "jojobot says what it does");
        });
        let text = String::from_utf8(kept.0.lock().expect("log buffer").clone()).expect("utf8");
        assert!(!text.contains("tantivy housekeeping"), "{text}");
        assert!(!text.contains("deleted a segment file"), "{text}");
        for line in ["tantivy warning", "jojobot says what it does"] {
            assert!(text.contains(line), "{line:?} was dropped: {text}");
        }
    }

    /// **An empty `RUST_LOG` is the default, and one the operator sets wins.**
    /// A value that names tantivy at info brings its info lines back, and a value
    /// that does not parse (a level that is no level) falls back to the default.
    #[test]
    fn a_rust_log_the_operator_sets_wins_and_an_empty_one_is_the_default() {
        for (rust_log, tantivy_info_kept) in [
            (Some("  "), false),
            (Some(""), false),
            (Some("info,tantivy=info"), true),
            (Some("info,tantivy=loud"), false),
        ] {
            let kept = Kept::default();
            let subscriber = logging(kept.clone(), false, filter_for(rust_log));
            tracing::subscriber::with_default(subscriber, || {
                tracing::info!(target: "tantivy::indexer::index_writer", "tantivy housekeeping");
                tracing::info!(target: "jojobot::wiring", "jojobot says what it does");
            });
            let text = String::from_utf8(kept.0.lock().expect("log buffer").clone()).expect("utf8");
            assert_eq!(
                text.contains("tantivy housekeeping"),
                tantivy_info_kept,
                "{rust_log:?}: {text}"
            );
            assert!(
                text.contains("jojobot says what it does"),
                "{rust_log:?}: {text}"
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
        let subscriber = logging(
            kept.clone(),
            false,
            tracing_subscriber::EnvFilter::new("info"),
        );
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
