//! NATS output connector (blueprint).

use async_trait::async_trait;
use serde::Deserialize;
use staircase_core::error::Result;
use staircase_core::model::DataPoint;
use staircase_core::traits::DataPublisher;

use crate::not_implemented;

/// Configuration for the NATS connector.
#[derive(Debug, Clone, Deserialize)]
pub struct NatsConnectorConfig {
    /// NATS server URL (e.g. `nats://host:4222`).
    pub url: String,
    /// Subject to publish to.
    pub subject: String,
}

/// Publishes data points to NATS subjects.
pub struct NatsConnector {
    config: NatsConnectorConfig,
    // TODO: async NATS client handle (e.g. async-nats Client).
}

impl NatsConnector {
    /// Build a connector from its configuration.
    pub fn new(config: NatsConnectorConfig) -> Self {
        Self { config }
    }

    /// The connector configuration.
    pub fn config(&self) -> &NatsConnectorConfig {
        &self.config
    }
}

#[async_trait]
impl DataPublisher for NatsConnector {
    async fn connect(&mut self) -> Result<()> {
        // TODO: connect to `config.url`.
        Err(not_implemented("nats::connect"))
    }

    async fn publish(&mut self, points: &[DataPoint]) -> Result<()> {
        // TODO: serialize each point (crate::payload::to_json) and publish to
        // `config.subject`; flush to confirm delivery.
        let _ = points;
        Err(not_implemented("nats::publish"))
    }
}
