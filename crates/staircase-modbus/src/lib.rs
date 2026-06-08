//! `staircase-modbus` — Modbus TCP protocol driver for Staircase.
//!
//! Implements [`ProtocolDriver`](staircase_core::ProtocolDriver) over Modbus TCP.
//! It connects to a device, reads coils / discrete inputs / holding & input
//! registers per the tag configuration, and normalizes the results into
//! [`DataPoint`](staircase_core::DataPoint)s.
//!
//! ## Tag addressing
//!
//! Each tag's `address` selects a register table and an offset:
//!
//! - `holding:N` (or a bare number `N`) — holding register at 0-based offset `N`
//! - `input:N` — input register
//! - `coil:N` — coil (boolean)
//! - `discrete:N` — discrete input (boolean)
//!
//! The optional `data_type` hint controls how register words are decoded:
//! `u16` (default), `i16`, `u32`, `i32`, `f32`, `bool`. Multi-word types
//! (`u32`/`i32`/`f32`) read two consecutive registers, most-significant word
//! first.

use std::net::SocketAddr;

use async_trait::async_trait;
use staircase_core::config::DeviceConfig;
use staircase_core::error::{Result, StaircaseError};
use staircase_core::model::{DataPoint, TagValue, Value};
use staircase_core::traits::ProtocolDriver;
use tokio::sync::Mutex;
use tokio_modbus::client::tcp;
use tokio_modbus::prelude::{Client, Reader, Writer};
use tokio_modbus::Slave;
use tracing::warn;

/// Which Modbus data table a register address refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    /// Read/write 16-bit registers (function 0x03).
    Holding,
    /// Read-only 16-bit registers (function 0x04).
    Input,
    /// Read/write single-bit values (function 0x01).
    Coil,
    /// Read-only single-bit values (function 0x02).
    Discrete,
}

/// How to interpret the register word(s) read for a tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    /// Single unsigned 16-bit register.
    U16,
    /// Single signed 16-bit register.
    I16,
    /// Two registers as an unsigned 32-bit integer (MSW first).
    U32,
    /// Two registers as a signed 32-bit integer (MSW first).
    I32,
    /// Two registers as an IEEE-754 float (MSW first).
    F32,
    /// A single coil/discrete bit, or a register treated as boolean.
    Bool,
}

impl DataType {
    fn parse(hint: Option<&str>) -> Result<Self> {
        match hint.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
            None | Some("") | Some("u16") | Some("uint16") | Some("word") => Ok(DataType::U16),
            Some("i16") | Some("int16") => Ok(DataType::I16),
            Some("u32") | Some("uint32") | Some("dword") => Ok(DataType::U32),
            Some("i32") | Some("int32") => Ok(DataType::I32),
            Some("f32") | Some("float") | Some("real") => Ok(DataType::F32),
            Some("bool") | Some("boolean") | Some("bit") => Ok(DataType::Bool),
            Some(other) => Err(StaircaseError::config(format!(
                "unsupported modbus data_type '{other}'"
            ))),
        }
    }

    /// Number of 16-bit registers this type spans.
    fn word_count(self) -> u16 {
        match self {
            DataType::U32 | DataType::I32 | DataType::F32 => 2,
            _ => 1,
        }
    }
}

/// A parsed tag: where to read it and how to decode it.
#[derive(Debug, Clone)]
struct ModbusTag {
    name: String,
    table: Table,
    address: u16,
    data_type: DataType,
}

/// Parse a tag `address` into a [`Table`] and 0-based offset.
pub fn parse_address(address: &str) -> Result<(Table, u16)> {
    let address = address.trim();
    let (table, offset) = match address.split_once(':') {
        Some((prefix, rest)) => {
            let table = match prefix.trim().to_ascii_lowercase().as_str() {
                "holding" | "hr" | "h" => Table::Holding,
                "input" | "ir" | "i" => Table::Input,
                "coil" | "co" | "c" => Table::Coil,
                "discrete" | "di" | "d" => Table::Discrete,
                other => {
                    return Err(StaircaseError::config(format!(
                        "unknown modbus table '{other}' in address '{address}'"
                    )))
                }
            };
            (table, rest.trim())
        }
        None => (Table::Holding, address),
    };

    let offset: u16 = offset.parse().map_err(|_| {
        StaircaseError::config(format!("invalid modbus register offset in '{address}'"))
    })?;
    Ok((table, offset))
}

