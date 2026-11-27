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

/// The PHP 8.6 `Snmp\*` enums, output setters and `SNMP` instance flags exist.
#[test]
fn test_snmp_php86_output_api() {
    let out = compile_and_run(
        r#"<?php
snmp_set_quick_print(true);
echo snmp_get_quick_print() ? "quick" : "no", "\n";
snmp_set_enum_print(true);
snmp_set_oid_output_format(\Snmp\OidOutput::Numeric);
snmp_set_oid_numeric_print(\Snmp\OidOutput::Numeric);
snmp_set_output_option(\Snmp\Output::HexText, true);
snmp_set_output_option(\Snmp\Output::NumericIndex, true);
snmp_set_string_output_format(\Snmp\StringOutput::Hex);
snmp_init_mib(null);
snmp_set_mib_option(\Snmp\Mib::AllowUnderscores, true);
snmp_set_valueretrieval(1);
echo snmp_get_valueretrieval(), "\n";

$snmp = new SNMP(SNMP::VERSION_1, "127.0.0.1:1", "public", 1000000, 0);
$snmp->numeric_index = true;
$snmp->print_hex_text = true;
var_dump($snmp->setOidOutputFormat(\Snmp\OidOutput::Numeric));
var_dump($snmp->setStringOutputFormat(\Snmp\StringOutput::Hex));
echo $snmp->getErrno(), ":", $snmp->getError() === "" ? "empty" : "bad", "\n";
$snmp->close();
"#,
    );
    assert_eq!(out, "quick\n1\nbool(true)\nbool(true)\n0:empty\n");
}

/// Extracts the version and community TLV bodies from a request.
fn request_header(request: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let (_, message, _) = read_tlv(request, 0);
    let (_, version, offset) = read_tlv(&message, 0);
    let (_, community, _) = read_tlv(&message, offset);
    (version, community)
}

/// Reads a request's PDU tag, request id and varbind OIDs.
fn request_details(request: &[u8]) -> (u8, i64, Vec<String>) {
    let (_, message, _) = read_tlv(request, 0);
    let (_, _, offset) = read_tlv(&message, 0);
    let (_, _, offset) = read_tlv(&message, offset);
    let (pdu_tag, pdu, _) = read_tlv(&message, offset);
    let (_, request_id, offset) = read_tlv(&pdu, 0);
    let (_, _, offset) = read_tlv(&pdu, offset);
    let (_, _, offset) = read_tlv(&pdu, offset);
    let (_, list, _) = read_tlv(&pdu, offset);
    let mut varbinds = Vec::new();
    let mut list_offset = 0;
    while list_offset < list.len() {
        let (_, varbind, next) = read_tlv(&list, list_offset);
        let (_, oid_body, _) = read_tlv(&varbind, 0);
        varbinds.push(decode_oid(&oid_body));
        list_offset = next;
    }
    (pdu_tag, read_integer(&request_id), varbinds)
}

/// Builds a GetResponse containing several varbinds.
fn multi_response(
    version: &[u8],
    community: &[u8],
    request_id: i64,
    varbinds: Vec<(String, u8, Vec<u8>)>,
) -> Vec<u8> {
    let mut list_body = Vec::new();
    for (oid_text, tag, value) in varbinds {
        list_body.extend(tlv(0x30, &[oid(&oid_text), tlv(tag, &value)].concat()));
    }
    let list = tlv(0x30, &list_body);
    let pdu = tlv(
        0xA2,
        &[
            integer(request_id),
            integer(0),
            integer(0),
            list,
        ]
        .concat(),
    );
    tlv(0x30, &[tlv(0x02, version), tlv(0x04, community), pdu].concat())
}

/// snmp2_set, SNMP::set and GETBULK-based walk round-trip through a loopback agent.
#[test]
fn test_snmp_set_and_getbulk_walk() {
    let server = UdpSocket::bind(("127.0.0.1", 0)).expect("bind SNMP set fixture");
    let port = server.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        for _ in 0..4 {
            let (size, peer) = server.recv_from(&mut buffer).expect("SNMP request");
            let request = &buffer[..size];
            let (version, community) = request_header(request);
            let (pdu_tag, request_id, varbinds) = request_details(request);
            let reply = if pdu_tag == 0xA3 {
                let oid_text = varbinds.first().cloned().unwrap_or_default();
                multi_response(
                    &version,
                    &community,
                    request_id,
                    vec![(oid_text, 0x04, b"ok".to_vec())],
                )
            } else {
                let current = varbinds.first().cloned().unwrap_or_default();
                let entries = if current == ".1.3.6.1.2.1.1" {
                    vec![
                        (".1.3.6.1.2.1.1.1.0".to_string(), 0x04, b"one".to_vec()),
                        (".1.3.6.1.2.1.1.2.0".to_string(), 0x04, b"two".to_vec()),
                    ]
                } else if current == ".1.3.6.1.2.1.1.2.0" {
                    vec![
                        (".1.3.6.1.2.1.1.3.0".to_string(), 0x04, b"three".to_vec()),
                        (".1.3.6.1.3.0".to_string(), 0x05, Vec::new()),
                    ]
                } else {
                    vec![(".1.3.6.1.3.0".to_string(), 0x05, Vec::new())]
                };
                multi_response(&version, &community, request_id, entries)
            };
            server.send_to(&reply, peer).expect("SNMP response");
        }
    });

    let source = r#"<?php
