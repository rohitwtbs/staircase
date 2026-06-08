//! `staircase-bacnet` — BACnet/IP protocol driver for Staircase.
//!
//! Implements [`ProtocolDriver`](staircase_core::ProtocolDriver) over BACnet/IP
//! (UDP, default port `47808`). This is a focused, self-contained implementation
//! of the BACnet read path: it encodes a Confirmed `ReadProperty` request for an
//! object's `present-value`, sends it as a unicast NPDU, and decodes the
//! Complex-ACK into a normalized [`DataPoint`](staircase_core::DataPoint).
//!
//! Full BACnet (segmentation, COV subscriptions, who-is/i-am discovery, the
//! complete object/property catalog) is intentionally out of scope; this covers
//! the present-value reads needed for periodic polling.
//!
//! ## Tag addressing
//!
//! Each tag's `address` is `object-type:instance`, e.g. `analog-input:0`,
//! `ai:0`, `analog-value:1`, `binary-input:2`. Common abbreviations are
//! accepted.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use staircase_core::config::DeviceConfig;
use staircase_core::error::{Result, StaircaseError};
use staircase_core::model::{DataPoint, Value};
use staircase_core::traits::ProtocolDriver;
use tokio::net::UdpSocket;
use tracing::warn;

const BVLC_TYPE: u8 = 0x81;
const BVLC_ORIGINAL_UNICAST: u8 = 0x0a;
const NPDU_VERSION: u8 = 0x01;
const SERVICE_READ_PROPERTY: u8 = 12;
const PROP_PRESENT_VALUE: u8 = 85;

/// Map a BACnet object-type name (or abbreviation) to its numeric code.
pub fn object_type_code(name: &str) -> Result<u16> {
    let code = match name.trim().to_ascii_lowercase().as_str() {
        "analog-input" | "analog_input" | "ai" => 0,
        "analog-output" | "analog_output" | "ao" => 1,
        "analog-value" | "analog_value" | "av" => 2,
        "binary-input" | "binary_input" | "bi" => 3,
        "binary-output" | "binary_output" | "bo" => 4,
        "binary-value" | "binary_value" | "bv" => 5,
        "multi-state-input" | "multistate-input" | "msi" => 13,
        "multi-state-output" | "multistate-output" | "mso" => 14,
        "multi-state-value" | "multistate-value" | "msv" => 19,
        other => {
            return Err(StaircaseError::config(format!(
                "unknown bacnet object type '{other}'"
            )))
        }
    };
    Ok(code)
}

/// Parse a tag `address` of the form `object-type:instance`.
pub fn parse_object_id(address: &str) -> Result<(u16, u32)> {
    let (type_part, instance_part) = address.trim().split_once(':').ok_or_else(|| {
        StaircaseError::config(format!(
            "bacnet address '{address}' must be 'object-type:instance'"
        ))
    })?;
    let object_type = object_type_code(type_part)?;
    let instance: u32 = instance_part.trim().parse().map_err(|_| {
        StaircaseError::config(format!("invalid bacnet instance in '{address}'"))
    })?;
    if instance > 0x003F_FFFF {
        return Err(StaircaseError::config(format!(
            "bacnet instance {instance} exceeds 22-bit maximum"
        )));
    }
    Ok((object_type, instance))
}

/// Encode a Confirmed `ReadProperty` request for an object's present-value.
pub fn encode_read_property(invoke_id: u8, object_type: u16, instance: u32) -> Vec<u8> {
    // APDU
    let mut apdu = Vec::with_capacity(16);
    apdu.push(0x00); // Confirmed-Request, no segmentation
    apdu.push(0x05); // max segments unspecified / max APDU 1476
    apdu.push(invoke_id);
    apdu.push(SERVICE_READ_PROPERTY);

    // Context tag 0: object identifier (4 bytes)
    apdu.push(0x0c);
    let object_id = ((object_type as u32) << 22) | (instance & 0x003F_FFFF);
    apdu.extend_from_slice(&object_id.to_be_bytes());

    // Context tag 1: property identifier (present-value)
    apdu.push(0x19);
    apdu.push(PROP_PRESENT_VALUE);

    // NPDU: version + control (expecting reply)
    let mut npdu = Vec::with_capacity(2 + apdu.len());
    npdu.push(NPDU_VERSION);
    npdu.push(0x04); // expecting-reply
    npdu.extend_from_slice(&apdu);

    // BVLC: type, function, total length
    let total_len = (4 + npdu.len()) as u16;
    let mut frame = Vec::with_capacity(total_len as usize);
    frame.push(BVLC_TYPE);
    frame.push(BVLC_ORIGINAL_UNICAST);
    frame.extend_from_slice(&total_len.to_be_bytes());
    frame.extend_from_slice(&npdu);
    frame
}

