//! Purpose:
//! SNMP v1/v2c request/response client over UDP.
//!
//! Called from:
//! - `crate::` C ABI session/request wrappers.
//!
//! Key details:
//! - GetRequest, GetNextRequest, SetRequest and GetBulkRequest PDUs are
//!   emitted; GetResponse and its varbind value tags are decoded into typed
//!   values.
//! - Hosts accept `host` or `host:port`, defaulting to UDP/161.
//! - Timeouts are milliseconds and retries are extra attempts, matching the
//!   PHP wrapper's conversion from ext/snmp's microsecond timeout.

use std::net::{ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use crate::ber::{self, Reader};

static NEXT_REQUEST_ID: AtomicI32 = AtomicI32::new(1);

/// One decoded SNMP value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Value {
    /// INTEGER, Counter32, Gauge32, TimeTicks, or UInteger32.
    Int(i64),
    /// Counter64 value.
    Counter64(u64),
    /// OCTET STRING or Opaque bytes.
    Bytes(Vec<u8>),
    /// OBJECT IDENTIFIER.
    Oid(String),
    /// NULL.
    Null,
    /// IpAddress in dotted form.
    Ip(String),
    /// noSuchObject.
    NoSuchObject,
    /// noSuchInstance.
    NoSuchInstance,
    /// endOfMibView.
    EndOfMibView,
}

/// One decoded varbind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Varbind {
    /// Dotted OID with a leading dot.
    pub(crate) oid: String,
    /// ASN.1 value tag from the response.
    pub(crate) tag: u8,
    /// Decoded value.
    pub(crate) value: Value,
}

/// One decoded SNMP response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Response {
    /// SNMP error-status.
    pub(crate) error_status: i64,
    /// SNMP error-index.
    pub(crate) error_index: i64,
    /// Response varbinds.
    pub(crate) varbinds: Vec<Varbind>,
}

/// SNMP PDU tags used by the client.
pub(crate) const GET_REQUEST: i64 = 0xa0;
/// GetNextRequest PDU tag.
pub(crate) const GET_NEXT_REQUEST: i64 = 0xa1;
/// SetRequest PDU tag.
pub(crate) const SET_REQUEST: i64 = 0xa3;
/// GetBulkRequest PDU tag.
pub(crate) const GET_BULK_REQUEST: i64 = 0xa5;

/// Parses `host` or `host:port`.
pub(crate) fn endpoint(host: &str) -> Result<(String, u16), String> {
    if host.starts_with('[') {
        if let Some(end) = host.find(']') {
            let name = &host[1..end];
            let port = host[end + 1..]
                .strip_prefix(':')
                .and_then(|port| port.parse::<u16>().ok())
                .unwrap_or(161);
            return Ok((name.to_string(), port));
        }
    }
    if let Some((name, port)) = host.rsplit_once(':') {
        if !name.is_empty() && !port.is_empty() && port.chars().all(|ch| ch.is_ascii_digit()) {
            if let Ok(port) = port.parse::<u16>() {
                return Ok((name.to_string(), port));
            }
        }
    }
    Ok((host.to_string(), 161))
}

/// Builds one GetRequest/GetNextRequest message.
fn build_request(
    version: i64,
    community: &[u8],
    oid: &str,
    pdu_tag: i64,
    request_id: i64,
) -> Result<Vec<u8>, String> {
    let oid = ber::encode_oid(oid).ok_or_else(|| "invalid object identifier".to_string())?;
    let varbind = ber::tlv(0x30, &[oid, ber::tlv(0x05, &[])].concat());
    let list = ber::tlv(0x30, &varbind);
    let pdu = ber::tlv(
        u8::try_from(pdu_tag).map_err(|_| "invalid PDU".to_string())?,
        &[
            ber::encode_integer(request_id),
            ber::encode_integer(0),
            ber::encode_integer(0),
            list,
        ]
        .concat(),
    );
    Ok(ber::tlv(
        0x30,
        &[
            ber::encode_integer(version),
            ber::tlv(0x04, community),
            pdu,
        ]
        .concat(),
    ))
}

