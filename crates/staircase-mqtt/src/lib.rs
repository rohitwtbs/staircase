//! `staircase-mqtt` — MQTT client protocol driver for Staircase.
//!
//! Implements the **inbound** side of MQTT: it connects to a broker, subscribes
//! to configured topics, and converts received messages into
//! [`DataPoint`](staircase_core::DataPoint)s. (Outbound publishing of normalized
//! data lives in `staircase-connectors`.)
//!
//! MQTT is push-based, but [`ProtocolDriver::poll`] is pull-based, so the driver
//! runs the broker event loop in a background task that buffers incoming
//! messages. Each [`poll`](ProtocolDriver::poll) drains that buffer.
//!
//! ## Tag addressing
//!
//! Each tag's `address` is the MQTT topic to subscribe to. When a message
//! arrives, its topic is matched against the configured tags to recover the
//! logical tag name; unmatched topics fall back to using the topic as the name.
//! Payloads are decoded as float, then integer, then boolean, then string.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use staircase_core::config::DeviceConfig;
use staircase_core::error::{Result, StaircaseError};
use staircase_core::model::{DataPoint, Value};
use staircase_core::traits::ProtocolDriver;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

/// Decode a raw MQTT payload into a [`Value`].
///
/// Tries, in order: float, integer, boolean (`true`/`false`/`on`/`off`), then a
/// UTF-8 string, finally falling back to raw bytes.
pub fn payload_to_value(payload: &[u8]) -> Value {
    let Ok(text) = std::str::from_utf8(payload) else {
        return Value::Bytes(payload.to_vec());
    };
    let trimmed = text.trim();

    if let Ok(i) = trimmed.parse::<i64>() {
        return Value::Int(i);
    }
    if let Ok(f) = trimmed.parse::<f64>() {
        return Value::Float(f);
    }
    match trimmed.to_ascii_lowercase().as_str() {
        "true" | "on" => return Value::Bool(true),
        "false" | "off" => return Value::Bool(false),
        _ => {}
    }
    Value::String(trimmed.to_string())
}

/// A subscription: the MQTT topic and the logical tag name it maps to.
#[derive(Debug, Clone)]
struct Subscription {
    topic: String,
    tag_name: String,
}

/// MQTT client protocol driver (inbound collection).
pub struct MqttDriver {
    source: String,
    device_id: String,
    host: String,
    port: u16,
    client_id: String,
    credentials: Option<(String, String)>,
    subscriptions: Vec<Subscription>,
    buffer: Arc<Mutex<VecDeque<DataPoint>>>,
    client: Option<AsyncClient>,
    worker: Option<JoinHandle<()>>,
}

impl MqttDriver {
    /// Build a driver from a [`DeviceConfig`].
    ///
    /// `address` is `host:port` (port defaults to `1883`). Optional settings:
    /// `client_id`, `username`, `password`. Each tag's `address` is the topic to
    /// subscribe to.
    pub fn from_config(source: impl Into<String>, cfg: &DeviceConfig) -> Result<Self> {
        let (host, port) = split_host_port(&cfg.address, 1883)?;

        let client_id = cfg
            .settings
            .get("client_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("staircase-{}", cfg.name));

        let username = cfg
            .settings
            .get("username")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let password = cfg
            .settings
            .get("password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let credentials = match (username, password) {
            (Some(u), Some(p)) => Some((u, p)),
            (Some(u), None) => Some((u, String::new())),
            _ => None,
        };

        let subscriptions = cfg
            .tags
            .iter()
            .map(|t| Subscription {
                topic: t.address.clone(),
                tag_name: t.name.clone(),
            })
            .collect();

        Ok(Self {
            source: source.into(),
            device_id: cfg.name.clone(),
            host,
            port,
            client_id,
            credentials,
            subscriptions,
            buffer: Arc::new(Mutex::new(VecDeque::new())),
            client: None,
            worker: None,
        })
    }

    /// Resolve an inbound topic to its configured tag name (or the topic itself).
    fn tag_for_topic(subs: &[Subscription], topic: &str) -> String {
        subs.iter()
            .find(|s| topic_matches(&s.topic, topic))
            .map(|s| s.tag_name.clone())
            .unwrap_or_else(|| topic.to_string())
    }
}

#[async_trait]
impl ProtocolDriver for MqttDriver {
    fn protocol(&self) -> &str {
        "mqtt"
    }

