//! Purpose:
//! Exports profiled request slices as OpenTelemetry spans over OTLP/HTTP.
//!
//! Called from:
//! - `crate::monitor::run_stitch()` when `--otlp <endpoint>` is passed.
//!
//! Key details:
//! - elephc already speaks **W3C Trace Context**: a `--web --instrument` slice
//!   carries the trace id, span id and parent it was told, so it belongs to the
//!   caller's trace whether or not anything is exported. What was missing is the
//!   other direction — the spans themselves never reached a backend, so an
//!   elephc service appeared in someone else's trace as a hole. This closes that.
//! - Traces are the **stable** OTel signal. The Profiles signal entered public
//!   alpha in 2026 and the SIG advises against depending on it, so profiles are
//!   deliberately NOT exported here: `--pprof` already writes a pprof file, and
//!   OTLP Profiles round-trips losslessly with pprof, so a Collector's `pprof`
//!   receiver ingests elephc profiles today without this crate implementing an
//!   alpha wire format that is still moving.
//! - Plain HTTP/1.1 to the endpoint, `application/x-protobuf`. That is the
//!   normal OTLP deployment — an agent or sidecar on localhost:4318. A remote or
//!   authenticated collector belongs behind one, which keeps a TLS stack and a
//!   credential store out of the compiler binary.

use std::io::{Read as _, Write as _};

/// One exported span: a profiled request slice.
pub(crate) struct OtlpSpan {
    /// Becomes the `service.name` resource attribute spans are grouped by.
    pub service: String,
    /// Lowercase hex, as carried on the wire by `traceparent`.
    pub trace_id: String,
    pub span_id: String,
    /// Empty at the root of a trace.
    pub parent_span_id: String,
    pub name: String,
    pub start_unix_nano: u64,
    pub end_unix_nano: u64,
    /// Span attributes; integers only, which is all a profile has to say.
    pub attributes: Vec<(String, i64)>,
    /// String attributes, e.g. `http.route`.
    pub string_attributes: Vec<(String, String)>,
}

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
}

fn put_bytes(out: &mut Vec<u8>, field: u64, payload: &[u8]) {
    put_varint(out, (field << 3) | 2);
    put_varint(out, payload.len() as u64);
    out.extend_from_slice(payload);
}

fn put_varint_field(out: &mut Vec<u8>, field: u64, value: u64) {
    if value == 0 {
        return; // proto3 default — omitted
    }
    put_varint(out, field << 3);
    put_varint(out, value);
}

/// Wire type 1. OTel timestamps are `fixed64`, not varint: a varint would encode
/// a nanosecond epoch in ten bytes and, worse, a reader expecting fixed64 would
/// mis-frame every field after it.
fn put_fixed64(out: &mut Vec<u8>, field: u64, value: u64) {
    if value == 0 {
        return;
    }
    put_varint(out, (field << 3) | 1);
    out.extend_from_slice(&value.to_le_bytes());
}

/// Decodes lowercase hex into raw bytes. OTel carries ids as bytes; `traceparent`
/// carries them as hex, and sending the hex *text* would produce a 32-byte trace
/// id that no backend correlates with anything.
fn hex_bytes(hex: &str) -> Vec<u8> {
    let bytes = hex.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i + 1 < bytes.len() {
        let pair = std::str::from_utf8(&bytes[i..i + 2]).ok();
        match pair.and_then(|p| u8::from_str_radix(p, 16).ok()) {
            Some(byte) => out.push(byte),
            None => return Vec::new(),
        }
        i += 2;
    }
    out
}

/// `KeyValue { key, AnyValue }` with an int or string value.
fn key_value_int(key: &str, value: i64) -> Vec<u8> {
    let mut any = Vec::new();
    // AnyValue.int_value = 3
    put_varint(&mut any, 3 << 3);
    put_varint(&mut any, value as u64);
    let mut kv = Vec::new();
    put_bytes(&mut kv, 1, key.as_bytes());
    put_bytes(&mut kv, 2, &any);
    kv
}

fn key_value_str(key: &str, value: &str) -> Vec<u8> {
    let mut any = Vec::new();
    // AnyValue.string_value = 1
    put_bytes(&mut any, 1, value.as_bytes());
    let mut kv = Vec::new();
    put_bytes(&mut kv, 1, key.as_bytes());
    put_bytes(&mut kv, 2, &any);
    kv
}

fn encode_span(span: &OtlpSpan) -> Vec<u8> {
    let mut out = Vec::new();
    put_bytes(&mut out, 1, &hex_bytes(&span.trace_id));
    put_bytes(&mut out, 2, &hex_bytes(&span.span_id));
    if !span.parent_span_id.is_empty() {
        put_bytes(&mut out, 4, &hex_bytes(&span.parent_span_id));
    }
    put_bytes(&mut out, 5, span.name.as_bytes());
    // SpanKind::SERVER = 2. A profiled slice is always work done answering a
    // request, never a client call we made.
    put_varint_field(&mut out, 6, 2);
    put_fixed64(&mut out, 7, span.start_unix_nano);
    put_fixed64(&mut out, 8, span.end_unix_nano);
    for (key, value) in &span.string_attributes {
        put_bytes(&mut out, 9, &key_value_str(key, value));
    }
    for (key, value) in &span.attributes {
        put_bytes(&mut out, 9, &key_value_int(key, *value));
    }
    out
}

