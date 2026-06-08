---
name: workspace layout
description: Why the staircase repo root is both a workspace root and an umbrella binary
---

# Staircase workspace layout

The repo root `Cargo.toml` is intentionally BOTH a `[workspace]` root and a small
`[package]` (umbrella binary `staircase`). The nine framework crates live under
`crates/` (staircase-core + bacnet/modbus/opcua/mqtt/knx/storage/rules/connectors).

**Why:** the Replit "Start application" workflow runs `cargo run`. A pure virtual
workspace has no default binary, so `cargo run` would fail. Keeping a thin root
binary keeps the workflow green without `-p`. Shared dependency versions are
centralized in `[workspace.dependencies]`; member crates reference them with
`{ workspace = true }`.

**How to apply:** when adding crates, add them under `crates/` and pin shared
deps in the root `[workspace.dependencies]`. `staircase-core` must never depend on
a protocol implementation crate (open/closed: new protocols are new crates).
