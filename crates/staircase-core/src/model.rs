//! The unified, protocol-independent data model.
//!
//! Every protocol driver normalizes its readings into these structures so that
//! application code never needs protocol-specific knowledge.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A UTC timestamp used throughout the data model.
pub type Timestamp = DateTime<Utc>;

/// A flexible, protocol-independent value.
///
/// Drivers map their native representations into one of these variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    /// No value / absent.
    Null,
    /// A boolean.
    Bool(bool),
    /// A signed integer.
    Int(i64),
    /// A floating-point number.
    Float(f64),
    /// A UTF-8 string.
    String(String),
    /// Raw bytes.
    Bytes(Vec<u8>),
}

impl Value {
    /// Interpret this value as an `f64` where reasonable (`Int`, `Float`, `Bool`).
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    /// Interpret this value as an `i64` where reasonable.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Float(f) => Some(*f as i64),
            Value::Bool(b) => Some(if *b { 1 } else { 0 }),
            _ => None,
        }
    }

    /// Interpret this value as a `bool` where reasonable.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Int(i) => Some(*i != 0),
            Value::Float(f) => Some(*f != 0.0),
            _ => None,
        }
    }

    /// Borrow this value as a string slice if it is a [`Value::String`].
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write!(f, "{x}"),
            Value::String(s) => write!(f, "{s}"),
            Value::Bytes(b) => write!(f, "<{} bytes>", b.len()),
        }
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Int(v as i64)
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_string())
    }
}

/// The quality of a sampled value, in the spirit of OPC/BACnet quality flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    /// The value is trustworthy.
    #[default]
    Good,
    /// The value is known to be bad (e.g. read failure).
    Bad,
    /// The value's quality cannot be determined.
    Uncertain,
    /// The value is old / has not been refreshed within its expected interval.
    Stale,
}

/// A single normalized sample collected from a device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataPoint {
    /// Logical origin of the reading (e.g. the gateway or collector name).
    pub source: String,
    /// Protocol identifier (e.g. `"bacnet"`, `"modbus"`, `"mqtt"`).
    pub protocol: String,
    /// The device this reading came from.
    pub device_id: String,
    /// The tag (point) name within the device.
    pub tag_name: String,
    /// The sampled value.
    pub value: Value,
    /// The quality of the sample.
    #[serde(default)]
    pub quality: Quality,
    /// When the sample was taken.
    pub timestamp: Timestamp,
}

impl DataPoint {
    /// Create a new `DataPoint` with [`Quality::Good`] and the current UTC time.
    pub fn new(
        source: impl Into<String>,
        protocol: impl Into<String>,
        device_id: impl Into<String>,
        tag_name: impl Into<String>,
        value: impl Into<Value>,
    ) -> Self {
        Self {
            source: source.into(),
            protocol: protocol.into(),
            device_id: device_id.into(),
            tag_name: tag_name.into(),
            value: value.into(),
            quality: Quality::Good,
            timestamp: Utc::now(),
        }
    }

    /// Override the quality.
    pub fn with_quality(mut self, quality: Quality) -> Self {
        self.quality = quality;
        self
    }

    /// Override the timestamp.
    pub fn with_timestamp(mut self, timestamp: Timestamp) -> Self {
        self.timestamp = timestamp;
        self
    }
}

/// A field device known to the gateway.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Device {
    /// Human-friendly device name.
    pub name: String,
    /// Protocol identifier used to talk to the device.
    pub protocol: String,
    /// Transport address (e.g. `"192.168.1.10:502"`).
    pub address: String,
    /// Polling interval, in seconds.
    pub poll_interval_secs: u64,
    /// The tags (points) to collect from the device.
    #[serde(default)]
    pub tags: Vec<Tag>,
}

/// A single readable/writable point on a device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    /// Logical tag name (normalized, protocol-independent).
    pub name: String,
    /// Protocol-specific address (e.g. Modbus register, BACnet object id).
    pub address: String,
    /// Optional declared data type hint.
    #[serde(default)]
    pub data_type: Option<String>,
}

/// A value associated with a specific tag, with quality and timestamp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagValue {
    /// The tag name.
    pub tag_name: String,
    /// The value.
    pub value: Value,
    /// The quality of the value.
    #[serde(default)]
    pub quality: Quality,
    /// When the value was produced.
    pub timestamp: Timestamp,
}

impl TagValue {
    /// Construct a `TagValue` with good quality and the current time.
    pub fn new(tag_name: impl Into<String>, value: impl Into<Value>) -> Self {
        Self {
            tag_name: tag_name.into(),
            value: value.into(),
            quality: Quality::Good,
            timestamp: Utc::now(),
        }
    }
}

/// Severity level for [`Alarm`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Informational.
    Info,
    /// A warning condition.
    Warning,
    /// A critical condition requiring attention.
    Critical,
}

/// An alarm raised by the rule engine or a driver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alarm {
    /// Unique alarm id.
    pub id: String,
    /// The device the alarm relates to.
    pub device_id: String,
    /// The tag the alarm relates to.
    pub tag_name: String,
    /// Severity of the alarm.
    pub severity: Severity,
    /// Human-readable message.
    pub message: String,
    /// When the alarm was raised.
    pub timestamp: Timestamp,
    /// Whether the alarm is currently active.
    pub active: bool,
}

impl Alarm {
    /// Create a new active alarm with a generated id and the current time.
    pub fn new(
        device_id: impl Into<String>,
        tag_name: impl Into<String>,
        severity: Severity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            device_id: device_id.into(),
            tag_name: tag_name.into(),
            severity,
            message: message.into(),
            timestamp: Utc::now(),
            active: true,
        }
    }
}

/// A discrete event (lifecycle, audit, or rule-emitted).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Unique event id.
    pub id: String,
    /// Event kind/category.
    pub kind: String,
    /// Optional related device.
    #[serde(default)]
    pub device_id: Option<String>,
    /// Human-readable message.
    pub message: String,
    /// When the event occurred.
    pub timestamp: Timestamp,
    /// Arbitrary structured metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl Event {
    /// Create a new event with a generated id and the current time.
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind: kind.into(),
            device_id: None,
            message: message.into(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Attach a related device id.
    pub fn with_device(mut self, device_id: impl Into<String>) -> Self {
        self.device_id = Some(device_id.into());
        self
    }

    /// Insert a metadata key/value pair.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datapoint_json_roundtrip() {
        let dp = DataPoint::new("gw", "modbus", "meter_1", "voltage", 230.5_f64);
        let json = serde_json::to_string(&dp).unwrap();
        let back: DataPoint = serde_json::from_str(&json).unwrap();
        assert_eq!(dp, back);
        assert_eq!(back.value.as_f64(), Some(230.5));
    }

    #[test]
    fn value_conversions() {
        assert_eq!(Value::from(true).as_f64(), Some(1.0));
        assert_eq!(Value::Int(7).as_bool(), Some(true));
        assert_eq!(Value::from("hi").as_str(), Some("hi"));
        assert_eq!(Value::Float(3.9).as_i64(), Some(3));
    }

    #[test]
    fn alarm_and_event_have_unique_ids() {
        let a = Alarm::new("d", "t", Severity::Warning, "hot");
        let b = Alarm::new("d", "t", Severity::Warning, "hot");
        assert_ne!(a.id, b.id);
        assert!(a.active);

        let e = Event::new("startup", "gateway up").with_device("d1");
        assert_eq!(e.device_id.as_deref(), Some("d1"));
    }
}
