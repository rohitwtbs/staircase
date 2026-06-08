//! Staircase umbrella binary.
//!
//! This is a small status/entry binary for the workspace. The full example
//! gateway (`examples/gateway.rs`) and end-to-end wiring are delivered by the
//! integration task. Each protocol, storage, rule, and connector capability
//! lives in its own crate under `crates/`.

fn main() {
    println!(
        "staircase v{} — modular building-automation / industrial-IoT edge gateway framework",
        staircase_core::version()
    );
    println!("workspace crates:");
    for crate_name in [
        "staircase-core",
        "staircase-bacnet",
        "staircase-modbus",
        "staircase-opcua",
        "staircase-mqtt",
        "staircase-knx",
        "staircase-storage",
        "staircase-rules",
        "staircase-connectors",
    ] {
        println!("  - {crate_name}");
    }
}
