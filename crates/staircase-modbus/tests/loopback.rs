//! End-to-end test of [`ModbusDriver`] against a minimal in-process Modbus TCP
//! server.
//!
//! The server speaks just enough of the Modbus TCP (MBAP) framing to answer a
//! single `read holding registers` request with a fixed register value, so the
//! driver's connect → poll → decode → [`DataPoint`] path is exercised for real.

use std::collections::HashMap;

use staircase_core::config::{DeviceConfig, TagConfig};
use staircase_core::model::Value;
use staircase_core::traits::ProtocolDriver;
use staircase_modbus::ModbusDriver;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn polls_holding_register_from_fixture_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        // MBAP header (7) + function (1) + start (2) + qty (2) = 12 bytes.
        let mut req = [0u8; 12];
        socket.read_exact(&mut req).await.unwrap();

        let transaction = [req[0], req[1]];
        assert_eq!(req[7], 0x03, "expected read-holding-registers function");

        // Respond with one register = 0x1234.
        let mut resp = Vec::new();
        resp.extend_from_slice(&transaction); // transaction id
        resp.extend_from_slice(&[0x00, 0x00]); // protocol id
        resp.extend_from_slice(&[0x00, 0x05]); // length: unit+func+bytecount+2 data
        resp.push(req[6]); // unit id (echo)
        resp.push(0x03); // function
        resp.push(0x02); // byte count
        resp.extend_from_slice(&[0x12, 0x34]); // register value
        socket.write_all(&resp).await.unwrap();
        socket.flush().await.unwrap();
    });

    let cfg = DeviceConfig {
        name: "plc-1".to_string(),
        protocol: "modbus".to_string(),
        address: addr.to_string(),
        poll_interval: 1,
        tags: vec![TagConfig {
            name: "setpoint".to_string(),
            address: "holding:0".to_string(),
            data_type: Some("u16".to_string()),
        }],
        settings: HashMap::new(),
    };

    let mut driver = ModbusDriver::from_config("gateway", &cfg).unwrap();
    driver.connect().await.unwrap();

    let points = driver.poll().await.unwrap();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].protocol, "modbus");
    assert_eq!(points[0].device_id, "plc-1");
    assert_eq!(points[0].tag_name, "setpoint");
    assert_eq!(points[0].value, Value::Int(0x1234));
}
