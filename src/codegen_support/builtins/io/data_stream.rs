//! Purpose:
//! Lowers `fopen()` calls whose path is a `data://` URI (RFC 2397).
//! Decodes the payload at compile time and materializes it as a readable
//! stream descriptor through the `__rt_data_stream` runtime helper.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::fopen::emit()` when the path literal
//!   begins with `data://`.
//!
//! Key details:
//! - The URI must be a string literal; the `;base64` payload is base64-decoded
//!   and any other payload is percent-decoded, both entirely at compile time.
//! - An unparseable URI lowers to PHP `false`, matching a failed `fopen()`.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits a `fopen("data://...", ...)` call. The path is known to be a
/// string literal beginning with `data://`.
pub fn emit(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fopen() data:// stream");
    let decoded = match &args[0].kind {
        ExprKind::StringLiteral(path) => decode_data_uri(path),
        _ => None,
    };
    // The mode and optional fopen args are evaluated for side effects;
    // data:// streams are read-only regardless of the requested mode.
    super::fopen::emit_mode_and_ignored_optional_args(args, emitter, ctx, data);
    match decoded {
        Some(bytes) => {
            let (symbol, len) = data.add_string(&bytes);
            match emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_symbol_address(emitter, "x0", &symbol);
                    emitter.instruction(&format!("mov x1, #{}", len));          // decoded payload length
                }
                Arch::X86_64 => {
                    abi::emit_symbol_address(emitter, "rdi", &symbol);
                    emitter.instruction(&format!("mov rsi, {}", len));          // decoded payload length
                }
            }
            abi::emit_call_label(emitter, "__rt_data_stream");                  // build the readable data:// descriptor
        }
        None => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("mov x0, #-1"),                // unparseable data:// URI lowers to PHP false
            Arch::X86_64 => emitter.instruction("mov rax, -1"),                 // unparseable data:// URI lowers to PHP false
        },
    }
    super::fopen::box_fopen_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Decodes a `data://[<mediatype>][;base64],<payload>` URI into its raw bytes.
/// Returns `None` when the URI lacks the mandatory comma or carries an invalid
/// base64 payload.
fn decode_data_uri(path: &str) -> Option<Vec<u8>> {
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

/// Decodes a base64 payload, tolerating embedded whitespace and stopping at the
/// first `=` padding character. Returns `None` on an invalid alphabet byte.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    /// Converts one base64 byte into its six-bit value for data:// decoding.
    fn sextet(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a') as u32 + 26),
            b'0'..=b'9' => Some((c - b'0') as u32 + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0u32;
    for &c in input.as_bytes() {
        if c == b'=' {
            break;
        }
        if c.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | sextet(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Percent-decodes a `data://` payload: `%HH` escapes become their byte value
/// and `+` becomes a space, matching PHP's `data://` wrapper.
fn percent_decode(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                match (hi, lo) {
                    (Some(hi), Some(lo)) => {
                        out.push((hi * 16 + lo) as u8);
                        i += 3;
                    }
                    _ => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    out
}
