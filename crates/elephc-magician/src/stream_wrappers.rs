//! Purpose:
//! Parses eval-supported PHP stream wrapper URLs that can be handled entirely
//! inside the magician process.
//!
//! Called from:
//! - `crate::stream_resources::EvalStreamResources` for `fopen()` wrapper routing.
//! - Filesystem builtins that need direct `file_get_contents()` / `file_put_contents()` handling.
//!
//! Key details:
//! - Network wrappers are deliberately not implemented here; callers receive
//!   `None` for unsupported schemes and surface ordinary PHP false paths.

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