/// Encodes an unsigned integer body with minimal big-endian bytes.
fn unsigned_body(value: u64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let start = bytes.iter().position(|byte| *byte != 0).unwrap_or(7);
    bytes[start..].to_vec()
}

/// Decodes a hex string used by the `x` and `b` SET types.
pub(crate) fn decode_hex(value: &[u8]) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(value).map_err(|_| "invalid SNMP hex value".to_string())?;
    if text.len() % 2 != 0 {
        return Err("invalid SNMP hex value".to_string());
    }
    let digits = |byte: u8| -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    };
    let bytes = text.as_bytes();
    let mut output = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let high = digits(pair[0]).ok_or_else(|| "invalid SNMP hex value".to_string())?;
        let low = digits(pair[1]).ok_or_else(|| "invalid SNMP hex value".to_string())?;
        output.push(high << 4 | low);
    }
    Ok(output)
}

/// Encodes a SET value from ext/snmp's type character and textual value.
pub(crate) fn encode_set_value(value_type: &str, value: &[u8]) -> Result<Vec<u8>, String> {
    let text = || std::str::from_utf8(value).map_err(|_| "invalid SNMP set value".to_string());
    match value_type {
        "i" => {
            let parsed: i64 = text()?
                .trim()
                .parse()
                .map_err(|_| "invalid SNMP integer".to_string())?;
            Ok(ber::encode_integer(parsed))
        }
        "u" => {
            let parsed: u64 = text()?
                .trim()
                .parse()
                .map_err(|_| "invalid SNMP unsigned integer".to_string())?;
            Ok(ber::tlv(0x42, &unsigned_body(parsed)))
        }
        "t" => {
            let parsed: u64 = text()?
                .trim()
                .parse()
                .map_err(|_| "invalid SNMP timeticks".to_string())?;
            Ok(ber::tlv(0x43, &unsigned_body(parsed)))
        }
        "s" | "d" => Ok(ber::tlv(0x04, value)),
        "x" | "b" => Ok(ber::tlv(0x04, &decode_hex(value)?)),
        "n" => Ok(ber::tlv(0x05, &[])),
        "o" => ber::encode_oid(text()?).ok_or_else(|| "invalid object identifier".to_string()),
        "a" => {
            let text = text()?.trim();
            let parts: Vec<&str> = text.split('.').collect();
            if parts.len() != 4 {
                return Err("invalid SNMP IP address".to_string());
            }
            let mut bytes = [0u8; 4];
            for (index, part) in parts.iter().enumerate() {
                bytes[index] = part
                    .parse::<u8>()
                    .map_err(|_| "invalid SNMP IP address".to_string())?;
            }
            Ok(ber::tlv(0x40, &bytes))
        }
        _ => Err(format!("unsupported SNMP set type: {value_type}")),
    }
}