/// Decode a BACnet/IP `ReadProperty` Complex-ACK, validating that it answers the
/// request we sent before extracting the present-value.
///
/// The frame is parsed field-by-field in order — BVLC, NPDU (skipping any
/// routing fields), APDU header, then the ReadProperty-ACK's object-id,
/// property-id and (optional) array-index context tags — rather than scanning
/// for marker bytes. The returned invoke-id, object identifier and property
/// identifier must match `expected_invoke_id` and `expected_object_id`, which
/// guards against stale or mismatched UDP datagrams.
pub fn decode_read_property_ack(
    frame: &[u8],
    expected_invoke_id: u8,
    expected_object_id: u32,
) -> Result<Value> {
    if frame.len() < 6 || frame[0] != BVLC_TYPE {
        return Err(StaircaseError::protocol("not a BACnet/IP frame"));
    }

    // Skip BVLC header (4 bytes).
    let mut idx = 4;

    // NPDU: version + control, plus optional routing fields.
    if frame.len() < idx + 2 {
        return Err(StaircaseError::protocol("truncated NPDU"));
    }
    let control = frame[idx + 1];
    idx += 2;
    if control & 0x80 != 0 {
        return Err(StaircaseError::protocol("unexpected network-layer message"));
    }
    if control & 0x20 != 0 {
        // Destination present: DNET(2) + DLEN(1) + DADR(DLEN)
        idx += 2;
        let dlen = *frame.get(idx).ok_or_else(trunc)? as usize;
        idx += 1 + dlen;
    }
    if control & 0x08 != 0 {
        // Source present: SNET(2) + SLEN(1) + SADR(SLEN)
        idx += 2;
        let slen = *frame.get(idx).ok_or_else(trunc)? as usize;
        idx += 1 + slen;
    }
    if control & 0x20 != 0 {
        idx += 1; // hop count
    }

    // APDU header.
    let pdu_type = *frame.get(idx).ok_or_else(trunc)?;
    match pdu_type & 0xf0 {
        0x30 => {} // Complex-ACK
        0x50 => return Err(StaircaseError::protocol("bacnet error response")),
        0x60 => return Err(StaircaseError::protocol("bacnet reject response")),
        0x70 => return Err(StaircaseError::protocol("bacnet abort response")),
        other => {
            return Err(StaircaseError::protocol(format!(
                "unexpected bacnet PDU type 0x{other:02x}"
            )))
        }
    }
    idx += 1; // pdu_type
    let invoke_id = *frame.get(idx).ok_or_else(trunc)?;
    idx += 1;
    if invoke_id != expected_invoke_id {
        return Err(StaircaseError::protocol(format!(
            "bacnet invoke-id mismatch: got {invoke_id}, expected {expected_invoke_id}"
        )));
    }
    let service = *frame.get(idx).ok_or_else(trunc)?;
    idx += 1;
    if service != SERVICE_READ_PROPERTY {
        return Err(StaircaseError::protocol(format!(
            "unexpected service ack {service}"
        )));
    }

    // Context tag 0: object identifier (4 bytes).
    let (object_bytes, next) = read_context_value(frame, idx, 0)?;
    idx = next;
    let object_id = u32::from_be_bytes(
        object_bytes
            .try_into()
            .map_err(|_| StaircaseError::protocol("bad bacnet object-id length"))?,
    );
    if object_id != expected_object_id {
        return Err(StaircaseError::protocol(format!(
            "bacnet object-id mismatch: got 0x{object_id:08x}, expected 0x{expected_object_id:08x}"
        )));
    }

    // Context tag 1: property identifier.
    let (property_bytes, next) = read_context_value(frame, idx, 1)?;
    idx = next;
    let property_id = be_uint(property_bytes);
    if property_id != PROP_PRESENT_VALUE as u64 {
        return Err(StaircaseError::protocol(format!(
            "bacnet property mismatch: got {property_id}, expected present-value"
        )));
    }

    // Optional context tag 2: array index — skip if present.
    if let Some(&b) = frame.get(idx) {
        if b & 0x08 != 0 && (b >> 4) == 2 && (b & 0x07) != 6 && (b & 0x07) != 7 {
            let (_, next) = read_context_value(frame, idx, 2)?;
            idx = next;
        }
    }

    // Opening tag 3 wraps the value.
    let open = *frame.get(idx).ok_or_else(trunc)?;
    if open != 0x3e {
        return Err(StaircaseError::protocol("expected bacnet value opening tag"));
    }
    idx += 1;

    let (value, _) = decode_app_value(frame, idx)?;
    Ok(value)
}

