//! `staircase-storage` — store-and-forward storage layer for Staircase.
//!
//! # Status: blueprint
//!
//! This crate is a **compiling scaffold**, not a finished implementation. It
//! lays out the structure, configuration, and the [`StorageEngine`] surface so
//! the RocksDB-backed store-and-forward buffer can be filled in gradually. Every
//! method that needs the real backing store is marked with `todo!()` and a
//! comment describing the intended behavior.
//!
//! # Goal
//!
//! Provide a durable, ordered local buffer for [`DataPoint`]s so the gateway can:
//! - **operate offline** — writes succeed with no network/connector available,
//! - **survive restarts** — buffered data is persisted to disk and recovered,
//! - **replay exactly once** — buffered records are read in order, forwarded,
//!   then acknowledged (deleted) once a connector confirms delivery.
//!
//! # Intended RocksDB design
//!
//! - **Open** a RocksDB database at [`StorageConfig::path`]. Use a single
//!   column family (default) for the queue plus a small metadata key for the
//!   monotonic sequence counter so ids never repeat across restarts.
//! - **Key layout:** encode each record's [`RecordId`] (a `u64` sequence number)
//!   as **big-endian** bytes. Big-endian keys sort lexicographically in
//!   insertion order, so a forward iterator yields oldest-first — exactly the
//!   replay order we want.
//! - **Value layout:** serialize the [`DataPoint`] (e.g. JSON or a compact
//!   binary codec) as the record value.
//! - **`store`:** allocate the next sequence id per point, write all points in a
//!   single `WriteBatch` for atomic, durable enqueue, then persist the updated
//!   counter. Consider a synchronous write option for stronger durability.
//! - **`load_batch`:** iterate from the start, deserialize up to `max` records,
//!   and return them with their ids (without deleting).
//! - **`ack`:** delete the acknowledged ids in a `WriteBatch` (idempotent —
//!   deleting an absent key is a no-op), giving exactly-once forwarding.
//! - **`len`:** track an in-memory count (loaded on open) or fall back to an
//!   iterator count; update it on `store`/`ack`.
//! - **Retention:** when [`StorageConfig::max_records`] is exceeded, drop the
//!   oldest records (front of the queue) to bound disk usage.
//! - **Observability:** after each `store`/`ack`, report the queue depth via the
//!   core observability hook (see `staircase_core::observability`) so the
//!   gateway can surface buffer backlog.
//!
//! # Tests to add alongside the implementation
//!
//! - enqueue → `load_batch` returns oldest-first in insertion order,
//! - `ack` removes exactly the acknowledged ids and leaves the rest,
//! - offline writes succeed with no connector present,
//! - durability: write, drop the store, reopen the same path, and confirm the
//!   buffered records (and the sequence counter) are recovered.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use staircase_core::error::{Result, StaircaseError};
use staircase_core::model::DataPoint;
use staircase_core::traits::{RecordId, StorageEngine, StoredRecord};

/// Configuration for the store-and-forward buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// On-disk path for the RocksDB database (created if missing).
    pub path: String,
    /// Optional cap on buffered records; oldest are dropped past this limit.
    /// `None` means unbounded (limited only by disk).
    #[serde(default)]
    pub max_records: Option<usize>,
    /// Whether each write should be flushed synchronously for stronger
    /// durability (slower) versus relying on RocksDB's WAL (faster).
    #[serde(default)]
    pub sync_writes: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: "./data/staircase-store".to_string(),
            max_records: None,
            sync_writes: false,
        }
    }
}

/// RocksDB-backed store-and-forward buffer (blueprint).
///
/// Holds the configuration and, once implemented, the open database handle plus
/// the monotonic sequence counter used to mint [`RecordId`]s.
pub struct StoreAndForward {
    config: StorageConfig,
    // TODO: add the backing store handle and sequence counter, e.g.:
    //   db: rocksdb::DB,
    //   next_id: std::sync::atomic::AtomicU64,
    //   len: std::sync::atomic::AtomicUsize,
}

impl StoreAndForward {
    /// Open (or create) the store at [`StorageConfig::path`].
    ///
    /// Implementation outline: open RocksDB at `config.path`, load the persisted
    /// sequence counter (or seed it past the highest existing key), and
    /// initialize the in-memory length from the existing key count.
    pub fn open(config: StorageConfig) -> Result<Self> {
        // Placeholder so the type is constructible while the backend is built.
        let _ = &config;
        Ok(Self { config })
    }

    /// The configured on-disk path.
    pub fn path(&self) -> &str {
        &self.config.path
    }

    /// The configured retention cap, if any.
    pub fn max_records(&self) -> Option<usize> {
        self.config.max_records
    }
}

#[async_trait]
impl StorageEngine for StoreAndForward {
    async fn store(&self, points: &[DataPoint]) -> Result<()> {
        // TODO: assign the next sequence id to each point, write them in a single
        // durable WriteBatch keyed by big-endian id, persist the counter, apply
        // retention, and update the queue-depth metric.
        let _ = points;
        Err(not_implemented("store"))
    }

    async fn load_batch(&self, max: usize) -> Result<Vec<StoredRecord>> {
        // TODO: forward-iterate from the start, deserialize up to `max` records,
        // and return them (oldest first) without deleting.
        let _ = max;
        Err(not_implemented("load_batch"))
    }

    async fn ack(&self, ids: &[RecordId]) -> Result<()> {
        // TODO: delete the given ids in a WriteBatch (idempotent) and update the
        // queue-depth metric.
        let _ = ids;
        Err(not_implemented("ack"))
    }

    async fn len(&self) -> Result<usize> {
        // TODO: return the tracked queue depth.
        Err(not_implemented("len"))
    }
}

/// Uniform "not yet implemented" error for the blueprint surface.
fn not_implemented(op: &str) -> StaircaseError {
    StaircaseError::storage(format!(
        "staircase-storage::{op} is not implemented yet (blueprint)"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_are_sane() {
        let cfg = StorageConfig::default();
        assert!(!cfg.path.is_empty());
        assert_eq!(cfg.max_records, None);
        assert!(!cfg.sync_writes);
    }

    #[test]
    fn store_exposes_config() {
        let cfg = StorageConfig {
            path: "/tmp/staircase-test".into(),
            max_records: Some(1000),
            sync_writes: true,
        };
        let store = StoreAndForward::open(cfg).unwrap();
        assert_eq!(store.path(), "/tmp/staircase-test");
        assert_eq!(store.max_records(), Some(1000));
    }
}
