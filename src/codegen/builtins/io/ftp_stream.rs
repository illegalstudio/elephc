//! Purpose:
//! Lowers `fopen()` calls whose path is an `ftp://` URL.
//! Parses the URL at compile time and opens the file through the
//! `__rt_ftp_open` runtime helper, which performs the FTP handshake.
//!
//! Called from:
//! - `crate::codegen::builtins::io::fopen::emit()` when the path literal
//!   begins with `ftp://`.
//!
//! Key details:
//! - The URL must be a string literal: `ftp://[user[:pass]@]host[:port]/path`.
//!   v1 logs in anonymously and reads in binary (`TYPE I`) passive mode, so any
//!   `user:pass@` credentials in the URL are ignored.
//! - The control address (`tcp://host:port`) and the `RETR` command line are
//!   built at compile time and handed to `__rt_ftp_open`.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits a `fopen("ftp://...", ...)` call. The path is known to be a string
/// literal beginning with `ftp://`.
pub fn emit(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fopen() ftp:// stream");
    // The mode and optional fopen args are evaluated for side effects;
    // ftp:// streams are read-only.
    super::fopen::emit_mode_and_ignored_optional_args(args, emitter, ctx, data);
    emit_open_fd(args, emitter, data);
    super::fopen::box_fopen_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Emits the `ftp://` open, leaving the data-connection fd (or -1 on an
/// unparseable URL) in the int-result register. Does NOT evaluate a mode
/// argument or box the result — shared by `fopen()` and `file_get_contents()`.
pub(super) fn emit_open_fd(args: &[Expr], emitter: &mut Emitter, data: &mut DataSection) {
    let parsed = match &args[0].kind {
        ExprKind::StringLiteral(url) => parse_ftp_url(url),
        _ => None,
    };
    match parsed {
        Some((ctrl_addr, retr_cmd)) => {
            let (ctrl_sym, ctrl_len) = data.add_string(ctrl_addr.as_bytes());
            let (retr_sym, retr_len) = data.add_string(retr_cmd.as_bytes());
            match emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_symbol_address(emitter, "x0", &ctrl_sym);
                    emitter.instruction(&format!("mov x1, #{}", ctrl_len));     // control address length
                    abi::emit_symbol_address(emitter, "x2", &retr_sym);
                    emitter.instruction(&format!("mov x3, #{}", retr_len));     // RETR command length
                }
                Arch::X86_64 => {
                    abi::emit_symbol_address(emitter, "rdi", &ctrl_sym);
                    emitter.instruction(&format!("mov rsi, {}", ctrl_len));     // control address length
                    abi::emit_symbol_address(emitter, "rdx", &retr_sym);
                    emitter.instruction(&format!("mov rcx, {}", retr_len));     // RETR command length
                }
            }
            abi::emit_call_label(emitter, "__rt_ftp_open");                     // run the FTP handshake and open the file
        }
        None => match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("mov x0, #-1"),                // unparseable ftp:// URL lowers to PHP false
            Arch::X86_64 => emitter.instruction("mov rax, -1"),                 // unparseable ftp:// URL lowers to PHP false
        },
    }
}

/// Parses an `ftp://[user[:pass]@]host[:port]/path` URL into the control
/// address (`tcp://host:port`) and the `RETR <path>` command line. Returns
/// `None` when the URL has no path component.
fn parse_ftp_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("ftp://")?;
    // v1 logs in anonymously; drop any user:pass@ userinfo before the host.
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