/// Read a primitive context tag with tag number `expected`, returning its data
/// bytes and the index just past them.
fn read_context_value(frame: &[u8], idx: usize, expected: u8) -> Result<(&[u8], usize)> {
    let tag = *frame.get(idx).ok_or_else(trunc)?;
    let number = tag >> 4;
    let is_context = tag & 0x08 != 0;
    let lvt = tag & 0x07;
    if !is_context || number != expected {
        return Err(StaircaseError::protocol(format!(
            "expected bacnet context tag {expected}"
        )));
    }
    let mut cursor = idx + 1;
    let len = if lvt == 5 {
        let l = *frame.get(cursor).ok_or_else(trunc)? as usize;
        cursor += 1;
        l
    } else {
        lvt as usize
    };
    let bytes = frame
        .get(cursor..cursor + len)
        .ok_or_else(|| StaircaseError::protocol("truncated bacnet context value"))?;
    Ok((bytes, cursor + len))
}

/// Decode one application-tagged primitive value starting at `idx`.
fn decode_app_value(frame: &[u8], idx: usize) -> Result<(Value, usize)> {
    let tag = *frame.get(idx).ok_or_else(trunc)?;
    let tag_number = tag >> 4;
    let len_field = (tag & 0x07) as usize;
    let mut cursor = idx + 1;

    // Boolean encodes its value in the length/value field; no data bytes.
    if tag_number == 1 {
        return Ok((Value::Bool(len_field != 0), cursor));
    }

    let len = if len_field == 5 {
        let l = *frame.get(cursor).ok_or_else(trunc)? as usize;
        cursor += 1;
        l
    } else {
        len_field
    };

    let bytes = frame
        .get(cursor..cursor + len)
        .ok_or_else(|| StaircaseError::protocol("truncated bacnet value"))?;
    let end = cursor + len;

    let value = match tag_number {
        0 => Value::Null,                                   // Null
        2 => Value::Int(be_uint(bytes) as i64),             // Unsigned
        3 => Value::Int(be_sint(bytes)),                    // Signed
        4 => {
            // Real (IEEE-754 single)
            let arr: [u8; 4] = bytes
                .try_into()
                .map_err(|_| StaircaseError::protocol("bad bacnet real length"))?;
            Value::Float(f32::from_be_bytes(arr) as f64)
        }
        5 => {
            // Double
            let arr: [u8; 8] = bytes
                .try_into()
                .map_err(|_| StaircaseError::protocol("bad bacnet double length"))?;
            Value::Float(f64::from_be_bytes(arr))
        }
        9 => Value::Int(be_uint(bytes) as i64), // Enumerated
        other => {
            return Err(StaircaseError::protocol(format!(
                "unsupported bacnet value tag {other}"
            )))
        }
    };
    Ok((value, end))
}

fn be_uint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0u64, |acc, &b| (acc << 8) | b as u64)
}

fn be_sint(bytes: &[u8]) -> i64 {
    if bytes.is_empty() {
        return 0;
    }
    let mut value = if bytes[0] & 0x80 != 0 { -1i64 } else { 0i64 };
    for &b in bytes {
        value = (value << 8) | b as i64;
    }
    value
}

fn trunc() -> StaircaseError {
    StaircaseError::protocol("truncated bacnet frame")
}

/// A parsed BACnet tag: which object's present-value to read.
#[derive(Debug, Clone)]
struct BacnetTag {
    name: String,
    object_type: u16,
    instance: u32,
}

/// BACnet/IP protocol driver.
pub struct BacnetDriver {
    source: String,
    device_id: String,
    socket_addr: SocketAddr,
    timeout: Duration,
    tags: Vec<BacnetTag>,
    invoke_id: AtomicU8,
    socket: Option<UdpSocket>,
}

