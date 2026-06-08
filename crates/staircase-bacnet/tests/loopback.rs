//! End-to-end test of [`BacnetDriver`] against a local UDP fixture "device".
//!
//! A UDP socket stands in for a BACnet device: it receives the driver's
//! ReadProperty request, echoes back a Complex-ACK carrying a Real present-value
//! (using the request's own invoke-id so correlation succeeds), and the driver
//! polls and normalizes it into a [`DataPoint`].

use std::collections::HashMap;

use staircase_bacnet::BacnetDriver;
use staircase_core::config::{DeviceConfig, TagConfig};
use staircase_core::model::Value;
use staircase_core::traits::ProtocolDriver;
use tokio::net::UdpSocket;

const BVLC_TYPE: u8 = 0x81;
const BVLC_ORIGINAL_UNICAST: u8 = 0x0a;
const SERVICE_READ_PROPERTY: u8 = 12;

/// Build a Complex-ACK for object-id 5 (ai:5), present-value, with a Real value.
fn complex_ack(invoke_id: u8, real: f32) -> Vec<u8> {
    let mut apdu = vec![0x30, invoke_id, SERVICE_READ_PROPERTY];
    apdu.extend_from_slice(&[0x0c, 0x00, 0x00, 0x00, 0x05]); // object id ai:5
    apdu.extend_from_slice(&[0x19, 85]); // property present-value
    apdu.push(0x3e); // opening tag 3
    apdu.push(0x44); // Real, length 4
    apdu.extend_from_slice(&real.to_be_bytes());
    apdu.push(0x3f); // closing tag 3

    let mut npdu = vec![0x01, 0x00];
    npdu.extend_from_slice(&apdu);

    let total_len = (4 + npdu.len()) as u16;
    let mut frame = vec![BVLC_TYPE, BVLC_ORIGINAL_UNICAST];
    frame.extend_from_slice(&total_len.to_be_bytes());
    frame.extend_from_slice(&npdu);
    frame
}

#[tokio::test]
async fn polls_present_value_from_fixture_device() {
    // Fixture "device" listening on an ephemeral UDP port.
    let device = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let device_addr = device.local_addr().unwrap();

    tokio::spawn(async move {
        let mut buf = vec![0u8; 1500];
        let (n, peer) = device.recv_from(&mut buf).await.unwrap();
        // The request's invoke-id sits at offset 8 (BVLC[4] + NPDU[2] + APDU header[2]).
        assert!(n >= 9, "request too short");
        let invoke_id = buf[8];
        let reply = complex_ack(invoke_id, 72.5);
        device.send_to(&reply, peer).await.unwrap();
    });

    let cfg = DeviceConfig {
        name: "ahu-1".to_string(),
        protocol: "bacnet".to_string(),
        address: device_addr.to_string(),
        poll_interval: 1,
        tags: vec![TagConfig {
            name: "supply-temp".to_string(),
            address: "ai:5".to_string(),
            data_type: None,
        }],
        settings: HashMap::new(),
    };

    let mut driver = BacnetDriver::from_config("gateway", &cfg).unwrap();
    driver.connect().await.unwrap();

    let points = driver.poll().await.unwrap();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].protocol, "bacnet");
    assert_eq!(points[0].device_id, "ahu-1");
    assert_eq!(points[0].tag_name, "supply-temp");
    match points[0].value {
        Value::Float(f) => assert!((f - 72.5).abs() < 1e-6),
        ref other => panic!("expected float, got {other:?}"),
    }
}
