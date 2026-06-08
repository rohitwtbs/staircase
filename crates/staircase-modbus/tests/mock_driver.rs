//! End-to-end test of the core collector abstraction against the shared
//! [`MockDriver`](staircase_core::testing::MockDriver).
//!
//! The mock driver lives in `staircase-core` so every protocol crate (and the
//! integration/gateway task) can reuse it for deterministic tests without a live
//! device.

use staircase_core::model::Value;
use staircase_core::testing::MockDriver;
use staircase_core::traits::{DataCollector, ProtocolDriver};

#[tokio::test]
async fn mock_driver_polls_as_protocol_driver() {
    let mut driver = MockDriver::new("dev-1");
    driver.connect().await.unwrap();
    assert!(driver.is_connected());

    let first = driver.poll().await.unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].device_id, "dev-1");
    assert_eq!(first[0].protocol, "mock");
    assert_eq!(first[0].value, Value::Int(1));

    let second = driver.poll().await.unwrap();
    assert_eq!(second[0].value, Value::Int(2), "mock should emit changing values");
}

#[tokio::test]
async fn mock_driver_collects_via_collector_trait() {
    let mut driver = MockDriver::new("dev-2");
    driver.connect().await.unwrap();

    let batch = DataCollector::collect(&mut driver).await.unwrap();
    assert_eq!(batch.len(), 1);
    assert!(batch.iter().all(|p| p.protocol == "mock"));
}

#[tokio::test]
async fn mock_driver_can_be_configured_to_fail() {
    let mut driver = MockDriver::new("dev-3").failing();
    driver.connect().await.unwrap();
    assert!(driver.poll().await.is_err());
}