/// Sends one request message and waits for its response.
fn exchange(
    host: &str,
    timeout_ms: u64,
    retries: i64,
    message: &[u8],
    request_id: i64,
) -> Result<Response, String> {
    let (host, port) = endpoint(host)?;
    let address = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|error| format!("resolve SNMP host: {error}"))?
        .next()
        .ok_or_else(|| "SNMP host has no address".to_string())?;
    let socket = UdpSocket::bind(("0.0.0.0", 0))
        .map_err(|error| format!("bind SNMP socket: {error}"))?;
    socket
        .connect(address)
        .map_err(|error| format!("connect SNMP socket: {error}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(timeout_ms.max(1))))
        .map_err(|error| format!("set SNMP timeout: {error}"))?;
    let retries = if retries < 0 { 5 } else { retries.max(0) };
    let attempts = u64::try_from(retries).unwrap_or(5).saturating_add(1);
    let mut buffer = [0u8; 65_535];
    let mut last_error = "SNMP timeout".to_string();
    for _ in 0..attempts {
        if let Err(error) = socket.send(message) {
            last_error = format!("send SNMP request: {error}");
            continue;
        }
        match socket.recv(&mut buffer) {
            Ok(length) => match parse_response(&buffer[..length], request_id) {
                Ok(response) => return Ok(response),
                Err(error) => last_error = error,
            },
            Err(error) => last_error = format!("receive SNMP response: {error}"),
        }
    }
    Err(last_error)
}

/// Builds one varbind with a pre-encoded value TLV.
fn varbind(oid: &str, value: &[u8]) -> Result<Vec<u8>, String> {
    let oid = ber::encode_oid(oid).ok_or_else(|| "invalid object identifier".to_string())?;
    Ok(ber::tlv(0x30, &[oid, value.to_vec()].concat()))
}

/// Builds a varbind list from one encoded varbind.
fn varbind_list(varbind: &[u8]) -> Vec<u8> {
    ber::tlv(0x30, varbind)
}

/// Builds a message around one PDU.
fn message(version: i64, community: &[u8], pdu: &[u8]) -> Vec<u8> {
    ber::tlv(
        0x30,
        &[
            ber::encode_integer(version),
            ber::tlv(0x04, community),
            pdu.to_vec(),
        ]
        .concat(),
    )
}

/// Decodes an SNMP response message.
fn parse_response(data: &[u8], expected_id: i64) -> Result<Response, String> {
    let mut message = Reader::new(data);
    let (tag, body) = message
        .read_tlv()
        .ok_or_else(|| "truncated SNMP message".to_string())?;
    if tag != 0x30 {
        return Err("SNMP message is not a sequence".to_string());
    }
    let mut message = Reader::new(body);
    let (tag, version) = message
        .read_tlv()
        .ok_or_else(|| "missing SNMP version".to_string())?;
    if tag != 0x02 || ber::decode_integer(version).is_none() {
        return Err("invalid SNMP version".to_string());
    }
    let (tag, _community) = message
        .read_tlv()
        .ok_or_else(|| "missing SNMP community".to_string())?;
    if tag != 0x04 {
        return Err("invalid SNMP community".to_string());
    }
    let (tag, pdu) = message
        .read_tlv()
        .ok_or_else(|| "missing SNMP PDU".to_string())?;
    if tag != 0xa2 {
        return Err("SNMP response PDU is not GetResponse".to_string());
    }
    let mut pdu = Reader::new(pdu);
    let (tag, request_id) = pdu
        .read_tlv()
        .ok_or_else(|| "missing SNMP request id".to_string())?;
    if tag != 0x02 {
        return Err("invalid SNMP request id".to_string());
    }
    if ber::decode_integer(request_id) != Some(expected_id) {
        return Err("SNMP request id mismatch".to_string());
    }
    let (tag, error_status) = pdu
        .read_tlv()
        .ok_or_else(|| "missing SNMP error status".to_string())?;
    if tag != 0x02 {
        return Err("invalid SNMP error status".to_string());
    }
    let (tag, error_index) = pdu
        .read_tlv()
        .ok_or_else(|| "missing SNMP error index".to_string())?;
    if tag != 0x02 {
        return Err("invalid SNMP error index".to_string());
    }
    let (tag, list) = pdu
        .read_tlv()
        .ok_or_else(|| "missing SNMP varbind list".to_string())?;
    if tag != 0x30 {
        return Err("invalid SNMP varbind list".to_string());
    }
    let mut list = Reader::new(list);
    let mut varbinds = Vec::new();
    while !list.remaining().is_empty() {
        let (tag, varbind) = list
            .read_tlv()
            .ok_or_else(|| "truncated SNMP varbind".to_string())?;
        if tag != 0x30 {
            return Err("invalid SNMP varbind".to_string());
        }
        let mut varbind = Reader::new(varbind);
        let (tag, oid) = varbind
            .read_tlv()
            .ok_or_else(|| "missing SNMP varbind OID".to_string())?;
        if tag != 0x06 {
            return Err("invalid SNMP varbind OID".to_string());
        }
        let oid = ber::decode_oid(oid).ok_or_else(|| "invalid SNMP OID".to_string())?;
        let (tag, body) = varbind
            .read_tlv()
            .ok_or_else(|| "missing SNMP varbind value".to_string())?;
        let value = decode_value(tag, body)?;
        varbinds.push(Varbind { oid, tag, value });
    }
    Ok(Response {
        error_status: ber::decode_integer(error_status).unwrap_or(0),
        error_index: ber::decode_integer(error_index).unwrap_or(0),
        varbinds,
    })
}

/// Decodes one SNMP varbind value.
fn decode_value(tag: u8, body: &[u8]) -> Result<Value, String> {
    match tag {
        0x02 => ber::decode_integer(body)
            .map(Value::Int)
            .ok_or_else(|| "invalid SNMP integer".to_string()),
        0x04 | 0x44 => Ok(Value::Bytes(body.to_vec())),
        0x05 => Ok(Value::Null),
        0x06 => ber::decode_oid(body)
            .map(Value::Oid)
            .ok_or_else(|| "invalid SNMP OID value".to_string()),
        0x40 if body.len() == 4 => Ok(Value::Ip(format!(
            "{}.{}.{}.{}",
            body[0], body[1], body[2], body[3]
        ))),
        0x41 | 0x42 | 0x43 => ber::decode_integer(body)
            .map(Value::Int)
            .ok_or_else(|| "invalid SNMP counter".to_string()),
        0x46 => {
            let body = if body.len() == 9 && body[0] == 0 {
                &body[1..]
            } else {
                body
            };
            if body.is_empty() || body.len() > 8 {
                return Ok(Value::Bytes(body.to_vec()));
            }
            let mut value = 0u64;
            for byte in body {
                value = value << 8 | u64::from(*byte);
            }
            Ok(Value::Counter64(value))
        }
        0x80 => Ok(Value::NoSuchObject),
        0x81 => Ok(Value::NoSuchInstance),
        0x82 => Ok(Value::EndOfMibView),
        _ => Ok(Value::Bytes(body.to_vec())),
    }
}

/// Sends one GetRequest/GetNextRequest and decodes its response.
pub(crate) fn request(
    version: i64,
    host: &str,
    community: &[u8],
    oid: &str,
    pdu_tag: i64,
    timeout_ms: u64,
    retries: i64,
) -> Result<Response, String> {
    if version != 0 && version != 1 {
        return Err("only SNMP v1 and v2c are supported".to_string());
    }
    if pdu_tag != GET_REQUEST && pdu_tag != GET_NEXT_REQUEST {
        return Err("unsupported SNMP PDU".to_string());
    }
    let request_id = i64::from(NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed));
    let message = build_request(version, community, oid, pdu_tag, request_id)?;
    exchange(host, timeout_ms, retries, &message, request_id)
}

