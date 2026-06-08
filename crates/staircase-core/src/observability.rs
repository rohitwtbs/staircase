//! Observability hooks: `tracing` setup and metric handles.
//!
//! The concrete Prometheus exposition endpoint is a planned blueprint of the
//! gateway / integration layer (filled in gradually). This module provides:
//!
//! - [`init_tracing`] to initialize a `tracing` subscriber.
//! - [`Metrics`], a cheap, lock-free set of counters/gauges that every crate can
//!   increment, plus a serializable [`MetricsSnapshot`] for exposition.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

/// Initialize a global `tracing` subscriber.
///
/// Honors the `RUST_LOG` environment variable; falls back to `default_level`
/// (e.g. `"info"`). Safe to call multiple times — subsequent calls are ignored.
pub fn init_tracing(default_level: &str) {
    use tracing_subscriber::{EnvFilter, fmt};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));
    let _ = fmt().with_env_filter(filter).try_init();
}

/// Lock-free metric counters and gauges shared across the gateway.
///
/// Construct with [`Metrics::new`] (returns an [`Arc`]) and clone the `Arc` into
/// each task. Read a consistent view with [`Metrics::snapshot`].
#[derive(Debug, Default)]
pub struct Metrics {
    poll_count: AtomicU64,
    poll_latency_total_ms: AtomicU64,
    poll_latency_last_ms: AtomicU64,
    protocol_errors: AtomicU64,
    queue_size: AtomicU64,
    reconnect_attempts: AtomicU64,
    throughput: AtomicU64,
}

impl Metrics {
    /// Create a new, shareable metrics registry.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Record the latency (milliseconds) of a completed poll.
    pub fn record_poll_latency(&self, ms: u64) {
        self.poll_count.fetch_add(1, Ordering::Relaxed);
        self.poll_latency_total_ms.fetch_add(ms, Ordering::Relaxed);
        self.poll_latency_last_ms.store(ms, Ordering::Relaxed);
    }

    /// Increment the protocol-error counter.
    pub fn inc_protocol_error(&self) {
        self.protocol_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Set the current queue size gauge.
    pub fn set_queue_size(&self, n: u64) {
        self.queue_size.store(n, Ordering::Relaxed);
    }

    /// Increment the reconnect-attempts counter.
    pub fn inc_reconnect(&self) {
        self.reconnect_attempts.fetch_add(1, Ordering::Relaxed);
    }

    /// Add to the throughput counter (number of points published).
    pub fn add_throughput(&self, n: u64) {
        self.throughput.fetch_add(n, Ordering::Relaxed);
    }

    /// Take a consistent, serializable snapshot of the current metrics.
    pub fn snapshot(&self) -> MetricsSnapshot {
        let poll_count = self.poll_count.load(Ordering::Relaxed);
        let total = self.poll_latency_total_ms.load(Ordering::Relaxed);
        let avg = if poll_count > 0 {
            total as f64 / poll_count as f64
        } else {
            0.0
        };
        MetricsSnapshot {
            poll_count,
            avg_poll_latency_ms: avg,
            last_poll_latency_ms: self.poll_latency_last_ms.load(Ordering::Relaxed),
            protocol_errors: self.protocol_errors.load(Ordering::Relaxed),
            queue_size: self.queue_size.load(Ordering::Relaxed),
            reconnect_attempts: self.reconnect_attempts.load(Ordering::Relaxed),
            throughput: self.throughput.load(Ordering::Relaxed),
        }
    }
}

/// A point-in-time, serializable view of [`Metrics`].
#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    /// Total number of polls recorded.
    pub poll_count: u64,
    /// Average poll latency in milliseconds.
    pub avg_poll_latency_ms: f64,
    /// Latency of the most recent poll in milliseconds.
    pub last_poll_latency_ms: u64,
    /// Total protocol errors.
    pub protocol_errors: u64,
    /// Current queue size.
    pub queue_size: u64,
    /// Total reconnect attempts.
    pub reconnect_attempts: u64,
    /// Total data points published.
    pub throughput: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_track_and_snapshot() {
        let m = Metrics::new();
        m.record_poll_latency(10);
        m.record_poll_latency(20);
        m.inc_protocol_error();
        m.set_queue_size(5);
        m.inc_reconnect();
        m.add_throughput(100);

        let snap = m.snapshot();
        assert_eq!(snap.poll_count, 2);
        assert_eq!(snap.avg_poll_latency_ms, 15.0);
        assert_eq!(snap.last_poll_latency_ms, 20);
        assert_eq!(snap.protocol_errors, 1);
        assert_eq!(snap.queue_size, 5);
        assert_eq!(snap.reconnect_attempts, 1);
        assert_eq!(snap.throughput, 100);
    }
}
