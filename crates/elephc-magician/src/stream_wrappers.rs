//! Purpose:
//! Parses eval-supported PHP stream wrapper URLs that can be handled entirely
//! inside the magician process.
//!
//! Called from:
//! - `crate::stream_resources::EvalStreamResources` for `fopen()` wrapper routing.
//! - Filesystem builtins that need direct `file_get_contents()` / `file_put_contents()` handling.
//!
//! Key details:
//! - Plain `http://` uses a small blocking HTTP/1.0 client. TLS-backed
//!   `https://` remains outside magician's implemented wrapper paths for now.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const HTTP_TIMEOUT: Duration = Duration::from_secs(5);

/// Returns true when the path names a `php://memory` or `php://temp` stream.
pub(crate) fn is_php_memory_stream(path: &str) -> bool {
    path == "php://memory" || path == "php://temp" || path.starts_with("php://temp/")
}

/// Returns true when the path names a `data://` stream.
pub(crate) fn is_data_stream(path: &str) -> bool {
    path.starts_with("data://")
}

/// Returns true when the path names a `phar://` stream.
pub(crate) fn is_phar_stream(path: &str) -> bool {
    path.starts_with("phar://")
}

/// Returns true when the path names a plain `http://` stream.
pub(crate) fn is_http_stream(path: &str) -> bool {
    path.starts_with("http://")
}

/// Extracts and normalizes the PHP stream-wrapper scheme from a URL-like path.
pub(crate) fn stream_scheme(path: &str) -> Option<String> {
    let separator = path.find("://")?;
    if separator == 0 || !path_has_scheme(path) {
        return None;
    }
    Some(path[..separator].to_ascii_lowercase())
}

/// Maps plain local paths and `file://` URLs onto host filesystem paths.
pub(crate) fn local_filesystem_path(path: &str) -> Option<String> {
    if let Some(rest) = path.strip_prefix("file://") {
        return Some(file_url_local_path(rest));
    }
    if path_has_scheme(path) {
        None
    } else {
        Some(path.to_string())
    }
}

/// Decodes a `data://[<mediatype>][;base64],<payload>` URI into raw bytes.
pub(crate) fn decode_data_uri(path: &str) -> Option<Vec<u8>> {
    let rest = path.strip_prefix("data://")?;
    let comma = rest.find(',')?;
    let meta = &rest[..comma];
    let payload = &rest[comma + 1..];
    if meta.to_ascii_lowercase().ends_with(";base64") {
        base64_decode(payload)
    } else {
        Some(percent_decode(payload))
    }
}

/// Reads a plain HTTP URL into response-body bytes.
pub(crate) fn read_http_url(path: &str) -> Option<Vec<u8>> {
    let request = parse_http_url(path)?;
    let mut stream = TcpStream::connect((request.host.as_str(), request.port)).ok()?;
    let _ = stream.set_read_timeout(Some(HTTP_TIMEOUT));
    let _ = stream.set_write_timeout(Some(HTTP_TIMEOUT));
    let wire_request = format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
        request.path_and_query,
        request.host_header()
    );
    stream.write_all(wire_request.as_bytes()).ok()?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).ok()?;
    parse_http_response_body(&response)
}

/// Parsed `http://` URL pieces needed for a minimal HTTP request.
struct EvalHttpRequest {
    host: String,
    port: u16,
    path_and_query: String,
}

impl EvalHttpRequest {
    /// Returns the Host header value, preserving non-default ports.
    fn host_header(&self) -> String {
        if self.port == 80 {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

/// Parses a plain `http://host[:port][/path][?query]` URL.
fn parse_http_url(path: &str) -> Option<EvalHttpRequest> {
    let rest = path.strip_prefix("http://")?;
    let (authority, suffix) = match rest.find(['/', '?', '#']) {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let (host, port) = parse_http_authority(authority)?;
    let mut path_and_query = match suffix.chars().next() {
        Some('/') => suffix.to_string(),
        Some('?') => format!("/{suffix}"),
        Some('#') | None => "/".to_string(),
        _ => return None,
    };
    if let Some(fragment) = path_and_query.find('#') {
        path_and_query.truncate(fragment);
    }
    if path_and_query.is_empty() {
        path_and_query.push('/');
    }
    Some(EvalHttpRequest {
        host,
        port,
        path_and_query,
    })
}

/// Parses the authority portion of an HTTP URL.
fn parse_http_authority(authority: &str) -> Option<(String, u16)> {
    if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']')?;
        let host = rest[..end].to_string();
        let after = &rest[end + 1..];
        let port = if let Some(port) = after.strip_prefix(':') {
            port.parse::<u16>().ok()?
        } else if after.is_empty() {
            80
        } else {
            return None;
        };
        return Some((host, port));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit()) => {
            (host, port.parse::<u16>().ok()?)
        }
        _ => (authority, 80),
    };
    if host.is_empty() {
        None
    } else {
        Some((host.to_string(), port))
    }
}

/// Extracts the body from an HTTP response with a successful status code.
fn parse_http_response_body(response: &[u8]) -> Option<Vec<u8>> {
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .or_else(|| {
            response
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|index| index + 2)
        })?;
    let status_line_end = response
        .windows(2)
        .position(|window| window == b"\r\n")
        .or_else(|| response.iter().position(|byte| *byte == b'\n'))?;
    let status_line = std::str::from_utf8(&response[..status_line_end]).ok()?;
    let mut pieces = status_line.split_whitespace();
    let _version = pieces.next()?;
    let status = pieces.next()?.parse::<u16>().ok()?;
    if !(200..300).contains(&status) {
        return None;
    }
    Some(response[header_end..].to_vec())
}

/// Converts the part after `file://` into a path understood by the host OS.
fn file_url_local_path(rest: &str) -> String {
    if let Some(localhost_path) = rest.strip_prefix("localhost/") {
        format!("/{localhost_path}")
    } else if rest == "localhost" {
        "/".to_string()
    } else {
        rest.to_string()
    }
}

/// Reports whether a string starts with a PHP-style `scheme://` prefix.
fn path_has_scheme(path: &str) -> bool {
    let Some(separator) = path.find("://") else {
        return false;
    };
    separator > 0
        && path[..separator]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

/// Decodes a base64 payload into raw bytes, rejecting invalid alphabet bytes.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    /// Converts one base64 byte into its six-bit value.
    fn sextet(byte: u8) -> Option<u32> {
        match byte {
            b'A'..=b'Z' => Some(u32::from(byte - b'A')),
            b'a'..=b'z' => Some(u32::from(byte - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(byte - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let mut output = Vec::new();
    let mut accumulator = 0_u32;
    let mut bits = 0_u32;
    for byte in input.bytes() {
        if byte == b'=' {
            break;
        }
        if byte.is_ascii_whitespace() {
            continue;
        }
        accumulator = (accumulator << 6) | sextet(byte)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
        }
    }
    Some(output)
}

/// Percent-decodes the non-base64 data URI payload form.
fn percent_decode(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let high = (bytes[index + 1] as char).to_digit(16);
                let low = (bytes[index + 2] as char).to_digit(16);
                match (high, low) {
                    (Some(high), Some(low)) => {
                        output.push((high * 16 + low) as u8);
                        index += 3;
                    }
                    _ => {
                        output.push(b'%');
                        index += 1;
                    }
                }
            }
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    output
}