impl BacnetDriver {
    /// Build a driver from a [`DeviceConfig`].
    ///
    /// `address` is `host:port` (port defaults to `47808`). The optional
    /// `timeout_ms` setting bounds each read (default `2000`).
    pub fn from_config(source: impl Into<String>, cfg: &DeviceConfig) -> Result<Self> {
        let socket_addr = resolve_addr(&cfg.address)?;
        let timeout = cfg
            .settings
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .map(Duration::from_millis)
            .unwrap_or_else(|| Duration::from_millis(2000));

        let mut tags = Vec::with_capacity(cfg.tags.len());
        for tag in &cfg.tags {
            let (object_type, instance) = parse_object_id(&tag.address)?;
            tags.push(BacnetTag {
                name: tag.name.clone(),
                object_type,
                instance,
            });
        }

        Ok(Self {
            source: source.into(),
            device_id: cfg.name.clone(),
            socket_addr,
            timeout,
            tags,
            invoke_id: AtomicU8::new(0),
            socket: None,
        })
    }

    async fn read_tag(&self, tag: &BacnetTag) -> Result<Value> {
        let socket = self
            .socket
            .as_ref()
            .ok_or_else(|| StaircaseError::connection("bacnet driver is not connected"))?;

        let invoke_id = self.invoke_id.fetch_add(1, Ordering::Relaxed);
        let request = encode_read_property(invoke_id, tag.object_type, tag.instance);
        socket
            .send(&request)
            .await
            .map_err(|e| StaircaseError::connection(format!("bacnet send failed: {e}")))?;

        let mut buf = vec![0u8; 1500];
        let n = tokio::time::timeout(self.timeout, socket.recv(&mut buf))
            .await
            .map_err(|_| StaircaseError::timeout("bacnet read timed out"))?
            .map_err(|e| StaircaseError::connection(format!("bacnet recv failed: {e}")))?;

        let expected_object_id = ((tag.object_type as u32) << 22) | (tag.instance & 0x003F_FFFF);
        decode_read_property_ack(&buf[..n], invoke_id, expected_object_id)
    }
}

#[async_trait]
impl ProtocolDriver for BacnetDriver {
    fn protocol(&self) -> &str {
        "bacnet"
    }

    async fn connect(&mut self) -> Result<()> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| StaircaseError::connection(format!("bacnet bind failed: {e}")))?;
        socket.connect(self.socket_addr).await.map_err(|e| {
            StaircaseError::connection(format!(
                "bacnet connect to {} failed: {e}",
                self.socket_addr
            ))
        })?;
        self.socket = Some(socket);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.socket = None;
        Ok(())
    }

    async fn poll(&mut self) -> Result<Vec<DataPoint>> {
        let tags = self.tags.clone();
        let mut points = Vec::with_capacity(tags.len());
        for tag in &tags {
            match self.read_tag(tag).await {
                Ok(value) => points.push(DataPoint::new(
                    self.source.clone(),
                    "bacnet",
                    self.device_id.clone(),
                    tag.name.clone(),
                    value,
                )),
                Err(e) => warn!(
                    device = %self.device_id,
                    tag = %tag.name,
                    error = %e,
                    "bacnet tag read failed; skipping"
                ),
            }
        }
        Ok(points)
    }
}

