//! Purpose:
//! Lowers `fopen()` calls whose path is an `ftps://` URL (RFC 4217 explicit
//! FTP over TLS). Parses the URL at compile time, sets `_ftp_use_tls = 1`
//! so the runtime helper performs the AUTH TLS handshake, then dispatches
//! into the standard `__rt_ftp_open` flow.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::fopen::emit()` when the path literal
//!   begins with `ftps://`.
//!
//! Key details:
//! - The URL must be a string literal: `ftps://[user[:pass]@]host[:port]/path`.
//!   v1 logs in anonymously (binary, passive). The default port is 21 — the
//!   "explicit" RFC 4217 mode where the client connects in cleartext and
//!   upgrades via `AUTH TLS`. (PHP also accepts implicit ftps on port 990,
//!   but elephc doesn't implement that v1.)
//! - The runtime helper attaches elephc-tls to both the control fd (after
//!   `AUTH TLS`) and the PASV data fd, so subsequent `fread` automatically
//!   routes through TLS via the `_tls_sessions` table.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits codegen for PHP `ftps_stream()` stream and I/O builtin calls.
pub fn emit(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fopen() ftps:// stream");
    // The mode and optional fopen args are evaluated for side effects;
    // ftps:// streams are read-only.
    super::fopen::emit_mode_and_ignored_optional_args(args, emitter, ctx, data);
    emit_open_fd(args, emitter, data);
    super::fopen::box_fopen_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Emits the `ftps://` open (publishing the TLS fn-pointers and flagging the
/// AUTH-TLS handshake), leaving the data-connection fd (or -1 on an unparseable
/// URL) in the int-result register. Does NOT evaluate a mode argument or box the
/// result — shared by `fopen()` and `file_get_contents()`.
pub(super) fn emit_open_fd(args: &[Expr], emitter: &mut Emitter, data: &mut DataSection) {
    let parsed = match &args[0].kind {
        ExprKind::StringLiteral(url) => parse_ftps_url(url),
        _ => None,
    };
    match parsed {
        Some((ctrl_addr, retr_cmd)) => {
            let (ctrl_sym, ctrl_len) = data.add_string(ctrl_addr.as_bytes());
            let (retr_sym, retr_len) = data.add_string(retr_cmd.as_bytes());
            // Publish the elephc-tls C entries into their runtime slots so the
            // ftp helper's AUTH-TLS path can route through them.
            super::https_stream::publish_tls_function_pointers(emitter);
            match emitter.target.arch {
                Arch::AArch64 => {
                    // Set _ftp_use_tls = 1 so __rt_ftp_open does the AUTH TLS
                    // dance, PBSZ 0 / PROT P, and TLS-attaches both channels.
                    abi::emit_symbol_address(emitter, "x9", "_ftp_use_tls");
                    emitter.instruction("mov x10, #1");                         // flag the next FTP open as AUTH-TLS
                    emitter.instruction("str x10, [x9]");                       // publish the AUTH-TLS flag for __rt_ftp_open
                    abi::emit_symbol_address(emitter, "x0", &ctrl_sym);
                    emitter.instruction(&format!("mov x1, #{}", ctrl_len));     // control address length
                    abi::emit_symbol_address(emitter, "x2", &retr_sym);
                    emitter.instruction(&format!("mov x3, #{}", retr_len));     // RETR command length
                }
                Arch::X86_64 => {
                    abi::emit_store_imm_to_symbol(emitter, "_ftp_use_tls", 0, 1); // publish the AUTH-TLS flag for __rt_ftp_open
                    abi::emit_symbol_address(emitter, "rdi", &ctrl_sym);
                    emitter.instruction(&format!("mov rsi, {}", ctrl_len));     // control address length
                    abi::emit_symbol_address(emitter, "rdx", &retr_sym);
                    emitter.instruction(&format!("mov rcx, {}", retr_len));     // RETR command length
                }
            }
            abi::emit_call_label(emitter, "__rt_ftp_open");                     // run the FTP+TLS handshake
        }
        None => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("mov x0, #-1"),                // unparseable ftps:// URL lowers to PHP false
            Arch::X86_64 => emitter.instruction("mov rax, -1"),                 // unparseable ftps:// URL lowers to PHP false
        },
    }
}

/// Parses an `ftps://[user[:pass]@]host[:port]/path` URL. Same shape as the
/// plain `ftp://` parser; only the prefix and default port behaviour differ.
fn parse_ftps_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("ftps://")?;
    let after_userinfo = match rest.find('@') {
        Some(at) => &rest[at + 1..],
        None => rest,
    };
    let slash = after_userinfo.find('/')?;
    let authority = &after_userinfo[..slash];
    let path = &after_userinfo[slash..];
    if authority.is_empty() || path.len() < 2 {
        return None;
    }
    let (host, port) = match authority.rfind(':') {
        Some(colon) => (&authority[..colon], &authority[colon + 1..]),
        None => (authority, "21"),
    };
    if host.is_empty() || port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((
        format!("tcp://{}:{}", host, port),
        format!("RETR {}\r\n", path),
    ))
}
