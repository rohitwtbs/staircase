---
name: protocol driver patterns
description: Cross-cutting conventions for Staircase ProtocolDriver implementations
---

# Staircase protocol driver patterns

Conventions shared by the field-protocol driver crates (modbus/mqtt/bacnet/…).

- **`ProtocolDriver: Send + Sync`** — any non-Sync transport handle (e.g.
  tokio-modbus `Context`) must be wrapped in a `tokio::sync::Mutex` so the driver
  stays `Sync`. **Why:** the core trait requires `Sync` for `Box<dyn>` use.
- **Construction:** each driver exposes `from_config(source, &DeviceConfig)`.
  Tag `address` and `data_type` are protocol-specific and parsed at construction
  so per-tag errors surface early. Per-driver address conventions:
  modbus `holding|input|coil|discrete:N` (bare N = holding); bacnet
  `object-type:instance` (e.g. `ai:5`); mqtt tag address = topic.
- **Resilient poll:** `poll()` reads each tag; a single failed tag is logged via
  `tracing::warn` and skipped, not propagated, so one bad point doesn't drop the
  whole batch.
- **Push protocols (MQTT):** run the broker event loop in a spawned task that
  buffers incoming messages into an `Arc<Mutex<VecDeque<DataPoint>>>`; `poll()`
  drains the buffer. Abort the task on `disconnect`/`Drop`.
- **Binary protocol decoding (BACnet):** parse frame fields strictly in order and
  validate request/response correlation (invoke-id + object-id + property) —
  never byte-scan for marker bytes, and never trust an unsolicited UDP datagram.
- **Testing without hardware:** keep codec/parse logic in free functions and unit
  test them; add end-to-end tests with in-process fixture servers
  (`TcpListener` MBAP server for Modbus, `UdpSocket` for BACnet) to exercise
  connect→poll→DataPoint. The reusable `staircase_core::testing::MockDriver`
  covers the collector abstraction.