fn resolve_addr(address: &str) -> Result<SocketAddr> {
    let address = address.trim();
    let with_port = if address.contains(':') {
        address.to_string()
    } else {
        format!("{address}:47808")
    };
    with_port
        .parse()
        .map_err(|_| StaircaseError::config(format!("invalid bacnet address '{address}'")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_object_ids() {
        assert_eq!(parse_object_id("analog-input:0").unwrap(), (0, 0));
        assert_eq!(parse_object_id("ai:5").unwrap(), (0, 5));
        assert_eq!(parse_object_id("av:1").unwrap(), (2, 1));
        assert_eq!(parse_object_id("binary-value:3").unwrap(), (5, 3));
        assert!(parse_object_id("nope:1").is_err());
        assert!(parse_object_id("ai:notanumber").is_err());
        assert!(parse_object_id("ai").is_err());
    }

    #[test]
    fn encodes_read_property_request() {
        let frame = encode_read_property(1, 0, 5);
        // BVLC header
        assert_eq!(frame[0], 0x81);
        assert_eq!(frame[1], 0x0a);
        assert_eq!(u16::from_be_bytes([frame[2], frame[3]]) as usize, frame.len());
        // NPDU
        assert_eq!(frame[4], 0x01);
        assert_eq!(frame[5], 0x04);
        // APDU
        assert_eq!(frame[6], 0x00);
        assert_eq!(frame[7], 0x05);
        assert_eq!(frame[8], 0x01); // invoke id
        assert_eq!(frame[9], 0x0c); // ReadProperty
        // object id context tag
        assert_eq!(frame[10], 0x0c);
        let object_id = u32::from_be_bytes([frame[11], frame[12], frame[13], frame[14]]);
        assert_eq!(object_id, 5);
        // property id context tag
        assert_eq!(frame[15], 0x19);
        assert_eq!(frame[16], 85);
    }

    /// Build a minimal Complex-ACK frame wrapping a single application value.
    fn complex_ack(value_bytes: &[u8]) -> Vec<u8> {
        let mut apdu = vec![
            0x30, // Complex-ACK
            0x01, // invoke id
            SERVICE_READ_PROPERTY,
            0x0c, 0x00, 0x00, 0x00, 0x05, // object id (ai:5)
            0x19, 85, // property id present-value
            0x3e, // opening tag 3
        ];
        apdu.extend_from_slice(value_bytes);
        apdu.push(0x3f); // closing tag 3

        let mut npdu = vec![NPDU_VERSION, 0x00];
        npdu.extend_from_slice(&apdu);

        let total_len = (4 + npdu.len()) as u16;
        let mut frame = vec![BVLC_TYPE, BVLC_ORIGINAL_UNICAST];
        frame.extend_from_slice(&total_len.to_be_bytes());
        frame.extend_from_slice(&npdu);
        frame
    }

    #[test]
    fn decodes_real_value() {
        // 72.5f32 == 0x42910000
        let frame = complex_ack(&[0x44, 0x42, 0x91, 0x00, 0x00]);
        match decode_read_property_ack(&frame, 1, 5).unwrap() {
            Value::Float(f) => assert!((f - 72.5).abs() < 1e-6),
            other => panic!("expected float, got {other:?}"),
        }
    }

    #[test]
    fn decodes_unsigned_value() {
        // Unsigned, length 2, value 0x0123
        let frame = complex_ack(&[0x22, 0x01, 0x23]);
        assert_eq!(decode_read_property_ack(&frame, 1, 5).unwrap(), Value::Int(0x0123));
    }

    #[test]
    fn decodes_enumerated_value() {
        // Enumerated (tag 9), length 1, value 1 (e.g. binary active)
        let frame = complex_ack(&[0x91, 0x01]);
        assert_eq!(decode_read_property_ack(&frame, 1, 5).unwrap(), Value::Int(1));
    }

    #[test]
    fn decodes_boolean_value() {
        // Boolean (tag 1), value 1 -> tag byte 0x11
        let frame = complex_ack(&[0x11]);
        assert_eq!(decode_read_property_ack(&frame, 1, 5).unwrap(), Value::Bool(true));
    }

    #[test]
    fn rejects_error_pdu() {
        let mut npdu = vec![NPDU_VERSION, 0x00];
        npdu.extend_from_slice(&[0x50, 0x01, SERVICE_READ_PROPERTY]); // Error-PDU
        let total_len = (4 + npdu.len()) as u16;
        let mut frame = vec![BVLC_TYPE, BVLC_ORIGINAL_UNICAST];
        frame.extend_from_slice(&total_len.to_be_bytes());
        frame.extend_from_slice(&npdu);
        assert!(decode_read_property_ack(&frame, 1, 5).is_err());
    }

    #[test]
    fn rejects_mismatched_invoke_id_and_object() {
        let frame = complex_ack(&[0x44, 0x42, 0x91, 0x00, 0x00]);
        // Wrong invoke id.
        assert!(decode_read_property_ack(&frame, 99, 5).is_err());
        // Wrong object id.
        assert!(decode_read_property_ack(&frame, 1, 6).is_err());
    }

    #[test]
    fn decodes_signed_negative() {
        // Signed (tag 3), length 1, value 0xFF == -1
        let frame = complex_ack(&[0x31, 0xFF]);
        assert_eq!(decode_read_property_ack(&frame, 1, 5).unwrap(), Value::Int(-1));
    }

    #[test]
    fn resolves_addresses() {
        assert_eq!(
            resolve_addr("10.0.0.5").unwrap(),
            "10.0.0.5:47808".parse().unwrap()
        );
        assert_eq!(
            resolve_addr("10.0.0.5:47809").unwrap(),
            "10.0.0.5:47809".parse().unwrap()
        );
    }
}
