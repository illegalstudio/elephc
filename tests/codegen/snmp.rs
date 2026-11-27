//! Purpose:
//! End-to-end coverage for the first `ext/snmp` v1/v2c slice and its pure-Rust
//! `elephc-snmp` UDP bridge.
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext snmp`.
//!
//! Key details:
//! - A loopback UDP agent answers the fixture's GET and GETNEXT requests, so
//!   the test exercises real BER encoding, UDP transport and response parsing.
//! - The agent returns two walk entries and then an OID outside the root; the
//!   PHP walker stops on the prefix change.

use std::net::UdpSocket;
use std::thread;

use crate::support::*;

/// Encodes a BER length.
fn length(value: usize) -> Vec<u8> {
    if value < 0x80 {
        return vec![value as u8];
    }
    let mut bytes = Vec::new();
    let mut remaining = value;
    while remaining > 0 {
        bytes.push((remaining & 0xff) as u8);
        remaining >>= 8;
    }
    bytes.reverse();
    let mut out = vec![0x80 | bytes.len() as u8];
    out.extend(bytes);
    out
}

/// Wraps `body` in one TLV.
fn tlv(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend(length(body.len()));
    out.extend(body);
    out
}

/// Encodes a signed integer.
fn integer(value: i64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let mut start = 0;
    while start < 7 {
        let current = bytes[start];
        let next = bytes[start + 1];
        if (current == 0 && next & 0x80 == 0) || (current == 0xff && next & 0x80 != 0) {
            start += 1;
        } else {
            break;
        }
    }
    tlv(0x02, &bytes[start..])
}

/// Encodes an OID.
fn oid(value: &str) -> Vec<u8> {
    let arcs: Vec<u64> = value
        .trim_start_matches('.')
        .split('.')
        .map(|arc| arc.parse().unwrap())
        .collect();
    let mut body = Vec::new();
    let mut first = arcs[0] * 40 + arcs[1];
    let mut bytes = vec![(first & 0x7f) as u8];
    first >>= 7;
    while first > 0 {
        bytes.push(0x80 | (first & 0x7f) as u8);
        first >>= 7;
    }
    bytes.reverse();
    body.extend(bytes);
    for arc in &arcs[2..] {
        let mut bytes = vec![(arc & 0x7f) as u8];
        let mut value = arc >> 7;
        while value > 0 {
            bytes.push(0x80 | (value & 0x7f) as u8);
            value >>= 7;
        }
        bytes.reverse();
        body.extend(bytes);
    }
    tlv(0x06, &body)
}

/// Decodes a BER length, returning `(length, header bytes)`.
fn read_length(data: &[u8]) -> (usize, usize) {
    let first = data[0];
    if first & 0x80 == 0 {
        return (usize::from(first), 1);
    }
    let count = usize::from(first & 0x7f);
    let mut value = 0usize;
    for byte in &data[1..1 + count] {
        value = value << 8 | usize::from(*byte);
    }
    (value, 1 + count)
}

/// Reads one TLV, returning `(tag, body, next offset)`.
fn read_tlv(data: &[u8], offset: usize) -> (u8, Vec<u8>, usize) {
    let tag = data[offset];
    let (len, header) = read_length(&data[offset + 1..]);
    let start = offset + 1 + header;
    (tag, data[start..start + len].to_vec(), start + len)
}

/// Decodes an integer body.
fn read_integer(body: &[u8]) -> i64 {
    let mut value = if body[0] & 0x80 != 0 { -1i64 } else { 0 };
    for byte in body {
        value = value << 8 | i64::from(*byte);
    }
    value
}

/// Builds a response for one request.
fn response(request: &[u8]) -> Vec<u8> {
    let (_, message, _) = read_tlv(request, 0);
    let (_, version, offset) = read_tlv(&message, 0);
    let (_, community, offset) = read_tlv(&message, offset);
    let (pdu_tag, pdu, _) = read_tlv(&message, offset);
    let (_, request_id, offset) = read_tlv(&pdu, 0);
    let (_, _, offset) = read_tlv(&pdu, offset);
    let (_, _, offset) = read_tlv(&pdu, offset);
    let (_, list, _) = read_tlv(&pdu, offset);
    let (_, varbind, _) = read_tlv(&list, 0);
    let (_, request_oid, _) = read_tlv(&varbind, 0);
    let request_oid = decode_oid(&request_oid);

    let (reply_oid, value_tag, value) = if pdu_tag == 0xA0 {
        (".1.3.6.1.2.1.1.1.0".to_string(), 0x04, b"elephc snmp".to_vec())
    } else {
        match request_oid.as_str() {
            ".1.3.6.1.2.1.1" => (
                ".1.3.6.1.2.1.1.1.0".to_string(),
                0x04,
                b"one".to_vec(),
            ),
            ".1.3.6.1.2.1.1.1.0" => (
                ".1.3.6.1.2.1.1.2.0".to_string(),
                0x04,
                b"two".to_vec(),
            ),
            _ => (".1.3.6.1.3.0".to_string(), 0x05, Vec::new()),
        }
    };
    let varbind = tlv(0x30, &[oid(&reply_oid), tlv(value_tag, &value)].concat());
    let list = tlv(0x30, &varbind);
    let pdu = tlv(
        0xA2,
        &[
            integer(read_integer(&request_id)),
            integer(0),
            integer(0),
            list,
        ]
        .concat(),
    );
    tlv(
        0x30,
        &[
            integer(read_integer(&version)),
            tlv(0x04, &community),
            pdu,
        ]
        .concat(),
    )
}

/// Decodes an OID body into a leading-dot string.
fn decode_oid(body: &[u8]) -> String {
    let mut arcs = Vec::new();
    let mut value = 0u64;
    let mut started = false;
    for byte in body {
        started = true;
        value = value << 7 | u64::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            if arcs.is_empty() {
                let (first, second) = if value < 40 {
                    (0, value)
                } else if value < 80 {
                    (1, value - 40)
                } else {
                    (2, value - 80)
                };
                arcs.push(first);
                arcs.push(second);
            } else {
                arcs.push(value);
            }
            value = 0;
            started = false;
        }
    }
    assert!(!started);
    format!(
        ".{}",
        arcs.iter().map(u64::to_string).collect::<Vec<_>>().join(".")
    )
}

/// GET, GETNEXT, walk and the SNMP class round-trip through a loopback agent.
#[test]
fn test_snmp_v1_v2c_get_walk_and_class() {
    let server = UdpSocket::bind(("127.0.0.1", 0)).expect("bind SNMP fixture");
    let port = server.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        for _ in 0..6 {
            let (size, peer) = server.recv_from(&mut buffer).expect("SNMP request");
            let reply = response(&buffer[..size]);
            server.send_to(&reply, peer).expect("SNMP response");
        }
    });

    let source = r#"<?php
$host = "127.0.0.1:__PORT__";
echo snmp_get($host, "public", ".1.3.6.1.2.1.1.1.0", 1000000, 1), "\n";
echo snmp2_getnext($host, "public", ".1.3.6.1.2.1.1", 1000000, 1), "\n";
$walk = snmp2_walk($host, "public", ".1.3.6.1.2.1.1", 1000000, 1);
foreach ($walk as $oid => $value) {
    echo $oid, "=", $value, "\n";
}
$snmp = new SNMP(SNMP::VERSION_2C, $host, "public", 1000000, 1);
echo $snmp->get(".1.3.6.1.2.1.1.1.0"), "\n";
$snmp->close();
"#
    .replace("__PORT__", &port.to_string());

    let out = compile_and_run(&source);
    worker.join().expect("SNMP fixture thread");
    assert_eq!(
        out,
        "elephc snmp\none\n.1.3.6.1.2.1.1.1.0=one\n.1.3.6.1.2.1.1.2.0=two\nelephc snmp\n"
    );
}
