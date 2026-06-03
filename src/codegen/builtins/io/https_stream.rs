//! Purpose:
//! Lowers `fopen()` calls whose path is an `https://` URL.
//! Parses the URL at compile time, publishes the elephc-tls C entry points
//! into the runtime function-pointer slots, and opens the response body
//! through `__rt_https_open`.
//!
//! Called from:
//! - `crate::codegen::builtins::io::fopen::emit()` when the path literal
//!   begins with `https://`.
//!
//! Key details:
//! - The URL must be a string literal: `https://[user@]host[:port]/path`.
//!   v1 issues a plain HTTP/1.0 `GET` and ignores any `user@` userinfo.
//! - Indirect function pointers (`_elephc_tls_*_fn`) keep the shared runtime
//!   free of any direct elephc-tls reference, so only programs that actually
//!   open https URLs trigger `-lelephc_tls` linkage. The wrapper publishes
//!   the 4 entry points (`connect`, `write`, `read`, `close`) into BSS slots
//!   right before the call, idempotently overwriting them on every fopen.
//! - The host string and HTTP request body are materialised in `.rodata`;
//!   the port is passed as an immediate integer.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits a `fopen("https://...", ...)` call. The path is known to be a string
/// literal beginning with `https://`.
pub fn emit(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fopen() https:// stream");
    // The mode and optional fopen args are evaluated for side effects;
    // https:// streams are read-only.
    super::fopen::emit_mode_and_ignored_optional_args(args, emitter, ctx, data);
    emit_open_fd(args, emitter, data);
    super::fopen::box_fopen_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Emits the `https://` open (publishing the TLS fn-pointers), leaving the
/// response-body fd (or -1 on an unparseable URL) in the int-result register.
/// Does NOT evaluate a mode argument or box the result — shared by `fopen()`
/// and `file_get_contents()`.
pub(super) fn emit_open_fd(args: &[Expr], emitter: &mut Emitter, data: &mut DataSection) {
    let parsed = match &args[0].kind {
        ExprKind::StringLiteral(url) => parse_https_url(url),
        _ => None,
    };
    match parsed {
        Some(parts) => {
            let (host_sym, host_len) = data.add_string(parts.host.as_bytes());
            let (req_sym, req_len) = data.add_string(parts.request.as_bytes());
            publish_tls_function_pointers(emitter);
            match emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_symbol_address(emitter, "x0", &host_sym);
                    emitter.instruction(&format!("mov x1, #{}", host_len));     // hostname length
                    emitter.instruction(&format!("mov x2, #{}", parts.port));   // TCP port for the TLS handshake
                    abi::emit_symbol_address(emitter, "x3", &req_sym);
                    emitter.instruction(&format!("mov x4, #{}", req_len));      // HTTP request length
                }
                Arch::X86_64 => {
                    abi::emit_symbol_address(emitter, "rdi", &host_sym);
                    emitter.instruction(&format!("mov rsi, {}", host_len));     // hostname length
                    emitter.instruction(&format!("mov rdx, {}", parts.port));   // TCP port for the TLS handshake
                    abi::emit_symbol_address(emitter, "rcx", &req_sym);
                    emitter.instruction(&format!("mov r8, {}", req_len));       // HTTP request length
                }
            }
            abi::emit_call_label(emitter, "__rt_https_open");                   // run the TLS-secured request and open the body
        }
        None => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("mov x0, #-1"),                // unparseable https:// URL lowers to PHP false
            Arch::X86_64 => emitter.instruction("mov rax, -1"),                 // unparseable https:// URL lowers to PHP false
        },
    }
}

/// Stores the addresses of the elephc-tls C entry points into the
/// `_elephc_tls_*_fn` runtime slots so `__rt_https_open` and
/// `stream_socket_enable_crypto` can call through them.
pub(crate) fn publish_tls_function_pointers(emitter: &mut Emitter) {
    const ENTRIES: &[(&str, &str)] = &[
        ("elephc_tls_connect", "_elephc_tls_connect_fn"),
        ("elephc_tls_connect_insecure", "_elephc_tls_connect_insecure_fn"),
        ("elephc_tls_connect_cafile", "_elephc_tls_connect_cafile_fn"),
        ("elephc_tls_connect_capath", "_elephc_tls_connect_capath_fn"),
        ("elephc_tls_connect_peer_name", "_elephc_tls_connect_peer_name_fn"),
        ("elephc_tls_write", "_elephc_tls_write_fn"),
        ("elephc_tls_read", "_elephc_tls_read_fn"),
        ("elephc_tls_close", "_elephc_tls_close_fn"),
        ("elephc_tls_attach_fd", "_elephc_tls_attach_fd_fn"),
        (
            "elephc_tls_attach_fd_client_cert",
            "_elephc_tls_attach_fd_client_cert_fn",
        ),
        (
            "elephc_tls_connect_client_cert",
            "_elephc_tls_connect_client_cert_fn",
        ),
    ];
    match emitter.target.arch {
        Arch::AArch64 => {
            for (c_name, slot) in ENTRIES {
                let extern_sym = emitter.target.extern_symbol(c_name);
                abi::emit_extern_symbol_address(emitter, "x9", &extern_sym);
                abi::emit_symbol_address(emitter, "x10", slot);
                emitter.instruction("str x9, [x10]");                           // publish the elephc-tls entry into its runtime slot
            }
        }
        Arch::X86_64 => {
            for (c_name, slot) in ENTRIES {
                let extern_sym = emitter.target.extern_symbol(c_name);
                abi::emit_extern_symbol_address(emitter, "r9", &extern_sym);
                emitter.instruction(&format!("mov QWORD PTR [rip + {}], r9", slot)); // publish the elephc-tls entry into its runtime slot
            }
        }
    }
}

