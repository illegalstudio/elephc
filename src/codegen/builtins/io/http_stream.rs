//! Purpose:
//! Lowers `fopen()` calls whose path is an `http://` URL.
//! Parses the URL at compile time and opens the body through the
//! `__rt_http_open` runtime helper after building the request line at
//! runtime so that the active `stream_context_create(['http' => ...])`
//! options can override the method.
//!
//! Called from:
//! - `crate::codegen::builtins::io::fopen::emit()` when the path literal
//!   begins with `http://`.
//!
//! Key details:
//! - The URL must be a string literal: `http://[user@]host[:port]/path`.
//!   Any `user@` userinfo is dropped.
//! - The host and path are emitted as compile-time rodata literals and
//!   handed to `__rt_http_build_request`, which writes the full request
//!   into `_http_req_scratch` (consulting `_stream_context_options`
//!   along the way). `__rt_http_open` then sends that buffer.
//! - When the context has no `[http][method]` override, the runtime
//!   build falls back to `GET`, producing the same wire bytes as the
//!   previous static-only path.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits a `fopen("http://...", ...)` call. The path is known to be a string
/// literal beginning with `http://`.
pub fn emit(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fopen() http:// stream");
    // The mode and optional fopen args are evaluated for side effects;
    // http:// streams are read-only.
    super::fopen::emit_mode_and_ignored_optional_args(args, emitter, ctx, data);
    emit_open_fd(args, emitter, data);
    super::fopen::box_fopen_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Emits the `http://` open, leaving the response-body fd (or -1 on an
/// unparseable URL) in the int-result register. Does NOT evaluate a mode
/// argument or box the result — shared by `fopen()` and `file_get_contents()`.
pub(super) fn emit_open_fd(args: &[Expr], emitter: &mut Emitter, data: &mut DataSection) {
    let parsed = match &args[0].kind {
        ExprKind::StringLiteral(url) => parse_http_url(url),
        _ => None,
    };
    match parsed {
        Some(parsed) => {
            let (addr_sym, addr_len) = data.add_string(parsed.addr.as_bytes());
            let (host_sym, host_len) = data.add_string(parsed.host.as_bytes());
            let (path_sym, path_len) = data.add_string(parsed.path.as_bytes());

            match emitter.target.arch {
                Arch::AArch64 => {
                    // -- build the request at runtime so context [http][method] overrides apply --
                    abi::emit_symbol_address(emitter, "x0", &host_sym);
                    emitter.instruction(&format!("mov x1, #{}", host_len));     // host length
                    abi::emit_symbol_address(emitter, "x2", &path_sym);
                    emitter.instruction(&format!("mov x3, #{}", path_len));     // path length
                    abi::emit_call_label(emitter, "__rt_http_build_request");   // x0 = total request length
                    abi::emit_push_reg(emitter, "x0");                          // preserve the request length across the addr setup
                    abi::emit_symbol_address(emitter, "x0", &addr_sym);
                    emitter.instruction(&format!("mov x1, #{}", addr_len));     // TCP address length
                    abi::emit_symbol_address(emitter, "x2", "_http_req_scratch"); // request payload pointer
                    abi::emit_pop_reg(emitter, "x3");                           // request length
                    abi::emit_call_label(emitter, "__rt_http_open");            // send the HTTP request and open the response body
                }
                Arch::X86_64 => {
                    abi::emit_symbol_address(emitter, "rdi", &host_sym);
                    emitter.instruction(&format!("mov rsi, {}", host_len));     // host length
                    abi::emit_symbol_address(emitter, "rdx", &path_sym);
                    emitter.instruction(&format!("mov rcx, {}", path_len));     // path length
                    abi::emit_call_label(emitter, "__rt_http_build_request");   // rax = total request length
                    abi::emit_push_reg(emitter, "rax");                         // preserve the request length across the addr setup
                    abi::emit_symbol_address(emitter, "rdi", &addr_sym);
                    emitter.instruction(&format!("mov rsi, {}", addr_len));     // TCP address length
                    abi::emit_symbol_address(emitter, "rdx", "_http_req_scratch"); // request payload pointer
                    abi::emit_pop_reg(emitter, "rcx");                          // request length
                    abi::emit_call_label(emitter, "__rt_http_open");            // send the HTTP request and open the response body
                }
            }
        }
        None => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("mov x0, #-1"),                // unparseable http:// URL lowers to PHP false
            Arch::X86_64 => emitter.instruction("mov rax, -1"),                 // unparseable http:// URL lowers to PHP false
        },
    }
}

struct ParsedHttpUrl {
    addr: String,
    host: String,
    path: String,
}

/// Parses an `http://[user@]host[:port]/path` URL into the TCP address
/// (`tcp://host:port`), the host (for the Host: header), and the request
/// path. Returns `None` when the authority is missing or the port is
/// non-numeric.
fn parse_http_url(url: &str) -> Option<ParsedHttpUrl> {
    let rest = url.strip_prefix("http://")?;
    // Drop any `user@` userinfo before the host — v1 ignores credentials.
    let after_userinfo = match rest.find('@') {
        Some(at) => &rest[at + 1..],
        None => rest,
    };
    // Split the authority from the path; a missing path defaults to "/".
    let (authority, path) = match after_userinfo.find('/') {
        Some(slash) => (&after_userinfo[..slash], &after_userinfo[slash..]),
        None => (after_userinfo, "/"),
    };
    if authority.is_empty() {
        return None;
    }
    let (host, port) = match authority.rfind(':') {
        Some(colon) => (&authority[..colon], &authority[colon + 1..]),
        None => (authority, "80"),
    };
    if host.is_empty() || port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(ParsedHttpUrl {
        addr: format!("tcp://{}:{}", host, port),
        // The Host: header (and the request_fulluri absolute URI, which is
        // built as "http://" + host + path) include the port for non-default
        // ports, matching PHP — e.g. "127.0.0.1:8080", but bare "host" on :80.
        host: if port == "80" {
            host.to_string()
        } else {
            format!("{}:{}", host, port)
        },
        path: path.to_string(),
    })
}
