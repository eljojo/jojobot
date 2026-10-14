//! [`Uncertain`] — the guard that drops an image when a write's outcome is
//! unknown (decision log 292).
//!
//! A write that the store committed and whose future was then dropped never
//! reaches the code that brings the image up to date. The image would go on
//! answering without it, and every read from the image would disagree with the
//! store. The guard is armed across such a write and dropped with the image's
//! contents unless the write disarms it. A dropped image sends every read to the
//! store until the next write fills it again.

use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// **Drops the image unless disarmed**: armed across a write whose outcome the
/// image cannot know if the write does not finish.
pub(super) struct Uncertain<'a, I> {
    image: Option<&'a RwLock<Option<I>>>,
    /// The change signal the image bumps when it changes, when it has one.
    generation: Option<&'a AtomicU64>,
    /// What the image holds, for the log line.
    what: &'static str,
}

impl<'a, I> Uncertain<'a, I> {
    pub(super) fn armed(
        image: &'a RwLock<Option<I>>,
        generation: Option<&'a AtomicU64>,
        what: &'static str,
    ) -> Self {
        Uncertain {
            image: Some(image),
            generation,
            what,
        }
    }

    /// The outcome is known: keep the image.
    pub(super) fn disarm(mut self) {
        self.image = None;
    }
}

impl<I> Drop for Uncertain<'_, I> {
    fn drop(&mut self) {
        if let Some(image) = self.image {
            tracing::warn!(
                what = self.what,
                "a write's outcome is unknown, so reads go to the store until the next write \
                 fills the in-memory image again"
            );
            *image.write().expect("fold lock") = None;
            if let Some(generation) = self.generation {
                generation.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
}
