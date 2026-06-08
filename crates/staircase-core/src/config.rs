//! Configuration model and YAML loader.
//!
//! The configuration mirrors the example schema:
//!
//! ```yaml
//! devices:
//!   - name: ahu_1
//!     protocol: bacnet
//!     address: 192.168.1.10
//!     poll_interval: 5
//!   - name: meter_1
//!     protocol: modbus
//!     address: 192.168.1.20
//!     poll_interval: 10
//! rules:
//!   - condition: "room_temp > 28"
//!     action: "fan = true"
//! connectors:
//!   - name: cloud
//!     type: mqtt
//!     settings:
//!       url: "tcp://broker:1883"
//! ```
//!
//! Hot-reload wiring lives in the gateway/integration layer; this module only
//! provides the types and a parser.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Result, StaircaseError};

/// Default polling interval (seconds) when a device omits `poll_interval`.
pub const DEFAULT_POLL_INTERVAL: u64 = 5;

fn default_poll_interval() -> u64 {
    DEFAULT_POLL_INTERVAL
}

/// Top-level gateway configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// The devices to poll.
    #[serde(default)]
    pub devices: Vec<DeviceConfig>,
    /// Edge rules to evaluate.
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
    /// Output connectors to publish to.
    #[serde(default)]
    pub connectors: Vec<ConnectorConfig>,
    /// Optional storage configuration.
    #[serde(default)]
    pub storage: Option<StorageConfig>,
    /// Optional observability configuration.
    #[serde(default)]
    pub observability: Option<ObservabilityConfig>,
}

/// Configuration for a single device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    /// Device name.
    pub name: String,
    /// Protocol identifier (e.g. `bacnet`, `modbus`, `mqtt`).
    pub protocol: String,
    /// Transport address.
    pub address: String,
    /// Polling interval in seconds.
    #[serde(default = "default_poll_interval")]
    pub poll_interval: u64,
    /// Tags to read from the device.
    #[serde(default)]
    pub tags: Vec<TagConfig>,
    /// Arbitrary protocol-specific settings.
    #[serde(default)]
    pub settings: HashMap<String, serde_json::Value>,
}

/// Configuration for a single tag on a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagConfig {
    /// Logical tag name.
    pub name: String,
    /// Protocol-specific address.
    pub address: String,
    /// Optional data type hint.
    #[serde(default)]
    pub data_type: Option<String>,
}

/// Configuration for a single rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleConfig {
    /// Optional human-friendly rule name.
    #[serde(default)]
    pub name: Option<String>,
    /// Condition expression, e.g. `"room_temp > 28"`.
    pub condition: String,
    /// Action expression, e.g. `"fan = true"` or `"alarm"`.
    pub action: String,
}

/// Configuration for a single output connector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorConfig {
    /// Connector name.
    pub name: String,
    /// Connector type (e.g. `mqtt`, `kafka`, `postgres`, `rest`).
    #[serde(rename = "type")]
    pub connector_type: String,
    /// Arbitrary connector-specific settings.
    #[serde(default)]
    pub settings: HashMap<String, serde_json::Value>,
}

/// Configuration for the local store-and-forward storage layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// On-disk path for the local buffer.
    pub path: String,
    /// Optional maximum number of buffered records.
    #[serde(default)]
    pub max_buffer: Option<usize>,
}

/// Configuration for observability (logging + metrics).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Address to expose Prometheus metrics on (e.g. `0.0.0.0:9100`).
    #[serde(default)]
    pub metrics_address: Option<String>,
    /// Default log level (e.g. `info`, `debug`).
    #[serde(default)]
    pub log_level: Option<String>,
}

/// Parse a [`Config`] from a YAML string.
pub fn load_config_str(yaml: &str) -> Result<Config> {
    serde_yaml::from_str(yaml).map_err(|e| StaircaseError::config(format!("invalid config: {e}")))
}

/// Load a [`Config`] from a YAML file on disk.
pub fn load_config_file<P: AsRef<Path>>(path: P) -> Result<Config> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)
        .map_err(|e| StaircaseError::config(format!("failed to read {}: {e}", path.display())))?;
    load_config_str(&content)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
devices:
  - name: ahu_1
    protocol: bacnet
    address: 192.168.1.10
    poll_interval: 5
  - name: meter_1
    protocol: modbus
    address: 192.168.1.20
    poll_interval: 10
    tags:
      - name: voltage
        address: "40001"
rules:
  - condition: "room_temp > 28"
    action: "fan = true"
connectors:
  - name: cloud
    type: mqtt
    settings:
      url: "tcp://broker:1883"
storage:
  path: /var/lib/staircase
"#;

    #[test]
    fn parses_sample_config() {
        let cfg = load_config_str(SAMPLE).unwrap();
        assert_eq!(cfg.devices.len(), 2);
        assert_eq!(cfg.devices[0].name, "ahu_1");
        assert_eq!(cfg.devices[1].poll_interval, 10);
        assert_eq!(cfg.devices[1].tags.len(), 1);
        assert_eq!(cfg.rules.len(), 1);
        assert_eq!(cfg.connectors[0].connector_type, "mqtt");
        assert_eq!(cfg.storage.as_ref().unwrap().path, "/var/lib/staircase");
    }

    #[test]
    fn poll_interval_defaults_when_missing() {
        let yaml = r#"
devices:
  - name: d1
    protocol: mqtt
    address: localhost
"#;
        let cfg = load_config_str(yaml).unwrap();
        assert_eq!(cfg.devices[0].poll_interval, DEFAULT_POLL_INTERVAL);
    }

    #[test]
    fn invalid_config_errors() {
        let err = load_config_str("devices: : :").unwrap_err();
        assert!(matches!(err, StaircaseError::Config(_)));
    }
}
