//! Protocol-independent traits that every component conforms to.
//!
//! These traits are the seams of the framework. New protocols, storage backends,
//! and output connectors are added by implementing these traits in their own
//! crates — existing code never has to change (open/closed principle).

use async_trait::async_trait;

use crate::error::{Result, StaircaseError};
use crate::model::{Alarm, DataPoint, Event, TagValue, Value};

/// A driver for a single field protocol (BACnet, Modbus, MQTT, ...).
///
/// Implementors normalize protocol-specific reads into [`DataPoint`]s. Drivers
/// are used as trait objects (`Box<dyn ProtocolDriver>`), so this trait uses
/// [`async_trait`].
#[async_trait]
pub trait ProtocolDriver: Send + Sync {
    /// The protocol identifier this driver speaks (e.g. `"modbus"`).
    fn protocol(&self) -> &str;

    /// Establish the connection to the device/broker.
    async fn connect(&mut self) -> Result<()>;

    /// Tear down the connection. The default implementation is a no-op.
    async fn disconnect(&mut self) -> Result<()> {
        Ok(())
    }

    /// Poll the device once, returning all freshly collected points.
    async fn poll(&mut self) -> Result<Vec<DataPoint>>;

    /// Write a value back to a tag. Defaults to "not supported".
    async fn write_tag(&mut self, _tag_name: &str, _value: TagValue) -> Result<()> {
        Err(StaircaseError::protocol("write_tag not supported by this driver"))
    }
}

/// A higher-level abstraction that yields normalized data points.
///
/// A collector typically wraps one or more [`ProtocolDriver`]s.
#[async_trait]
pub trait DataCollector: Send + Sync {
    /// Collect the next batch of data points.
    async fn collect(&mut self) -> Result<Vec<DataPoint>>;
}

/// A sink that publishes normalized data points to an external system.
///
/// Output connectors (MQTT, Kafka, PostgreSQL, ...) implement this trait.
#[async_trait]
pub trait DataPublisher: Send + Sync {
    /// Establish the connection to the downstream system. Defaults to no-op.
    async fn connect(&mut self) -> Result<()> {
        Ok(())
    }

    /// Publish a batch of normalized data points.
    async fn publish(&mut self, points: &[DataPoint]) -> Result<()>;

    /// Flush and close the publisher. Defaults to no-op.
    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

/// The outcome of evaluating a rule against a data point.
#[derive(Debug, Clone, PartialEq)]
pub enum RuleOutcome {
    /// Assign a value to a (device, tag) pair.
    SetTag {
        /// Target device id.
        device_id: String,
        /// Target tag name.
        tag_name: String,
        /// Value to set.
        value: Value,
    },
    /// Raise an alarm.
    RaiseAlarm(Alarm),
    /// Emit an event.
    EmitEvent(Event),
}

/// A local edge rule engine.
///
/// Implementations evaluate incoming [`DataPoint`]s against rules loaded from
/// configuration and return zero or more [`RuleOutcome`]s. Evaluation must be
/// fully local (no cloud dependency).
pub trait RuleEngine: Send + Sync {
    /// Evaluate a single data point and return any resulting outcomes.
    fn evaluate(&mut self, point: &DataPoint) -> Result<Vec<RuleOutcome>>;
}

/// Identifier for a stored record in a [`StorageEngine`].
pub type RecordId = u64;

/// A persisted data point with its storage identifier.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredRecord {
    /// Storage-assigned identifier (used to acknowledge after forwarding).
    pub id: RecordId,
    /// The persisted data point.
    pub point: DataPoint,
}

/// A durable store-and-forward buffer for data points.
///
/// Implementations (e.g. RocksDB-backed) buffer data locally so the gateway can
/// operate offline and replay once connectivity returns.
#[async_trait]
pub trait StorageEngine: Send + Sync {
    /// Durably append a batch of data points.
    async fn store(&self, points: &[DataPoint]) -> Result<()>;

    /// Load up to `max` of the oldest buffered records (without removing them).
    async fn load_batch(&self, max: usize) -> Result<Vec<StoredRecord>>;

    /// Acknowledge (and remove) records that have been successfully forwarded.
    async fn ack(&self, ids: &[RecordId]) -> Result<()>;

    /// The number of buffered records.
    async fn len(&self) -> Result<usize>;

    /// Whether the buffer is empty.
    async fn is_empty(&self) -> Result<bool> {
        Ok(self.len().await? == 0)
    }
}
