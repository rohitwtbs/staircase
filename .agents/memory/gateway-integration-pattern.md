---
name: gateway integration pattern
description: How Staircase wires real + blueprint stages together in the example gateway and integration tests
---

# Gateway integration (example + tests)

The integration layer lives as an **example**, not a new crate: `examples/gateway.rs`
(root `staircase` package) + `tests/gateway_integration.rs`. The root crate is a
binary, so the example/tests pull the local crates via root `[dev-dependencies]`
(workspace path deps).

**Pattern for demonstrating mixed real/blueprint stages:** call every stage in the
real pipeline order (collect → payload map → rules → store → publish). Real stages
(MockDriver collect, `Metrics`, `staircase_connectors::payload`) assert success;
blueprint stages (rules `evaluate`, storage `store`, connector `publish`) are called
and their uniform "not implemented (blueprint)" error is handled with a `match` +
`warn!`, so the process still exits 0. Keep docs/comments honest — if a stage is
documented in the pipeline, it must actually be *called* (a code review caught the
rules stage being documented but not invoked).

**Why:** the user directive is that storage/rules/connectors stay compiling
blueprints filled in gradually; the integration must show the full shape without
faking results or hiding that those stages are pending.

**Workflow:** "Start application" runs `cargo run --example gateway` (console,
bounded demo that prints a metrics snapshot and exits). `cargo run` still runs the
umbrella status binary.
