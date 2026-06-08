//! Test utilities, including a reusable mock [`ProtocolDriver`].
//!
//! The [`MockDriver`] emits deterministic synthetic data and can be configured
//! to fail, which is handy for unit and integration tests across the workspace.

use async_trait::async_trait;

use crate::error::{Result, StaircaseError};
use crate::model::{DataPoint, Value};
use crate::traits::{DataCollector, ProtocolDriver};

/// A deterministic mock protocol driver for testing.
///
/// Each [`poll`](ProtocolDriver::poll) increments an internal counter and emits a
/// single integer [`DataPoint`]. Set [`fail_on_poll`](MockDriver::fail_on_poll)
/// to make polling return a protocol error.
#[derive(Debug, Clone)]
pub struct MockDriver {
    /// The device id reported in emitted points.
    pub device_id: String,
    /// The protocol label reported by the driver.
    pub protocol: String,
    /// The tag name reported in emitted points.
    pub tag_name: String,
    /// When `true`, [`poll`](ProtocolDriver::poll) returns an error.
    pub fail_on_poll: bool,
    counter: i64,
    connected: bool,
}

impl MockDriver {
    /// Create a new mock driver for the given device id.
    pub fn new(device_id: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            protocol: "mock".to_string(),
            tag_name: "counter".to_string(),
            fail_on_poll: false,
            counter: 0,
            connected: false,
        }
    }

    /// Builder-style setter to make the driver fail on poll.
    pub fn failing(mut self) -> Self {
        self.fail_on_poll = true;
        self
    }

    /// Whether [`connect`](ProtocolDriver::connect) has been called.
    pub fn is_connected(&self) -> bool {
        self.connected
    }
}

#[async_trait]
impl ProtocolDriver for MockDriver {
    fn protocol(&self) -> &str {
        &self.protocol
    }

    async fn connect(&mut self) -> Result<()> {
        self.connected = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.connected = false;
        Ok(())
    }

    async fn poll(&mut self) -> Result<Vec<DataPoint>> {
        if self.fail_on_poll {
            return Err(StaircaseError::protocol("mock driver configured to fail"));
        }
        self.counter += 1;
        Ok(vec![DataPoint::new(
            "mock-collector",
            self.protocol.clone(),
            self.device_id.clone(),
            self.tag_name.clone(),
            Value::Int(self.counter),
        )])
    }
}

#[async_trait]
impl DataCollector for MockDriver {
    async fn collect(&mut self) -> Result<Vec<DataPoint>> {
        self.poll().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_driver_emits_incrementing_points() {
        let mut driver: Box<dyn ProtocolDriver> = Box::new(MockDriver::new("d1"));
        driver.connect().await.unwrap();

        let first = driver.poll().await.unwrap();
        let second = driver.poll().await.unwrap();
        assert_eq!(first[0].value, Value::Int(1));
        assert_eq!(second[0].value, Value::Int(2));
        assert_eq!(first[0].device_id, "d1");
    }

    #[tokio::test]
    async fn failing_mock_driver_returns_protocol_error() {
        let mut driver = MockDriver::new("d1").failing();
        let err = driver.poll().await.unwrap_err();
        assert!(matches!(err, StaircaseError::Protocol(_)));
    }

    #[tokio::test]
    async fn mock_driver_is_a_data_collector() {
        let mut collector: Box<dyn DataCollector> = Box::new(MockDriver::new("d2"));
        let points = collector.collect().await.unwrap();
        assert_eq!(points.len(), 1);
    }
}
