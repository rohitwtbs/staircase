//! Example Staircase gateway.
//!
//! This example wires the framework together end-to-end and runs cleanly without
//! field hardware. It demonstrates the gateway pipeline:
//!
//! ```text
//! config (YAML) -> collectors -> normalize -> rules -> store-and-forward -> connectors
//!                                     |                                          |
//!                                  metrics  <----------- observability ----------+
//! ```
//!
//! Run with: `cargo run --example gateway [path/to/config.yaml]`
//! (defaults to `examples/gateway.yaml`).
//!
//! ## What is real vs. blueprint
//!
//! The collection path, the observability [`Metrics`], and the connector payload
//! mapping (`staircase_connectors::payload`) are real. The rule engine, storage
//! layer, and connector network I/O are blueprints filled in gradually; this
//! example calls into them and handles their "not yet implemented" results
//! gracefully so the pipeline shape is visible. Production code instantiates the
//! concrete `ProtocolDriver` per protocol (BACnet/Modbus/MQTT) instead of the
//! in-tree `MockDriver` used here.

use std::time::Instant;

use staircase_connectors::mqtt::{MqttConnector, MqttConnectorConfig};
use staircase_connectors::payload;
use staircase_core::config::{Config, load_config_file};
use staircase_core::observability::{Metrics, init_tracing};
use staircase_core::testing::MockDriver;
use staircase_core::traits::{DataPublisher, ProtocolDriver, RuleEngine, StorageEngine};
use staircase_rules::{RuleEngineImpl, RuleSet};
use staircase_storage::{StorageConfig, StoreAndForward};
use tracing::{info, warn};

const POLLS_PER_DEVICE: usize = 3;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing("info");

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "examples/gateway.yaml".to_string());
    let config: Config = load_config_file(&config_path)?;
    info!(
        path = %config_path,
        devices = config.devices.len(),
        rules = config.rules.len(),
        connectors = config.connectors.len(),
        "loaded gateway configuration"
    );

    let metrics = Metrics::new();

    // Edge rule engine (blueprint: evaluation pending). Production translates the
    // string-based `config.rules` into the engine's `RuleSet`; the gradual impl
    // fills that mapping in, so here we start from an empty default rule set.
    let mut rules = RuleEngineImpl::new(RuleSet::default());

    // Local store-and-forward buffer (blueprint: open() is real, enqueue pending).
    let storage_path = config
        .storage
        .as_ref()
        .map(|s| s.path.clone())
        .unwrap_or_else(|| StorageConfig::default().path);
    let store = StoreAndForward::open(StorageConfig {
        path: storage_path,
        ..StorageConfig::default()
    })?;

    // Output connector (blueprint: publish network I/O pending).
    let mut connector = MqttConnector::new(MqttConnectorConfig {
        address: "tcp://broker:1883".to_string(),
        client_id: "staircase-gateway".to_string(),
        topic: "staircase/data".to_string(),
    });

    for device in &config.devices {
        // Production: pick the concrete driver by `device.protocol`. Here we use
        // the in-tree MockDriver so the example runs without field hardware.
        let mut driver = MockDriver::new(device.name.clone());
        driver.connect().await?;
        info!(device = %device.name, protocol = %device.protocol, "collector started");

        for _ in 0..POLLS_PER_DEVICE {
            let start = Instant::now();
            let points = driver.poll().await?;
            metrics.record_poll_latency(start.elapsed().as_millis() as u64);

            for point in &points {
                // Connector payload mapping is real.
                let json = payload::to_json(point)?;
                info!(point = %json, "normalized data point");

                // Stage: edge rules (blueprint). Production acts on the emitted
                // outcomes (set tag / raise alarm / emit event).
                match rules.evaluate(point) {
                    Ok(outcomes) => info!(count = outcomes.len(), "rule outcomes"),
                    Err(e) => warn!(error = %e, "rule stage is a blueprint — evaluation pending"),
                }
            }

            // Stage: store-and-forward (blueprint).
            match store.store(&points).await {
                Ok(()) => {
                    if let Ok(n) = store.len().await {
                        metrics.set_queue_size(n as u64);
                    }
                }
                Err(e) => warn!(error = %e, "storage stage is a blueprint — buffering pending"),
            }

            // Stage: publish to connector (blueprint). On confirmed delivery the
            // gateway would ack the buffered records and count throughput.
            match connector.publish(&points).await {
                Ok(()) => metrics.add_throughput(points.len() as u64),
                Err(e) => warn!(error = %e, "connector stage is a blueprint — publishing pending"),
            }
        }

        driver.disconnect().await?;
    }

    let snapshot = metrics.snapshot();
    info!(?snapshot, "gateway demo complete");
    println!(
        "\n--- metrics snapshot ---\n{}",
        serde_json::to_string_pretty(&snapshot)?
    );
    Ok(())
}
