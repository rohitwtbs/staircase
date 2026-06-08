//! `staircase-rules` — lightweight edge rule engine for Staircase.
//!
//! # Status: blueprint
//!
//! This crate is a **compiling scaffold**, not a finished implementation. It
//! defines the rule configuration model and the [`RuleEngine`] surface so the
//! local condition/action evaluator can be filled in gradually. The evaluation
//! body is stubbed; the config types are real and parseable now.
//!
//! # Goal
//!
//! Evaluate incoming [`DataPoint`]s against rules loaded from configuration and
//! emit zero or more [`RuleOutcome`]s, fully locally (no cloud dependency).
//! Example rules: `room_temp > 28 => set fan = true`, `humidity > 80 => alarm`.
//!
//! # Intended design
//!
//! - **Rule model:** a [`Rule`] is a [`Condition`] plus one or more [`Action`]s.
//!   A condition is a boolean expression over tag values built from leaf
//!   comparisons ([`Comparison`]: `tag <op> value`) combined with `all`/`any`
//!   (AND/OR) and `not`. Keep the expression set lightweight — comparisons and
//!   basic boolean logic only, no general scripting.
//! - **Operators:** `==`, `!=`, `>`, `>=`, `<`, `<=` over numeric/bool/string
//!   values (see [`Operator`]). Numeric comparisons go through
//!   [`Value::as_f64`]; equality also supports bool/string.
//! - **State:** the engine keeps a small map of the latest [`Value`] per
//!   `(device_id, tag_name)` so rules referencing multiple tags can evaluate.
//!   On each `evaluate(point)`, update state for the incoming point, then
//!   evaluate every rule whose condition references known tags.
//! - **Actions → outcomes:** map each fired [`Action`] to a [`RuleOutcome`]:
//!   `SetTag` → [`RuleOutcome::SetTag`], `RaiseAlarm` → [`RuleOutcome::RaiseAlarm`]
//!   (build an [`Alarm`] with the configured [`Severity`] and message),
//!   `EmitEvent` → [`RuleOutcome::EmitEvent`].
//! - **Determinism & side effects:** `evaluate` is pure w.r.t. the engine's own
//!   tag state — it returns outcomes rather than performing I/O. Edge-triggering
//!   (fire only on transition into the true state) can be added by tracking each
//!   rule's last condition result.
//! - **Observability:** surface evaluation errors / fired-rule counts via the
//!   core observability hooks where appropriate.
//!
//! # Tests to add alongside the implementation
//!
//! - parse representative rules from config (thresholds, boolean combos),
//! - `>` / `<` / `==` threshold evaluation produces the expected outcome,
//! - boolean `all` / `any` / `not` combinations,
//! - alarm generation with the configured severity/message,
//! - multi-tag rules using retained state.

use serde::{Deserialize, Serialize};
use staircase_core::error::{Result, StaircaseError};
use staircase_core::model::{DataPoint, Severity, Value};
use staircase_core::traits::{RuleEngine, RuleOutcome};

/// Comparison operator for a rule [`Comparison`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Ge,
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
}

/// A single leaf comparison: `tag <op> value`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    /// Tag name the comparison reads (optionally `device.tag` qualified).
    pub tag: String,
    /// The comparison operator.
    pub op: Operator,
    /// The right-hand-side literal to compare against.
    pub value: Value,
}

/// A boolean condition tree over tag comparisons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    /// A leaf comparison.
    Compare(Comparison),
    /// True iff all sub-conditions are true (AND).
    All(Vec<Condition>),
    /// True iff any sub-condition is true (OR).
    Any(Vec<Condition>),
    /// Negation.
    Not(Box<Condition>),
}

/// An action produced when a rule's condition holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Assign a value to a `(device, tag)` pair.
    SetTag {
        /// Target device id.
        device_id: String,
        /// Target tag name.
        tag_name: String,
        /// Value to set.
        value: Value,
    },
    /// Raise an alarm with the given severity and message.
    RaiseAlarm {
        /// Alarm severity.
        severity: Severity,
        /// Human-readable message.
        message: String,
    },
    /// Emit an event of the given kind/message.
    EmitEvent {
        /// Event kind/category.
        kind: String,
        /// Human-readable message.
        message: String,
    },
}

