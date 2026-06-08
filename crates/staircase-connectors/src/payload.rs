//! Protocol-independent payload mapping shared by connectors.
//!
//! These functions are **real** (not stubbed): they convert normalized
//! [`DataPoint`]s into common wire formats. Connectors call them and then send
//! the result over their transport.

use staircase_core::model::{DataPoint, Value};

/// Serialize a single data point to a JSON object string.
///
/// Used by the REST connector (and any JSON-oriented target).
pub fn to_json(point: &DataPoint) -> staircase_core::error::Result<String> {
    serde_json::to_string(point)
        .map_err(|e| staircase_core::error::StaircaseError::Serialization(e.to_string()))
}

/// Serialize a batch of data points to a JSON array string.
pub fn to_json_batch(points: &[DataPoint]) -> staircase_core::error::Result<String> {
    serde_json::to_string(points)
        .map_err(|e| staircase_core::error::StaircaseError::Serialization(e.to_string()))
}

/// Render a single data point as an InfluxDB line-protocol line.
///
/// Format: `measurement,tagset fieldset timestamp`. The measurement is the tag
/// name; `source`/`protocol`/`device_id` become tags; the sample becomes a
/// `value` field; the timestamp is nanoseconds since the Unix epoch.
pub fn to_line_protocol(point: &DataPoint) -> String {
    let measurement = escape_tag(&point.tag_name);
    let tags = format!(
        "source={},protocol={},device={}",
        escape_tag(&point.source),
        escape_tag(&point.protocol),
        escape_tag(&point.device_id),
    );
    let field = format!("value={}", line_field(&point.value));
    let ts = point.timestamp.timestamp_nanos_opt().unwrap_or_default();
    format!("{measurement},{tags} {field} {ts}")
}

/// Render a batch as newline-separated line-protocol lines.
pub fn to_line_protocol_batch(points: &[DataPoint]) -> String {
    points
        .iter()
        .map(to_line_protocol)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format a [`Value`] as an InfluxDB field value (integers get the `i` suffix,
/// strings are quoted).
fn line_field(value: &Value) -> String {
    match value {
        Value::Int(i) => format!("{i}i"),
        Value::Float(f) => format!("{f}"),
        Value::Bool(b) => format!("{b}"),
        Value::String(s) => format!("{s:?}"),
        Value::Null => "\"\"".to_string(),
        Value::Bytes(b) => format!("{:?}", String::from_utf8_lossy(b).into_owned()),
    }
}

/// Escape characters that are special in an InfluxDB tag key/value.
fn escape_tag(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(',', "\\,")
        .replace(' ', "\\ ")
        .replace('=', "\\=")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_compat::dt;
    use staircase_core::model::Value;

    mod chrono_compat {
        use staircase_core::model::Timestamp;
        // Fixed timestamp helper so line-protocol output is deterministic.
        pub fn dt() -> Timestamp {
            use std::time::{Duration, UNIX_EPOCH};
            let sys = UNIX_EPOCH + Duration::from_secs(1_000);
            Timestamp::from(sys)
        }
    }

    fn point(value: Value) -> DataPoint {
        DataPoint::new("gw", "modbus", "dev1", "room_temp", value).with_timestamp(dt())
    }

    #[test]
    fn json_roundtrips_value() {
        let json = to_json(&point(Value::Float(21.5))).unwrap();
        assert!(json.contains("\"tag_name\":\"room_temp\""));
        assert!(json.contains("21.5"));
    }

    #[test]
    fn json_batch_is_array() {
        let json = to_json_batch(&[point(Value::Int(1)), point(Value::Int(2))]).unwrap();
        assert!(json.starts_with('['));
        assert!(json.ends_with(']'));
    }

    #[test]
    fn line_protocol_shapes_correctly() {
        let line = to_line_protocol(&point(Value::Float(21.5)));
        assert!(line.starts_with("room_temp,"));
        assert!(line.contains("source=gw"));
        assert!(line.contains("protocol=modbus"));
        assert!(line.contains("device=dev1"));
        assert!(line.contains("value=21.5"));
        assert!(line.ends_with("1000000000000")); // 1000s in ns
    }

    #[test]
    fn line_protocol_int_gets_suffix() {
        let line = to_line_protocol(&point(Value::Int(42)));
        assert!(line.contains("value=42i"));
    }

    #[test]
    fn tag_escaping() {
        assert_eq!(escape_tag("a b,c=d"), "a\\ b\\,c\\=d");
    }
}
