//! The one log sink for this crate's test binary.
//!
//! "It gets logged" is not a claim you can make by reading the call site, and
//! more than one module here reports something whose only surface IS a log
//! line. They share this sink, because they cannot each install their own: a
//! process gets exactly one global subscriber.

use std::sync::Arc;

/// A sink that keeps whatever was logged, so a test can assert on it, and
/// echoes it where the running test's own output goes.
///
/// **The echo is what puts a log line in front of a reader of a FAILED test.**
/// The test harness shows what a test printed only when it fails, so a line
/// logged during a case that passes costs nothing and one logged during a case
/// that fails is in the failure report. Without it the sink is a buffer nobody
/// reads unless the test asserts on it, which is how a refusal seen once on a
/// builder came with no cause.
#[derive(Clone)]
pub(crate) struct Captured {
    kept: Arc<std::sync::Mutex<Vec<u8>>>,
    echo: fn(&str),
}

impl Captured {
    /// A sink that echoes through `echo`, which a test of the sink replaces.
    fn echoing_to(echo: fn(&str)) -> Self {
        Captured {
            kept: Arc::default(),
            echo,
        }
    }

    /// Everything logged so far.
    pub(crate) fn text(&self) -> String {
        String::from_utf8_lossy(&self.kept.lock().expect("log buffer poisoned")).into_owned()
    }
}

/// The running test's own output: the harness holds `eprint!` per test and
/// shows it only for a test that fails.
fn to_the_test_output(text: &str) {
    eprint!("{text}");
}

impl Default for Captured {
    fn default() -> Self {
        Captured::echoing_to(to_the_test_output)
    }
}

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.kept
            .lock()
            .expect("log buffer poisoned")
            .extend_from_slice(buf);
        (self.echo)(&String::from_utf8_lossy(buf));
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Captured;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// The sink, installed once.
///
/// **Global on purpose.** `tracing` keeps a process-wide max-level hint, so a
/// thread-local subscriber is not enough: a sibling test running with none can
/// leave that hint below WARN, and the event never fires at all — the assertion
/// then reads an empty buffer and blames the code. Passing alone and failing in
/// the suite is the tell. One global sink, installed once, is deterministic.
pub(crate) fn log_sink() -> &'static Captured {
    static SINK: std::sync::OnceLock<Captured> = std::sync::OnceLock::new();
    SINK.get_or_init(|| {
        let captured = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(captured.clone())
            .with_ansi(false)
            .with_max_level(tracing::Level::WARN)
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("only this sink installs a global subscriber");
        // **Records from the `log` crate reach the sink too**, as they do in a
        // served binary: tantivy logs through `log`, and without this bridge a
        // test would see none of what the index warns about.
        tracing_log::LogTracer::init().expect("only this sink installs the log bridge");
        captured
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    thread_local! {
        static ECHOED: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    }

    fn remember(text: &str) {
        ECHOED.with(|echoed| echoed.borrow_mut().push_str(text));
    }

    /// **What is logged is kept for assertions and echoed for a reader.**
    /// Either alone passes on a sink that does the other, so both are read
    /// off the one write.
    #[test]
    fn a_logged_line_is_kept_and_echoed() {
        let mut sink = Captured::echoing_to(remember);
        sink.write_all(b"a migration failed")
            .expect("the sink takes it");
        assert_eq!(sink.text(), "a migration failed");
        ECHOED.with(|echoed| assert_eq!(*echoed.borrow(), "a migration failed"));
    }
}
