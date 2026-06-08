# staircase

A modular building-automation / industrial-IoT edge gateway framework implemented in Rust. It provides a unified API for collecting, normalizing, processing, and forwarding data from multiple field protocols (BACnet, Modbus, OPC UA, MQTT, KNX, etc.).

## Overview

This is a Cargo **workspace**. The root crate (`staircase`) is a small umbrella binary; the framework itself is split into focused member crates under `crates/`. There is no frontend or web server; it runs as a console application.

The protocol-independent `staircase-core` crate is the foundation every other crate builds on. It defines the unified data model, the core traits, structured errors, the configuration model, async runtime/supervision scaffolding, and observability hooks. `staircase-core` does NOT depend on any protocol implementation crate.

## Project Structure

- `Cargo.toml` — workspace manifest (shared dependency versions in `[workspace.dependencies]`)
- `src/main.rs` — umbrella `staircase` binary (status/entry point)
- `crates/staircase-core` — data model, traits, config, runtime, observability (implemented)
- `crates/staircase-bacnet` — BACnet/IP driver (implemented: ReadProperty present-value over UDP)
- `crates/staircase-modbus` — Modbus TCP driver (implemented: coils/discretes/holding/input registers)
- `crates/staircase-opcua` — OPC UA driver (stub, future)
- `crates/staircase-mqtt` — MQTT client driver (implemented: inbound topic subscription)
- `crates/staircase-knx` — KNX driver (stub, future)
- `crates/staircase-storage` — RocksDB store-and-forward (blueprint: design, config, `StorageEngine` skeleton; impl filled in gradually)
- `crates/staircase-rules` — edge rule engine (blueprint: rule/condition/action config model, `RuleEngine` skeleton; eval filled in gradually)
- `crates/staircase-connectors` — output connectors (blueprint: MQTT/Kafka/NATS/Postgres/Timescale/InfluxDB/REST `DataPublisher` skeletons; real shared payload mapping (JSON + InfluxDB line protocol); network I/O filled in gradually)

## Development

- Toolchain: Rust stable (installed as a Replit module)
- Build: `cargo build`
- Test: `cargo test`
- Lint: `cargo clippy --workspace --all-targets`
- Run: `cargo run` (configured as the "Start application" workflow, console output)

## User Preferences

(none recorded yet)
