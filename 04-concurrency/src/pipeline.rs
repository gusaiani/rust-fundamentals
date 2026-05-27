//! Reader → bounded channel → worker pool pipeline.
//!
//! Decouples the I/O-ish "produce lines" stage from the CPU-bound "parse +
//! aggregate" stage. The channel is **bounded** on purpose (Pill 10): an
//! unbounded queue on a huge log is an OOM waiting to happen — a bounded one
//! makes the producer block when workers fall behind (free backpressure).

use crossbeam_channel::bounded;

use crate::parser::parse_line;
use crate::stats::{Merge, Stats};

/// Lines per channel message. One line per message makes channel overhead
/// dominate; batch so the synchronization cost amortizes.
const BATCH: usize = 1024;

/// Analyze via a producer thread + `n_workers` consumer threads.
pub fn analyze_pipeline(data: &[u8], n_workers: usize) -> Stats {
    // bounded(64) = at most 64 batches in flight (~64k lines). If workers fall
    // behind, the producer's send() *blocks* — that's Pill 10's backpressure
    let (tx, rx) = bounded::<Vec<String>>(64);
    let text = String::from_utf8_lossy(data);

    std::thread::scope(|s| {
        // PRODUCER: one thread reads lines and ships them in batches.
        // `move` transfers ownership of `tx` into the closure, so when this
        // thread exits, `tx` is dropped — which is the workers' shutdown signal
        s.spawn(move || {
            let mut batch: Vec<String> = Vec::with_capacity(BATCH);
            for line in text.lines() {
                if line.is_empty() {
                    continue;
                }
                batch.push(line.to_owned());
                if batch.len() == BATCH {
                    // swap a fresh empty batch in, send the full one
                    let full = std::mem::replace(&mut batch, Vec::with_capacity(BATCH));
                    tx.send(full).unwrap();
                }
            }
            // flush the trailing partial branch
            if !batch.is_empty() {
                tx.send(batch).unwrap();
            }
            // tx dropped here as the closure ends
        });

        // WORKERS: each clones `rx` (crossbeam channels are MPMC — many
        // workers can pull from the same queue). Each worker owns its own
        // local Stats — no shared state, no Mutex.
        let workers: Vec<_> = (0..n_workers)
            .map(|_| {
                let rx = rx.clone();
                s.spawn(move || {
                    let mut local = Stats::default();
                    // `for batch in rx` ends when the channel is empty AND
                    // every Sender has been dropped. That's why the producer's
                    // `move`-and-exit pattern matters.
                    for batch in rx {
                        for line in batch {
                            match parse_line(&line) {
                                Ok(entry) => local.record(&entry),
                                Err(_) => local.record_malformed(),
                            }
                        }
                    }
                    local
                })
            })
            .collect();

        // REDUCE: same associative fold as parallel.rs
        let mut acc = Stats::default();
        for w in workers {
            acc.merge(w.join().unwrap());
        }
        acc
    })
}
