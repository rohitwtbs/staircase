//! Kafka output connector (blueprint).

use async_trait::async_trait;
use serde::Deserialize;
use staircase_core::error::Result;
use staircase_core::model::DataPoint;
use staircase_core::traits::DataPublisher;

use crate::not_implemented;

/// Configuration for the Kafka connector.
#[derive(Debug, Clone, Deserialize)]
pub struct KafkaConnectorConfig {
    /// Comma-separated bootstrap broker list.
    pub brokers: String,
    /// Topic to produce to.
    pub topic: String,
}

/// Produces data points to Kafka topics.
pub struct KafkaConnector {
    config: KafkaConnectorConfig,
    // TODO: async Kafka producer handle (e.g. rdkafka FutureProducer).
}

impl KafkaConnector {
    /// Build a connector from its configuration.
    pub fn new(config: KafkaConnectorConfig) -> Self {
        Self { config }
    }

    /// The connector configuration.
    pub fn config(&self) -> &KafkaConnectorConfig {
        &self.config
    }
}

#[async_trait]
impl DataPublisher for KafkaConnector {
    async fn connect(&mut self) -> Result<()> {
        // TODO: create the producer against `config.brokers`.
        Err(not_implemented("kafka::connect"))
    }

    async fn publish(&mut self, points: &[DataPoint]) -> Result<()> {
        // TODO: serialize each point (crate::payload::to_json), produce to
        // `config.topic` keyed by device/tag, and await broker acks.
        let _ = points;
        Err(not_implemented("kafka::publish"))
    }
}
