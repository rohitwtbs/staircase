//! Generic REST output connector (blueprint).

use async_trait::async_trait;
use serde::Deserialize;
use staircase_core::error::Result;
use staircase_core::model::DataPoint;
use staircase_core::traits::DataPublisher;

use crate::{not_implemented, payload};

/// Configuration for the REST connector.
#[derive(Debug, Clone, Deserialize)]
pub struct RestConnectorConfig {
    /// Endpoint URL to POST data points to.
    pub endpoint: String,
    /// Optional bearer token for `Authorization`.
    #[serde(default)]
    pub token: String,
}

/// POSTs serialized data points to a generic HTTP endpoint.
pub struct RestConnector {
    config: RestConnectorConfig,
    // TODO: HTTP client handle (e.g. reqwest::Client).
}

impl RestConnector {
    /// Build a connector from its configuration.
    pub fn new(config: RestConnectorConfig) -> Self {
        Self { config }
    }

    /// The connector configuration.
    pub fn config(&self) -> &RestConnectorConfig {
        &self.config
    }

    /// Build the JSON request body for a batch (real; uses
    /// [`payload::to_json_batch`]).
    pub fn encode_body(points: &[DataPoint]) -> Result<String> {
        payload::to_json_batch(points)
    }
}

#[async_trait]
impl DataPublisher for RestConnector {
    async fn connect(&mut self) -> Result<()> {
        // TODO: build the HTTP client (no persistent connection required).
        Err(not_implemented("rest::connect"))
    }

    async fn publish(&mut self, points: &[DataPoint]) -> Result<()> {
        // Body construction is real; the HTTP POST to `config.endpoint` (with an
        // optional bearer token) and 2xx-status check is the part to implement.
        let _body = Self::encode_body(points)?;
        Err(not_implemented("rest::publish"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use staircase_core::model::Value;

    #[test]
    fn encode_body_is_json_array() {
        let pts = vec![DataPoint::new("gw", "mqtt", "d", "t", Value::Bool(true))];
        let body = RestConnector::encode_body(&pts).unwrap();
        assert!(body.starts_with('['));
        assert!(body.contains("\"tag_name\":\"t\""));
    }
}