/// Decode raw register words into a [`Value`] according to `data_type`.
pub fn decode_registers(data_type: DataType, words: &[u16]) -> Result<Value> {
    let need = data_type.word_count() as usize;
    if words.len() < need {
        return Err(StaircaseError::protocol(format!(
            "modbus read returned {} word(s), expected {need}",
            words.len()
        )));
    }
    let value = match data_type {
        DataType::U16 => Value::Int(words[0] as i64),
        DataType::I16 => Value::Int(words[0] as i16 as i64),
        DataType::Bool => Value::Bool(words[0] != 0),
        DataType::U32 => {
            let v = ((words[0] as u32) << 16) | words[1] as u32;
            Value::Int(v as i64)
        }
        DataType::I32 => {
            let v = ((words[0] as u32) << 16) | words[1] as u32;
            Value::Int(v as i32 as i64)
        }
        DataType::F32 => {
            let v = ((words[0] as u32) << 16) | words[1] as u32;
            Value::Float(f32::from_bits(v) as f64)
        }
    };
    Ok(value)
}

/// Modbus TCP protocol driver.
pub struct ModbusDriver {
    source: String,
    device_id: String,
    socket_addr: SocketAddr,
    unit_id: u8,
    tags: Vec<ModbusTag>,
    ctx: Mutex<Option<tokio_modbus::client::Context>>,
}

impl ModbusDriver {
    /// Build a driver from a [`DeviceConfig`].
    ///
    /// `address` must be `host:port` (port defaults to `502` if omitted). The
    /// optional `unit_id` setting selects the Modbus unit/slave id (default `1`).
    pub fn from_config(source: impl Into<String>, cfg: &DeviceConfig) -> Result<Self> {
        let socket_addr = resolve_addr(&cfg.address)?;
        let unit_id = cfg
            .settings
            .get("unit_id")
            .and_then(|v| v.as_u64())
            .map(|v| v as u8)
            .unwrap_or(1);

        let mut tags = Vec::with_capacity(cfg.tags.len());
        for tag in &cfg.tags {
            let (table, address) = parse_address(&tag.address)?;
            let data_type = DataType::parse(tag.data_type.as_deref())?;
            tags.push(ModbusTag {
                name: tag.name.clone(),
                table,
                address,
                data_type,
            });
        }

        Ok(Self {
            source: source.into(),
            device_id: cfg.name.clone(),
            socket_addr,
            unit_id,
            tags,
            ctx: Mutex::new(None),
        })
    }

    async fn read_tag(&self, tag: &ModbusTag) -> Result<Value> {
        let mut guard = self.ctx.lock().await;
        let ctx = guard
            .as_mut()
            .ok_or_else(|| StaircaseError::connection("modbus driver is not connected"))?;

        match tag.table {
            Table::Coil => {
                let bits = unwrap_modbus(ctx.read_coils(tag.address, 1).await)?;
                Ok(Value::Bool(bits.first().copied().unwrap_or(false)))
            }
            Table::Discrete => {
                let bits = unwrap_modbus(ctx.read_discrete_inputs(tag.address, 1).await)?;
                Ok(Value::Bool(bits.first().copied().unwrap_or(false)))
            }
            Table::Holding => {
                let words =
                    unwrap_modbus(ctx.read_holding_registers(tag.address, tag.data_type.word_count()).await)?;
                decode_registers(tag.data_type, &words)
            }
            Table::Input => {
                let words =
                    unwrap_modbus(ctx.read_input_registers(tag.address, tag.data_type.word_count()).await)?;
                decode_registers(tag.data_type, &words)
            }
        }
    }
}

#[async_trait]
impl ProtocolDriver for ModbusDriver {
    fn protocol(&self) -> &str {
        "modbus"
    }

    async fn connect(&mut self) -> Result<()> {
        let ctx = tcp::connect_slave(self.socket_addr, Slave(self.unit_id))
            .await
            .map_err(|e| {
                StaircaseError::connection(format!(
                    "modbus connect to {} failed: {e}",
                    self.socket_addr
                ))
            })?;
        *self.ctx.lock().await = Some(ctx);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(mut ctx) = self.ctx.lock().await.take() {
            let _ = ctx.disconnect().await;
        }
        Ok(())
    }

    async fn poll(&mut self) -> Result<Vec<DataPoint>> {
        let tags = self.tags.clone();
        let mut points = Vec::with_capacity(tags.len());
        for tag in &tags {
            match self.read_tag(tag).await {
                Ok(value) => points.push(DataPoint::new(
                    self.source.clone(),
                    "modbus",
                    self.device_id.clone(),
                    tag.name.clone(),
                    value,
                )),
                Err(e) => warn!(
                    device = %self.device_id,
                    tag = %tag.name,
                    error = %e,
                    "modbus tag read failed; skipping"
                ),
            }
        }
        Ok(points)
    }

