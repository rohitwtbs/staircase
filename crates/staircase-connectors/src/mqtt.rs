//! MQTT output connector (blueprint).

use async_trait::async_trait;
use serde::Deserialize;
use staircase_core::error::Result;
use staircase_core::model::DataPoint;
use staircase_core::traits::DataPublisher;

use crate::not_implemented;

/// Configuration for the MQTT connector.
#[derive(Debug, Clone, Deserialize)]
pub struct MqttConnectorConfig {
    /// Broker address `host:port`.
    pub address: String,
    /// Client id to connect with.
    #[serde(default)]
    pub client_id: String,
    /// Topic (or topic prefix) to publish to.
    pub topic: String,
}

/// Publishes data points to MQTT topics.
pub struct MqttConnector {
    config: MqttConnectorConfig,
    // TODO: async MQTT client handle (e.g. rumqttc AsyncClient + eventloop task).
}

impl MqttConnector {
    /// Build a connector from its configuration.
    pub fn new(config: MqttConnectorConfig) -> Self {
        Self { config }
    }

    /// The connector configuration.
    pub fn config(&self) -> &MqttConnectorConfig {
        &self.config
    }
}

#[async_trait]
impl DataPublisher for MqttConnector {
    async fn connect(&mut self) -> Result<()> {
        // TODO: connect to `config.address`, spawn the event loop, await CONNACK.
        Err(not_implemented("mqtt::connect"))
    }

    async fn publish(&mut self, points: &[DataPoint]) -> Result<()> {
        // TODO: serialize each point (see crate::payload::to_json) and publish
        // to `config.topic` (optionally per-tag subtopics); confirm via QoS ack.
        let _ = points;
        Err(not_implemented("mqtt::publish"))
    }
}