/// Encodes an `ExportTraceServiceRequest`, one `ResourceSpans` per service.
pub(crate) fn encode_traces(spans: &[OtlpSpan]) -> Vec<u8> {
    use std::collections::BTreeMap;
    let mut by_service: BTreeMap<&str, Vec<&OtlpSpan>> = BTreeMap::new();
    for span in spans {
        by_service
            .entry(span.service.as_str())
            .or_default()
            .push(span);
    }

    let mut request = Vec::new();
    for (service, members) in by_service {
        let mut resource = Vec::new();
        put_bytes(&mut resource, 1, &key_value_str("service.name", service));

        let mut scope = Vec::new();
        let mut scope_msg = Vec::new();
        put_bytes(&mut scope_msg, 1, b"elephc");
        put_bytes(&mut scope_msg, 2, env!("CARGO_PKG_VERSION").as_bytes());
        put_bytes(&mut scope, 1, &scope_msg);
        for span in members {
            put_bytes(&mut scope, 2, &encode_span(span));
        }

        let mut resource_spans = Vec::new();
        put_bytes(&mut resource_spans, 1, &resource);
        put_bytes(&mut resource_spans, 2, &scope);
        put_bytes(&mut request, 1, &resource_spans);
    }
    request
}

/// POSTs an encoded payload to an OTLP/HTTP endpoint.
///
/// `endpoint` is the base URL; `/v1/traces` is appended when absent, matching
/// what every collector expects and what `OTEL_EXPORTER_OTLP_ENDPOINT` means.
pub(crate) fn post_traces(endpoint: &str, body: &[u8]) -> Result<(), String> {
    let rest = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| match endpoint.strip_prefix("https://") {
            // Being explicit beats a confusing connection error: we speak plain
            // HTTP on purpose, and the fix is a local collector, not a flag.
            Some(_) => format!(
                "{endpoint} is https; elephc posts plain OTLP/HTTP, so point it at a local \
                 collector (http://127.0.0.1:4318) and let that forward over TLS"
            ),
            None => format!("{endpoint} must start with http://"),
        })?;
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, ""),
    };
    let path = if path.is_empty() || path == "/" {
        "/v1/traces"
    } else {
        path
    };
    let address = if authority.contains(':') {
        authority.to_string()
    } else {
        format!("{authority}:4318")
    };

    let mut stream = std::net::TcpStream::connect(&address)
        .map_err(|error| format!("cannot reach the collector at {address}: {error}"))?;
    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/x-protobuf\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body))
        .map_err(|error| format!("cannot send to {address}: {error}"))?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| format!("no reply from {address}: {error}"))?;
    let text = String::from_utf8_lossy(&response);
    let status = text.lines().next().unwrap_or("");
    // A collector that rejects the payload answers 4xx with a reason; surfacing
    // it beats reporting success because the socket write worked.
    if status.contains(" 2") {
        Ok(())
    } else {
        Err(format!("collector refused the export: {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> OtlpSpan {
        OtlpSpan {
            service: "gateway".into(),
            trace_id: "4bf92f3577b34da6a3ce929d0e0e4736".into(),
            span_id: "00f067aa0ba902b7".into(),
            parent_span_id: String::new(),
            name: "GET /orders".into(),
            start_unix_nano: 1_700_000_000_000_000_000,
            end_unix_nano: 1_700_000_000_005_000_000,
            attributes: vec![("elephc.queries".into(), 3)],
            string_attributes: vec![("http.route".into(), "GET /orders".into())],
        }
    }

    /// Ids must travel as BYTES. Sending the hex text produces a 32-byte trace id
    /// that is structurally valid protobuf and correlates with nothing — the kind
    /// of bug that shows up as "my spans are in the backend but in their own
    /// trace", long after anyone would look here.
    #[test]
    fn ids_are_decoded_from_hex_to_raw_bytes() {
        assert_eq!(hex_bytes("00f067aa0ba902b7").len(), 8);
        assert_eq!(hex_bytes("4bf92f3577b34da6a3ce929d0e0e4736").len(), 16);
        assert_eq!(hex_bytes("00ff").as_slice(), &[0x00, 0xff]);
        // Malformed input yields nothing rather than half an id.
        assert!(hex_bytes("zz").is_empty());
    }

    /// Timestamps are fixed64: a varint here mis-frames every following field.
    #[test]
    fn timestamps_are_fixed64_little_endian() {
        let mut out = Vec::new();
        put_fixed64(&mut out, 7, 1);
        // tag byte (field 7, wire type 1) then exactly 8 bytes.
        assert_eq!(out.len(), 9, "{out:?}");
        assert_eq!(out[0], (7 << 3) | 1);
        assert_eq!(&out[1..], &[1, 0, 0, 0, 0, 0, 0, 0]);
    }

    /// The payload must carry the ids, the route and the service name, and must
    /// group by service — one ResourceSpans per service, not per span.
    #[test]
    fn the_payload_carries_identity_and_groups_by_service() {
        let mut second = span();
        second.span_id = "00f067aa0ba902b8".into();
        let mut other = span();
        other.service = "inventory".into();
        let body = encode_traces(&[span(), second, other]);

        assert!(!body.is_empty());
        let needle = hex_bytes("4bf92f3577b34da6a3ce929d0e0e4736");
        assert!(
            body.windows(needle.len()).any(|w| w == needle.as_slice()),
            "the raw trace id must appear in the payload"
        );
        assert!(body.windows(7).any(|w| w == b"gateway"));
        assert!(body.windows(9).any(|w| w == b"inventory"));
        assert!(body.windows(10).any(|w| w == b"http.route"));
    }

    /// An https endpoint fails with the fix in the message rather than a socket error.
    #[test]
    fn an_https_endpoint_says_what_to_do_instead() {
        let error = post_traces("https://collector.example", b"").unwrap_err();
        assert!(error.contains("local collector"), "{error}");
        assert!(post_traces("collector.example", b"").unwrap_err().contains("http://"));
    }
}