    async fn write_tag(&mut self, tag_name: &str, value: TagValue) -> Result<()> {
        let tag = self
            .tags
            .iter()
            .find(|t| t.name == tag_name)
            .cloned()
            .ok_or_else(|| {
                StaircaseError::config(format!("unknown modbus tag '{tag_name}'"))
            })?;
        let mut guard = self.ctx.lock().await;
        let ctx = guard
            .as_mut()
            .ok_or_else(|| StaircaseError::connection("modbus driver is not connected"))?;

        match tag.table {
            Table::Coil => {
                let bit = value.value.as_bool().ok_or_else(|| {
                    StaircaseError::protocol("coil write requires a boolean value")
                })?;
                unwrap_modbus(ctx.write_single_coil(tag.address, bit).await)
            }
            Table::Holding => {
                let word = value.value.as_i64().ok_or_else(|| {
                    StaircaseError::protocol("holding register write requires a numeric value")
                })? as u16;
                unwrap_modbus(ctx.write_single_register(tag.address, word).await)
            }
            Table::Input | Table::Discrete => Err(StaircaseError::protocol(
                "cannot write to a read-only modbus table",
            )),
        }
    }
}

/// Flatten tokio-modbus's nested `Result<Result<T, Exception>, io::Error>`.
fn unwrap_modbus<T>(
    result: std::result::Result<
        std::result::Result<T, tokio_modbus::ExceptionCode>,
        tokio_modbus::Error,
    >,
) -> Result<T> {
    match result {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(exc)) => Err(StaircaseError::protocol(format!("modbus exception: {exc}"))),
        Err(e) => Err(StaircaseError::connection(format!("modbus transport error: {e}"))),
    }
}

fn resolve_addr(address: &str) -> Result<SocketAddr> {
    let address = address.trim();
    let with_port = if address.contains(':') {
        address.to_string()
    } else {
        format!("{address}:502")
    };
    with_port
        .parse()
        .map_err(|_| StaircaseError::config(format!("invalid modbus address '{address}'")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_table_prefixes() {
        assert_eq!(parse_address("holding:10").unwrap(), (Table::Holding, 10));
        assert_eq!(parse_address("input:3").unwrap(), (Table::Input, 3));
        assert_eq!(parse_address("coil:0").unwrap(), (Table::Coil, 0));
        assert_eq!(parse_address("discrete:7").unwrap(), (Table::Discrete, 7));
        assert_eq!(parse_address("42").unwrap(), (Table::Holding, 42));
    }

    #[test]
    fn rejects_bad_addresses() {
        assert!(parse_address("bogus:1").is_err());
        assert!(parse_address("holding:notanumber").is_err());
    }

    #[test]
    fn data_type_parsing() {
        assert_eq!(DataType::parse(None).unwrap(), DataType::U16);
        assert_eq!(DataType::parse(Some("f32")).unwrap(), DataType::F32);
        assert_eq!(DataType::parse(Some("BOOL")).unwrap(), DataType::Bool);
        assert!(DataType::parse(Some("u128")).is_err());
        assert_eq!(DataType::F32.word_count(), 2);
        assert_eq!(DataType::U16.word_count(), 1);
    }

    #[test]
    fn decodes_register_words() {
        assert_eq!(decode_registers(DataType::U16, &[0x1234]).unwrap(), Value::Int(0x1234));
        assert_eq!(decode_registers(DataType::I16, &[0xFFFF]).unwrap(), Value::Int(-1));
        assert_eq!(decode_registers(DataType::Bool, &[0]).unwrap(), Value::Bool(false));
        assert_eq!(decode_registers(DataType::Bool, &[1]).unwrap(), Value::Bool(true));
        assert_eq!(
            decode_registers(DataType::U32, &[0x0001, 0x0000]).unwrap(),
            Value::Int(0x0001_0000)
        );
        assert_eq!(
            decode_registers(DataType::I32, &[0xFFFF, 0xFFFF]).unwrap(),
            Value::Int(-1)
        );
        // 25.0f32 == 0x41C80000
        match decode_registers(DataType::F32, &[0x41C8, 0x0000]).unwrap() {
            Value::Float(f) => assert!((f - 25.0).abs() < 1e-6),
            other => panic!("expected float, got {other:?}"),
        }
    }

    #[test]
    fn decode_rejects_short_reads() {
        assert!(decode_registers(DataType::F32, &[0x0001]).is_err());
    }

    #[test]
    fn resolves_addresses() {
        assert_eq!(
            resolve_addr("127.0.0.1").unwrap(),
            "127.0.0.1:502".parse().unwrap()
        );
        assert_eq!(
            resolve_addr("127.0.0.1:1502").unwrap(),
            "127.0.0.1:1502".parse().unwrap()
        );
        assert!(resolve_addr("not an address").is_err());
    }
}
