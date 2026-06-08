---
name: Crate delivery — blueprint-first
description: User's standing directive on how much to implement per Staircase crate
---

# Crate delivery: blueprint-first

The user implements the remaining Staircase crates **gradually themselves**. For
each not-yet-built crate (storage, rules, connectors, and similar), deliver a
**compiling blueprint/scaffold**, not a finished implementation.

**Why:** the user explicitly said "for every crate you do not have to do the
implementation, you have to just have a blueprint ready, actual implementations
i will do it gradually." (stated June 2026, during the storage task). This
overrides task specs that say "implement" for crates not yet started. Tasks #1
(core) and #2 (protocol drivers) were already fully implemented and merged before
this directive, so they are exempt.

**How to apply:** a blueprint should
- compile and keep the workspace green (so `cargo run`/CI stays clean),
- document the intended design in module/method doc comments,
- define the real public types and config structs,
- implement the relevant core trait with method skeletons that return a uniform
  "not implemented (blueprint)" error (or `todo!()`), each preceded by a comment
  describing the intended behavior,
- include light tests only for the parts that are real now (e.g. config
  defaults / constructor), leaving the behavioral tests for the real impl.