$host = "127.0.0.1:__PORT__";
var_dump(snmp2_set($host, "public", ".1.3.6.1.2.1.1.5.0", "s", "newname", 1000000, 1));
$snmp = new SNMP(SNMP::VERSION_2C, $host, "public", 1000000, 1);
var_dump($snmp->set(".1.3.6.1.2.1.1.5.0", "i", "42"));
$walk = $snmp->walk(".1.3.6.1.2.1.1", false, 2, 0);
foreach ($walk as $oid => $value) {
    echo $oid, "=", $value, "\n";
}
var_dump($snmp->info["hostname"] === $host);
echo $snmp->getErrno(), "\n";
$snmp->close();
"#
    .replace("__PORT__", &port.to_string());

    let out = compile_and_run(&source);
    worker.join().expect("SNMP set fixture thread");
    assert_eq!(
        out,
        "bool(true)\nbool(true)\n.1.3.6.1.2.1.1.1.0=one\n.1.3.6.1.2.1.1.2.0=two\n.1.3.6.1.2.1.1.3.0=three\nbool(true)\n0\n"
    );
}

/// A MIB file resolves symbolic OIDs and renders names/enums in walk keys/values.
#[test]
fn test_snmp_mib_resolution_and_rendering() {
    let server = UdpSocket::bind(("127.0.0.1", 0)).expect("bind SNMP MIB fixture");
    let port = server.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        for _ in 0..5 {
            let (size, peer) = server.recv_from(&mut buffer).expect("SNMP request");
            let request = &buffer[..size];
            let (version, community) = request_header(request);
            let (pdu_tag, request_id, varbinds) = request_details(request);
            let reply = if pdu_tag == 0xA0 {
                multi_response(
                    &version,
                    &community,
                    request_id,
                    vec![(
                        ".1.3.6.1.4.1.424242.1.0".to_string(),
                        0x04,
                        b"value".to_vec(),
                    )],
                )
            } else {
                let current = varbinds.first().cloned().unwrap_or_default();
                let entries = if current == ".1.3.6.1.4.1.424242" {
                    vec![
                        (
                            ".1.3.6.1.4.1.424242.1.0".to_string(),
                            0x04,
                            b"value".to_vec(),
                        ),
                        (".1.3.6.1.4.1.424242.2.0".to_string(), 0x02, vec![2]),
                    ]
                } else {
                    vec![(".1.3.6.1.3.0".to_string(), 0x05, Vec::new())]
                };
                multi_response(&version, &community, request_id, entries)
            };
            server.send_to(&reply, peer).expect("SNMP response");
        }
    });

    let source = r#"<?php
$mib = 'ELEPHC-MIB DEFINITIONS ::= BEGIN
elephc OBJECT IDENTIFIER ::= { enterprises 424242 }
elephcName OBJECT-TYPE
    SYNTAX DisplayString
    MAX-ACCESS read-only
    STATUS current
    ::= { elephc 1 }
elephcState OBJECT-TYPE
    SYNTAX INTEGER { up(1), down(2) }
    MAX-ACCESS read-only
    STATUS current
    ::= { elephc 2 }
END
';
file_put_contents("elephc-test.mib", $mib);
var_dump(snmp_read_mib("elephc-test.mib"));
$host = "127.0.0.1:__PORT__";
snmp_set_oid_output_format(\Snmp\OidOutput::Suffix);
echo snmp2_get($host, "public", "elephcName.0", 1000000, 1), "\n";
$walk = snmp2_walk($host, "public", "elephc", 1000000, 1);
foreach ($walk as $oid => $value) {
    echo $oid, "=", $value, "\n";
}
snmp_set_oid_output_format(\Snmp\OidOutput::Module);
snmp_set_enum_print(true);
$walk = snmp2_walk($host, "public", "elephc", 1000000, 1);
foreach ($walk as $oid => $value) {
    echo $oid, "=", $value, "\n";
}
"#
    .replace("__PORT__", &port.to_string());

    let out = compile_and_run(&source);
    worker.join().expect("SNMP MIB fixture thread");
    assert_eq!(
        out,
        "bool(true)\nvalue\nelephcName.0=value\nelephcState.0=down\nELEPHC-MIB::elephcName.0=value\nELEPHC-MIB::elephcState.0=2\n"
    );
}

/// The ext/snmp global constants and the VERSION_2c alias match PHP 8.6.
#[test]
fn test_snmp_constants() {
    let out = compile_and_run(
        r#"<?php
echo SNMP_OID_OUTPUT_SUFFIX, ":", SNMP_OID_OUTPUT_NONE, ":",
    SNMP_VALUE_OBJECT, ":", SNMP_COUNTER, ":", SNMP_TIMETICKS, ":",
    SNMP_UINTEGER, "\n";
echo SNMP::VERSION_2c, ":", SNMP::VERSION_2C, "\n";
"#,
    );
    assert_eq!(out, "1:6:2:66:67:71\n1:1\n");
}