/// Sends one SetRequest and decodes its response.
pub(crate) fn request_set(
    version: i64,
    host: &str,
    community: &[u8],
    oid: &str,
    value_type: &str,
    value: &[u8],
    timeout_ms: u64,
    retries: i64,
) -> Result<Response, String> {
    if version != 0 && version != 1 {
        return Err("only SNMP v1 and v2c are supported".to_string());
    }
    let request_id = i64::from(NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed));
    let varbind = varbind(oid, &encode_set_value(value_type, value)?)?;
    let list = varbind_list(&varbind);
    let pdu = ber::tlv(
        u8::try_from(SET_REQUEST).map_err(|_| "invalid PDU".to_string())?,
        &[
            ber::encode_integer(request_id),
            ber::encode_integer(0),
            ber::encode_integer(0),
            list,
        ]
        .concat(),
    );
    let message = message(version, community, &pdu);
    exchange(host, timeout_ms, retries, &message, request_id)
}

/// Sends one GetBulkRequest and decodes its response.
pub(crate) fn request_bulk(
    version: i64,
    host: &str,
    community: &[u8],
    oid: &str,
    non_repeaters: i64,
    max_repetitions: i64,
    timeout_ms: u64,
    retries: i64,
) -> Result<Response, String> {
    if version != 1 {
        return Err("GETBULK requires SNMP v2c or v3".to_string());
    }
    let request_id = i64::from(NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed));
    let varbind = varbind(oid, &ber::tlv(0x05, &[]))?;
    let list = varbind_list(&varbind);
    let pdu = ber::tlv(
        u8::try_from(GET_BULK_REQUEST).map_err(|_| "invalid PDU".to_string())?,
        &[
            ber::encode_integer(request_id),
            ber::encode_integer(non_repeaters.max(0)),
            ber::encode_integer(max_repetitions.max(1)),
            list,
        ]
        .concat(),
    );
    let message = message(version, community, &pdu);
    exchange(host, timeout_ms, retries, &message, request_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket;
    use std::thread;

    /// Builds a GetResponse for one request, echoing its OID and request id.
    fn response_for(request: &[u8], value: &[u8]) -> Vec<u8> {
        let mut message = Reader::new(request);
        let (_, body) = message.read_tlv().unwrap();
        let mut message = Reader::new(body);
        let (_, version) = message.read_tlv().unwrap();
        let (_, community) = message.read_tlv().unwrap();
        let (_, pdu) = message.read_tlv().unwrap();
        let mut pdu = Reader::new(pdu);
        let (_, request_id) = pdu.read_tlv().unwrap();
        let (_, _error_status) = pdu.read_tlv().unwrap();
        let (_, _error_index) = pdu.read_tlv().unwrap();
        let (_, list) = pdu.read_tlv().unwrap();
        let mut list = Reader::new(list);
        let (_, varbind) = list.read_tlv().unwrap();
        let mut varbind = Reader::new(varbind);
        let (_, oid) = varbind.read_tlv().unwrap();
        let oid = ber::decode_oid(oid).unwrap();
        let oid = ber::encode_oid(&oid).unwrap();
        let varbind = ber::tlv(0x30, &[oid, ber::tlv(0x04, value)].concat());
        let list = ber::tlv(0x30, &varbind);
        let response = ber::tlv(
            0xa2,
            &[
                ber::encode_integer(ber::decode_integer(request_id).unwrap()),
                ber::encode_integer(0),
                ber::encode_integer(0),
                list,
            ]
            .concat(),
        );
        ber::tlv(
            0x30,
            &[
                ber::encode_integer(ber::decode_integer(version).unwrap()),
                ber::tlv(0x04, community),
                response,
            ]
            .concat(),
        )
    }

    /// A loopback agent answers a v2c GET with an octet string.
    #[test]
    fn udp_get_round_trip() {
        let server = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        let port = server.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let mut buffer = [0u8; 2048];
            let (length, peer) = server.recv_from(&mut buffer).unwrap();
            let response = response_for(&buffer[..length], b"elephc");
            server.send_to(&response, peer).unwrap();
        });
        let response = request(
            1,
            &format!("127.0.0.1:{port}"),
            b"public",
            ".1.3.6.1.2.1.1.1.0",
            GET_REQUEST,
            1_000,
            0,
        )
        .unwrap();
        handle.join().unwrap();
        assert_eq!(response.error_status, 0);
        assert_eq!(response.varbinds.len(), 1);
        assert_eq!(response.varbinds[0].oid, ".1.3.6.1.2.1.1.1.0");
        assert_eq!(response.varbinds[0].value, Value::Bytes(b"elephc".to_vec()));
    }

    /// Hosts without a port use SNMP's UDP/161.
    #[test]
    fn endpoint_defaults_to_161() {
        assert_eq!(endpoint("127.0.0.1").unwrap(), ("127.0.0.1".to_string(), 161));
        assert_eq!(
            endpoint("127.0.0.1:1161").unwrap(),
            ("127.0.0.1".to_string(), 1161)
        );
    }
}
