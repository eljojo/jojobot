//! One uncertainty scope and one reload path for every serving rail.

use std::future::Future;
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::{Writer, sql};

pub(crate) trait StoreError: Sized {
    fn store(why: String) -> Self;
    fn store_native(why: String) -> Self {
        Self::store(why)
    }
    fn indeterminate() -> Self;
    fn unknown(&self) -> bool;
}

struct Scope<'a> {
    writer: &'a Writer,
    rail: &'static str,
    finished: bool,
}

impl Drop for Scope<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.writer
                .0
                .uncertain
                .lock()
                .expect("uncertainty lock")
                .insert(self.rail);
        }
    }
}

/// Fence the rail's old connection and reload its image. On success the rail
/// is certain again; on failure it stays uncertain and reads keep the last
/// published image.
async fn recover<E: StoreError, L, F: Future<Output = Result<L, E>>>(
    writer: &Writer,
    rail: &'static str,
    reload: impl FnOnce() -> F,
) -> Result<(), E> {
    writer.0.attempted.fetch_add(1, Ordering::SeqCst);
    let started = Instant::now();
    let recovered = match writer.0.sql.get() {
        Some(session) => match session.recover(writer).await {
            Ok(read) => {
                let loading = Instant::now();
                let result = sql::reloading(read.reader.clone(), reload()).await;
                let micros = u64::try_from(loading.elapsed().as_micros()).unwrap_or(u64::MAX);
                writer.0.reload_micros.fetch_add(micros, Ordering::SeqCst);
                read.finish().await;
                result.map(|_| ())
            }
            Err(error) => Err(E::store(error.to_string())),
        },
        // With no native transaction, completion of the owned in-process write
        // is its fence: no remote command remains able to commit behind it.
        None => reload().await.map(|_| ()),
    };
    sql::handled();
    match &recovered {
        Ok(()) => {
            writer
                .0
                .uncertain
                .lock()
                .expect("uncertainty lock")
                .remove(rail);
            writer.0.recovered.fetch_add(1, Ordering::SeqCst);
            tracing::info!(
                rail,
                elapsed_us = started.elapsed().as_micros() as u64,
                "the uncertain rail was fenced and reloaded"
            );
        }
        Err(_) => {
            writer.0.failed.fetch_add(1, Ordering::SeqCst);
            tracing::warn!(
                rail,
                "recovery failed; writes refuse and reads keep the last published image"
            );
        }
    }
    recovered
}

/// A write on an uncertain rail first retries the recovery that left it
/// uncertain, so one missed fence cannot refuse writes until restart.
pub(crate) async fn guarded<T, E: StoreError, L, F: Future<Output = Result<L, E>>>(
    writer: &Writer,
    rail: &'static str,
    write: impl Future<Output = Result<T, E>>,
    reload: impl Fn() -> F,
) -> Result<T, E> {
    if writer.rail_uncertain(rail) {
        sql::reset();
        if recover(writer, rail, &reload).await.is_err() {
            return Err(E::store(format!(
                "the {rail} rail is uncertain; this write was not started"
            )));
        }
    }
    sql::reset();
    let mut scope = Scope {
        writer,
        rail,
        finished: false,
    };
    let result = write.await;
    if !matches!(&result, Err(error) if error.unknown()) || sql::phase() == sql::Phase::BeforeCommit
    {
        scope.finished = true;
        return result;
    }
    writer
        .0
        .uncertain
        .lock()
        .expect("uncertainty lock")
        .insert(rail);
    let _ = recover(writer, rail, &reload).await;
    scope.finished = true;
    Err(E::indeterminate())
}