/// A single rule: when `condition` holds, perform `actions`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    /// Stable rule identifier (used in logs/alarms).
    pub id: String,
    /// The condition to evaluate.
    pub condition: Condition,
    /// Actions to perform when the condition holds.
    pub actions: Vec<Action>,
}

/// A parsed set of rules loaded from configuration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RuleSet {
    /// The rules, evaluated in order.
    #[serde(default)]
    pub rules: Vec<Rule>,
}

impl RuleSet {
    /// Parse a [`RuleSet`] from a YAML document.
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        serde_yaml_from_str(yaml)
    }
}

/// Local edge rule engine (blueprint).
///
/// Holds the rule set and, once implemented, the retained per-tag state used to
/// evaluate rules that reference multiple tags.
pub struct RuleEngineImpl {
    rules: RuleSet,
    // TODO: retained latest value per (device_id, tag_name), e.g.:
    //   state: std::collections::HashMap<(String, String), Value>,
    //   last_result: std::collections::HashMap<String, bool>, // for edge-triggering
}

impl RuleEngineImpl {
    /// Build an engine from an already-parsed [`RuleSet`].
    pub fn new(rules: RuleSet) -> Self {
        Self { rules }
    }

    /// Build an engine by parsing rules from a YAML document.
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        Ok(Self::new(RuleSet::from_yaml(yaml)?))
    }

    /// The rules this engine will evaluate.
    pub fn rules(&self) -> &[Rule] {
        &self.rules.rules
    }
}

impl RuleEngine for RuleEngineImpl {
    fn evaluate(&mut self, point: &DataPoint) -> Result<Vec<RuleOutcome>> {
        // TODO: update retained state with `point`, evaluate each rule's
        // condition tree (Compare via Value comparisons, All/Any/Not), and map
        // fired actions to RuleOutcome values (SetTag / RaiseAlarm / EmitEvent),
        // optionally edge-triggered on transition into the true state.
        let _ = point;
        Err(not_implemented("evaluate"))
    }
}

/// Compare a tag's current [`Value`] against a literal using an [`Operator`].
///
/// Outline for the real implementation: numeric ops use [`Value::as_f64`];
/// `Eq`/`Ne` also support bool and string operands.
pub fn compare_values(_lhs: &Value, _op: Operator, _rhs: &Value) -> bool {
    // TODO: implement comparison semantics; see doc comment.
    false
}

fn serde_yaml_from_str(yaml: &str) -> Result<RuleSet> {
    serde_yaml::from_str(yaml).map_err(|e| StaircaseError::config(format!("invalid rule set: {e}")))
}

/// Uniform "not yet implemented" error for the blueprint surface.
fn not_implemented(op: &str) -> StaircaseError {
    StaircaseError::Other(anyhow::anyhow!(
        "staircase-rules::{op} is not implemented yet (blueprint)"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruleset_parses_from_yaml() {
        // serde_yaml represents externally-tagged enums with `!tag` syntax;
        // unit variants (operators, severity) serialize as plain strings.
        let yaml = r#"
rules:
  - id: fan_on
    condition: !compare
      tag: room_temp
      op: gt
      value: !float 28.0
    actions:
      - !set_tag
        device_id: hvac
        tag_name: fan
        value: !bool true
  - id: humidity_alarm
    condition: !compare
      tag: humidity
      op: gt
      value: !int 80
    actions:
      - !raise_alarm
        severity: warning
        message: "humidity too high"
"#;
        let set = RuleSet::from_yaml(yaml).expect("parse");
        assert_eq!(set.rules.len(), 2);
        assert_eq!(set.rules[0].id, "fan_on");
        assert!(matches!(set.rules[0].condition, Condition::Compare(_)));
        assert_eq!(set.rules[1].actions.len(), 1);
    }

    #[test]
    fn empty_ruleset_is_default() {
        let set = RuleSet::default();
        assert!(set.rules.is_empty());
        let engine = RuleEngineImpl::new(set);
        assert_eq!(engine.rules().len(), 0);
    }
}
