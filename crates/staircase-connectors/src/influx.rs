//! InfluxDB output connector (blueprint).

use async_trait::async_trait;
use serde::Deserialize;
use staircase_core::error::Result;
use staircase_core::model::DataPoint;
use staircase_core::traits::DataPublisher;

use crate::{not_implemented, payload};

/// Configuration for the InfluxDB connector.
#[derive(Debug, Clone, Deserialize)]
pub struct InfluxConnectorConfig {
    /// Base URL (e.g. `http://host:8086`).
    pub url: String,
    /// Target database/bucket.
    pub bucket: String,
    /// Optional org (InfluxDB 2.x).
    #[serde(default)]
    pub org: String,
    /// Optional auth token.
    #[serde(default)]
    pub token: String,
}

/// Writes data points to InfluxDB using line protocol over HTTP.
pub struct InfluxConnector {
    config: InfluxConnectorConfig,
    // TODO: HTTP client handle (e.g. reqwest::Client).
}

impl InfluxConnector {
    /// Build a connector from its configuration.
    pub fn new(config: InfluxConnectorConfig) -> Self {
        Self { config }
    }

    /// The connector configuration.
    pub fn config(&self) -> &InfluxConnectorConfig {
        &self.config
    }

    /// Build the line-protocol request body for a batch (real; uses
    /// [`payload::to_line_protocol_batch`]).
    pub fn encode_body(points: &[DataPoint]) -> String {
        payload::to_line_protocol_batch(points)
    }
}

#[async_trait]
impl DataPublisher for InfluxConnector {
    async fn connect(&mut self) -> Result<()> {
        // TODO: build the HTTP client; optionally ping `config.url`/health.
        Err(not_implemented("influx::connect"))
    }

    async fn publish(&mut self, points: &[DataPoint]) -> Result<()> {
        // Body construction is real; the HTTP POST to the /write endpoint is the
        // part to implement (auth via `config.token`, db via `config.bucket`).
        let _body = Self::encode_body(points);
        Err(not_implemented("influx::publish"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use staircase_core::model::Value;

    #[test]
    fn encode_body_joins_lines() {
        let pts = vec![
            DataPoint::new("gw", "modbus", "d", "t1", Value::Int(1)),
            DataPoint::new("gw", "modbus", "d", "t2", Value::Int(2)),
        ];
        let body = InfluxConnector::encode_body(&pts);
        assert_eq!(body.lines().count(), 2);
        assert!(body.contains("t1,"));
        assert!(body.contains("t2,"));
    }
}
