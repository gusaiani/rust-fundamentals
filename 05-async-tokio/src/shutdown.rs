//! A tiny graceful-shutdown primitive, built on [`tokio::sync::watch`].
//!
//! The pattern (Pill 14): one producer flips a flag; many tasks `.await` on
//! it inside a `select!`. When the flag flips, every `select!` that was
//! racing real work against `shutdown.cancelled()` takes the shutdown arm,
//! finishes its current unit, and returns — so the program drains instead of
//! `abort()`-ing mid-write.
//!
//! `tokio_util::sync::CancellationToken` is the production-grade version of
//! exactly this; building it on `watch` once shows you there's no magic —
//! it's a broadcast bool with a `.changed().await`. (Swapping in the real
//! `CancellationToken` is a stretch goal.)

use tokio::sync::watch;

/// A clonable shutdown handle. Clone it into every task that should stop on
/// signal; call [`Shutdown::trigger`] once (e.g. from a Ctrl-C handler) to
/// fan the signal out to all of them.
#[derive(Clone)]
pub struct Shutdown {
    tx: std::sync::Arc<watch::Sender<bool>>,
    rx: watch::Receiver<bool>,
}

impl Shutdown {
    /// Create a fresh handle in the "running" state.
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(false);
        Shutdown {
            tx: std::sync::Arc::new(tx),
            rx,
        }
    }

    /// Flip the flag to "shutting down". Idempotent — calling it twice is fine.
    pub fn trigger(&self) {
        let _ = self.tx.send(true);
    }

    /// Has shutdown already been requested? Cheap, non-blocking — use it to
    /// bail out of an accept loop's next iteration.
    pub fn is_triggered(&self) -> bool {
        *self.rx.borrow()
    }

    /// Resolve when shutdown is requested. Put this in a `select!` arm against
    /// your real work. Returns immediately if already triggered.
    ///
    /// Implementation: clone the receiver, then loop `changed().await` until
    /// `*borrow()` is true (or return now if it already is). `&mut self` is
    /// avoided so a `&Shutdown` shared across tasks can await it — clone the
    /// receiver locally instead.
    pub async fn cancelled(&self) {
        let mut rx = self.rx.clone();

        if *rx.borrow() {
            return;
        }

        while rx.changed().await.is_ok() {
            if *rx.borrow() {
                return;
            }
        }
    }
}

impl Default for Shutdown {
    fn default() -> Self {
        Self::new()
    }
}
