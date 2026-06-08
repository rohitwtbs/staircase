//! End-to-end integration tests for the gateway pipeline.
//!
//! These exercise the real, implemented path (collection via the mock driver,
//! observability metrics, and connector payload mapping), the shipped example
//! configuration, and the current blueprint behavior of the storage, rule, and
//! connector stages.

use staircase_connectors::payload;
use staircase_connectors::rest::{RestConnector, RestConnectorConfig};
use staircase_core::config::load_config_str;
use staircase_core::model::{DataPoint, Value};
use staircase_core::observability::Metrics;
use staircase_core::testing::MockDriver;
use staircase_core::traits::{DataCollector, DataPublisher, ProtocolDriver, RuleEngine, StorageEngine};
use staircase_rules::{RuleEngineImpl, RuleSet};
use staircase_storage::{StorageConfig, StoreAndForward};

/// Collect → normalize → metrics → connector payload mapping, end to end.
#[tokio::test]
async fn collect_normalize_and_map_end_to_end() {
    let mut driver = MockDriver::new("dev1");
    driver.connect().await.unwrap();
    assert!(driver.is_connected());

    let metrics = Metrics::new();
    let mut collected: Vec<DataPoint> = Vec::new();
    for _ in 0..5 {
        let points = DataCollector::collect(&mut driver).await.unwrap();
        metrics.record_poll_latency(1);
        metrics.add_throughput(points.len() as u64);
        collected.extend(points);
    }

    assert_eq!(collected.len(), 5);
    // MockDriver emits an incrementing counter, proving the loop really ran.
    assert_eq!(collected[0].value, Value::Int(1));
    assert_eq!(collected[4].value, Value::Int(5));

    let snap = metrics.snapshot();
    assert_eq!(snap.poll_count, 5);
    assert_eq!(snap.throughput, 5);

    // Connector payload mapping is real.
    let json = payload::to_json_batch(&collected).unwrap();
    assert!(json.starts_with('['));
    let lines = payload::to_line_protocol_batch(&collected);
    assert_eq!(lines.lines().count(), 5);
}

/// The gateway loop drives every stage: collection + metrics + payload mapping
/// succeed, while the rules, storage, and connector stages report their blueprint
/// status — exactly the flow the example gateway runs.
#[tokio::test]
async fn gateway_pipeline_drives_all_stages() {
    let mut driver = MockDriver::new("dev1");
    driver.connect().await.unwrap();

    let metrics = Metrics::new();
    let mut rules = RuleEngineImpl::new(RuleSet::default());
    let store = StoreAndForward::open(StorageConfig::default()).unwrap();
    let mut connector = RestConnector::new(RestConnectorConfig {
        endpoint: "http://localhost/ingest".to_string(),
        token: String::new(),
    });

    let mut blueprint_errors = 0usize;
    for _ in 0..3 {
        let points = driver.poll().await.unwrap();
        metrics.record_poll_latency(1);

        for point in &points {
            // Real stage.
            assert!(payload::to_json(point).unwrap().contains("\"value\""));
            // Blueprint stage exercised within the flow.
            if rules.evaluate(point).is_err() {
                blueprint_errors += 1;
            }
        }
        if store.store(&points).await.is_err() {
            blueprint_errors += 1;
        }
        if connector.publish(&points).await.is_err() {
            blueprint_errors += 1;
        }
    }

    assert_eq!(metrics.snapshot().poll_count, 3);
    // 3 cycles x (1 rule + 1 store + 1 publish) blueprint errors, all handled.
    assert_eq!(blueprint_errors, 9);
}

/// The shipped example configuration parses into the core config model.
#[test]
fn shipped_example_config_parses() {
    let yaml = include_str!("../examples/gateway.yaml");
    let cfg = load_config_str(yaml).unwrap();
    assert_eq!(cfg.devices.len(), 3);
    assert_eq!(cfg.rules.len(), 2);
    assert_eq!(cfg.connectors.len(), 1);
    assert!(cfg.storage.is_some());
    assert!(cfg.observability.is_some());
}

/// The storage, rule, and connector stages are blueprints today: constructors
/// succeed and pure mapping works, while behavioral calls report "not yet
/// implemented" rather than silently succeeding.
#[tokio::test]
async fn blueprint_stages_report_pending() {
    let points = vec![DataPoint::new("gw", "mock", "d", "t", Value::Int(1))];

    let store = StoreAndForward::open(StorageConfig::default()).unwrap();
    assert!(store.store(&points).await.is_err());

    let mut connector = RestConnector::new(RestConnectorConfig {
        endpoint: "http://localhost/ingest".to_string(),
        token: String::new(),
    });
    assert!(connector.publish(&points).await.is_err());
    // ...but the connector's body encoding is real:
    let body = RestConnector::encode_body(&points).unwrap();
    assert!(body.contains("\"tag_name\":\"t\""));

    let mut engine = RuleEngineImpl::new(RuleSet::default());
    assert!(engine.evaluate(&points[0]).is_err());
}