    async fn connect(&mut self) -> Result<()> {
        let mut opts = MqttOptions::new(&self.client_id, &self.host, self.port);
        opts.set_keep_alive(Duration::from_secs(30));
        if let Some((user, pass)) = &self.credentials {
            opts.set_credentials(user, pass);
        }

        let (client, mut eventloop) = AsyncClient::new(opts, 64);

        for sub in &self.subscriptions {
            client
                .subscribe(&sub.topic, QoS::AtLeastOnce)
                .await
                .map_err(|e| {
                    StaircaseError::connection(format!(
                        "mqtt subscribe to '{}' failed: {e}",
                        sub.topic
                    ))
                })?;
        }

        let buffer = Arc::clone(&self.buffer);
        let subs = self.subscriptions.clone();
        let source = self.source.clone();
        let device_id = self.device_id.clone();

        let worker = tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        let tag_name = MqttDriver::tag_for_topic(&subs, &publish.topic);
                        let value = payload_to_value(&publish.payload);
                        let point = DataPoint::new(
                            source.clone(),
                            "mqtt",
                            device_id.clone(),
                            tag_name,
                            value,
                        );
                        buffer.lock().await.push_back(point);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        // rumqttc reconnects on the next poll; back off to avoid spinning.
                        debug!(device = %device_id, error = %e, "mqtt event loop error; retrying");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        self.client = Some(client);
        self.worker = Some(worker);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(worker) = self.worker.take() {
            worker.abort();
        }
        if let Some(client) = self.client.take() {
            if let Err(e) = client.disconnect().await {
                warn!(device = %self.device_id, error = %e, "mqtt disconnect failed");
            }
        }
        self.buffer.lock().await.clear();
        Ok(())
    }

    async fn poll(&mut self) -> Result<Vec<DataPoint>> {
        if self.worker.is_none() {
            return Err(StaircaseError::connection("mqtt driver is not connected"));
        }
        let mut buffer = self.buffer.lock().await;
        Ok(buffer.drain(..).collect())
    }
}

impl Drop for MqttDriver {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.abort();
        }
    }
}

/// Match a subscription filter (supporting `+` and `#` wildcards) to a topic.
fn topic_matches(filter: &str, topic: &str) -> bool {
    if filter == topic {
        return true;
    }
    let mut f = filter.split('/');
    let mut t = topic.split('/');
    loop {
        match (f.next(), t.next()) {
            (Some("#"), _) => return true,
            (Some("+"), Some(_)) => continue,
            (Some(a), Some(b)) if a == b => continue,
            (None, None) => return true,
            _ => return false,
        }
    }
}

fn split_host_port(address: &str, default_port: u16) -> Result<(String, u16)> {
    let address = address.trim();
    match address.rsplit_once(':') {
        Some((host, port)) => {
            let port = port.parse().map_err(|_| {
                StaircaseError::config(format!("invalid mqtt port in '{address}'"))
            })?;
            Ok((host.to_string(), port))
        }
        None => Ok((address.to_string(), default_port)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_payloads() {
        assert_eq!(payload_to_value(b"42"), Value::Int(42));
        assert_eq!(payload_to_value(b"2.5"), Value::Float(2.5));
        assert_eq!(payload_to_value(b"true"), Value::Bool(true));
        assert_eq!(payload_to_value(b"OFF"), Value::Bool(false));
        assert_eq!(payload_to_value(b"hello"), Value::String("hello".into()));
        assert_eq!(payload_to_value(b"  7 "), Value::Int(7));
    }

    #[test]
    fn matches_topics_with_wildcards() {
        assert!(topic_matches("a/b/c", "a/b/c"));
        assert!(topic_matches("a/+/c", "a/x/c"));
        assert!(topic_matches("a/#", "a/b/c/d"));
        assert!(!topic_matches("a/+/c", "a/x/d"));
        assert!(!topic_matches("a/b", "a/b/c"));
    }

    #[test]
    fn resolves_tag_names() {
        let subs = vec![
            Subscription {
                topic: "sensors/+/temp".into(),
                tag_name: "temperature".into(),
            },
            Subscription {
                topic: "sensors/room1/hum".into(),
                tag_name: "humidity".into(),
            },
        ];
        assert_eq!(MqttDriver::tag_for_topic(&subs, "sensors/room1/temp"), "temperature");
        assert_eq!(MqttDriver::tag_for_topic(&subs, "sensors/room1/hum"), "humidity");
        // Unmatched topic falls back to the topic string.
        assert_eq!(MqttDriver::tag_for_topic(&subs, "other/topic"), "other/topic");
    }

    #[test]
    fn splits_host_and_port() {
        assert_eq!(
            split_host_port("broker.local", 1883).unwrap(),
            ("broker.local".to_string(), 1883)
        );
        assert_eq!(
            split_host_port("broker.local:8883", 1883).unwrap(),
            ("broker.local".to_string(), 8883)
        );
        assert!(split_host_port("broker.local:notaport", 1883).is_err());
    }
}