struct HttpsUrl {
    host: String,
    port: u16,
    request: String,
}

/// Parses an `https://[user@]host[:port]/path` URL into the hostname, the TCP
/// port (defaulting to 443), and the HTTP/1.0 request text. Returns `None`
/// when the authority is missing or the port is non-numeric.
///
/// IPv6 hosts use the bracket-literal form (`[::1]`, `[2001:db8::1]:8443`);
/// the brackets are stripped from the value passed to `elephc_tls_connect`
/// but preserved in the `Host:` header per RFC 7230 §5.4.
fn parse_https_url(url: &str) -> Option<HttpsUrl> {
    let rest = url.strip_prefix("https://")?;
    let after_userinfo = match rest.find('@') {
        Some(at) => &rest[at + 1..],
        None => rest,
    };
    let (authority, path) = match after_userinfo.find('/') {
        Some(slash) => (&after_userinfo[..slash], &after_userinfo[slash..]),
        None => (after_userinfo, "/"),
    };
    if authority.is_empty() {
        return None;
    }
    let (connect_host, host_header, port_str) = split_authority(authority)?;
    let port: u16 = port_str.parse().ok()?;
    Some(HttpsUrl {
        host: connect_host,
        port,
        request: format!(
            "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
            path, host_header
        ),
    })
}

/// Splits `authority` into `(connect_host, host_header, port_str)`.
///
/// For an IPv6 literal `[::1]:8443`, `connect_host` is `::1` (the bytes
/// `elephc_tls_connect` will actually resolve) while `host_header` keeps the
/// brackets so the HTTP `Host:` line stays RFC-compliant.
fn split_authority(authority: &str) -> Option<(String, String, &str)> {
    if let Some(rest) = authority.strip_prefix('[') {
        // IPv6 literal: walk to the closing bracket, then look for ':port'.
        let close = rest.find(']')?;
        let v6 = &rest[..close];
        if v6.is_empty() {
            return None;
        }
        let after = &rest[close + 1..];
        let port_str = if after.is_empty() {
            "443"
        } else {
            after.strip_prefix(':').filter(|p| !p.is_empty())?
        };
        Some((v6.to_string(), format!("[{}]", v6), port_str))
    } else {
        let (host, port_str) = match authority.rfind(':') {
            Some(colon) => (&authority[..colon], &authority[colon + 1..]),
            None => (authority, "443"),
        };
        if host.is_empty() || port_str.is_empty() {
            return None;
        }
        Some((host.to_string(), host.to_string(), port_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses an HTTPS URL into host, port, and path pieces for unit assertions.
    fn first_line(url: &str) -> Option<(String, u16, String)> {
        let parsed = parse_https_url(url)?;
        let request_line = parsed.request.lines().next()?.to_string();
        Some((parsed.host, parsed.port, request_line))
    }

    /// Verifies HTTPS URL parsing with the default port.
    #[test]
    fn parses_default_port() {
        let (host, port, line) = first_line("https://example.com/").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 443);
        assert_eq!(line, "GET / HTTP/1.0");
    }

    /// Verifies HTTPS URL parsing with an explicit port and path.
    #[test]
    fn parses_explicit_port_and_path() {
        let (host, port, line) = first_line("https://example.com:8443/api?id=1").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 8443);
        assert_eq!(line, "GET /api?id=1 HTTP/1.0");
    }

    /// Verifies HTTPS URL parsing with an IPv6 literal and default port.
    #[test]
    fn parses_ipv6_literal_default_port() {
        let parsed = parse_https_url("https://[::1]/").unwrap();
        assert_eq!(parsed.host, "::1");
        assert_eq!(parsed.port, 443);
        assert!(parsed.request.contains("\r\nHost: [::1]\r\n"));
    }

    /// Verifies HTTPS URL parsing with an IPv6 literal and explicit port.
    #[test]
    fn parses_ipv6_literal_with_port() {
        let parsed = parse_https_url("https://[2001:db8::1]:8443/path").unwrap();
        assert_eq!(parsed.host, "2001:db8::1");
        assert_eq!(parsed.port, 8443);
        assert!(parsed.request.contains("\r\nHost: [2001:db8::1]\r\n"));
        assert!(parsed.request.starts_with("GET /path HTTP/1.0"));
    }

    /// Verifies HTTPS URL rejection with a missing authority.
    #[test]
    fn rejects_missing_authority() {
        assert!(parse_https_url("https://").is_none());
        assert!(parse_https_url("https:///path").is_none());
    }

    /// Verifies HTTPS URL rejection with an unclosed IPv6 literal.
    #[test]
    fn rejects_unclosed_ipv6() {
        assert!(parse_https_url("https://[::1/").is_none());
    }

    /// Verifies HTTPS URL rejection with an empty port.
    #[test]
    fn rejects_empty_port() {
        assert!(parse_https_url("https://example.com:/").is_none());
    }
}
