//! `staircase-connectors` — output connectors for Staircase.
//!
//! # Status: blueprint
//!
//! This crate is a **compiling scaffold**, not a finished implementation. It
//! lays out one module per downstream system, each exposing a real config type
//! and a [`DataPublisher`] implementation whose network I/O is stubbed. The
//! protocol-independent **payload mapping** ([`payload`]) is real and tested,
//! since that is the part every connector shares and the task emphasizes.
//!
//! # Goal
//!
//! Forward normalized [`DataPoint`]s to external systems. Every connector
//! consumes the same [`DataPoint`] — there is no protocol-specific code at the
//! call site. Connectors:
//! - establish/re-establish their connection in [`DataPublisher::connect`],
//! - serialize points to the target wire format and send them in
//!   [`DataPublisher::publish`], returning `Ok(())` only on confirmed delivery
//!   (so a store-and-forward layer can ack the buffered records),
//! - report reconnect attempts / throughput via the core observability hooks
//!   (see [`DeliveryStats`] for the intended counters).
//!
//! # Connectors (blueprint modules)
//!
//! - [`mqtt`] — publish to MQTT topics.
//! - [`kafka`] — produce to Kafka topics.
//! - [`nats`] — publish to NATS subjects.
//! - [`postgres`] — insert rows into PostgreSQL.
//! - [`timescale`] — PostgreSQL-compatible, hypertable-friendly inserts.
//! - [`influx`] — InfluxDB line protocol over HTTP.
//! - [`rest`] — generic HTTP POST of serialized data points.
//!
//! # Delivery semantics
//!
//! `publish` returning `Ok(())` is the uniform delivery confirmation: callers
//! (e.g. the gateway forwarding loop) treat it as "safe to ack the buffer".
//! Errors are returned so the caller can retry/keep buffering. Implementations
//! should be batch-oriented and idempotent where the target allows.

use staircase_core::error::StaircaseError;

pub mod payload;

pub mod influx;
pub mod kafka;
pub mod mqtt;
pub mod nats;
pub mod postgres;
pub mod rest;
pub mod timescale;

/// Counters a connector reports via core observability hooks.
///
/// Blueprint placeholder for the intended throughput/reliability metrics; wire
/// these into `staircase_core::observability` as connectors are implemented.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DeliveryStats {
    /// Data points successfully delivered.
    pub delivered: u64,
    /// Data points that failed to deliver.
    pub failed: u64,
    /// Reconnect attempts made.
    pub reconnects: u64,
}

/// Uniform "not yet implemented" error for the blueprint surface.
pub(crate) fn not_implemented(op: &str) -> StaircaseError {
    StaircaseError::Other(anyhow::anyhow!(
        "staircase-connectors::{op} is not implemented yet (blueprint)"
    ))
}
