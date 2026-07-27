//! The one log sink for this crate's test binary.
//!
//! "It gets logged" is not a claim you can make by reading the call site, and
//! more than one module here reports something whose only surface IS a log
//! line. They share this sink, because they cannot each install their own: a
//! process gets exactly one global subscriber.

use std::sync::Arc;

/// A sink that keeps whatever was logged, so a test can assert on it.
#[derive(Clone, Default)]
pub(crate) struct Captured(Arc<std::sync::Mutex<Vec<u8>>>);

impl Captured {
    /// Everything logged so far.
    pub(crate) fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("log buffer poisoned")).into_owned()
    }

    /// The one logged line containing `needle` — so an assertion is about one
    /// event's own fields, not about a buffer every test in the binary writes
    /// to. A card id is a small integer and turns up in plenty of other lines.
    pub(crate) fn line_with(&self, needle: &str) -> Option<String> {
        self.text()
            .lines()
            .find(|line| line.contains(needle))
            .map(str::to_string)
    }
}

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("log buffer poisoned")
            .extend_from_slice(buf);
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
        captured
    })
}
