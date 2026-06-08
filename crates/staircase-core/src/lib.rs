//! # staircase-core
//!
//! Protocol-independent foundation for the Staircase building-automation /
//! industrial-IoT edge gateway framework.
//!
//! This crate intentionally does **not** depend on any protocol implementation
//! crate. It defines:
//!
//! - A [unified data model](crate::model) ([`DataPoint`](crate::model::DataPoint),
//!   [`Device`](crate::model::Device), [`Tag`](crate::model::Tag),
//!   [`TagValue`](crate::model::TagValue), [`Alarm`](crate::model::Alarm),
//!   [`Event`](crate::model::Event)).
//! - Protocol-independent [traits](crate::traits)
//!   ([`ProtocolDriver`](crate::traits::ProtocolDriver),
//!   [`DataCollector`](crate::traits::DataCollector),
//!   [`DataPublisher`](crate::traits::DataPublisher),
//!   [`RuleEngine`](crate::traits::RuleEngine),
//!   [`StorageEngine`](crate::traits::StorageEngine)).
//! - Structured [errors](crate::error).
//! - A serde-based [configuration model](crate::config) with a YAML loader.
//! - Async [runtime](crate::runtime) scaffolding for per-device tasks, graceful
//!   shutdown and task supervision.
//! - [Observability](crate::observability) hooks (`tracing` + metric handles).
//!
//! Applications built on Staircase work exclusively with these normalized types
//! and never need protocol-specific code.

pub mod config;
pub mod error;
pub mod model;
pub mod observability;
pub mod runtime;
pub mod testing;
pub mod traits;

pub use error::{Result, StaircaseError};
pub use model::{
    Alarm, DataPoint, Device, Event, Quality, Severity, Tag, TagValue, Timestamp, Value,
};
pub use traits::{
    DataCollector, DataPublisher, ProtocolDriver, RecordId, RuleEngine, RuleOutcome, StorageEngine,
    StoredRecord,
};

/// The version of this crate, taken from the Cargo manifest.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
