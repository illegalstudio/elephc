//! Purpose:
//! FTP and HTTP literal stream opening.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use elephc_memoir::buffers::io_scratch;

use super::*;

/// Lowers a literal `fopen("ftp://...", ...)` through the FTP runtime wrapper.
pub(super) fn lower_literal_ftp_fopen(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
) -> Result<()> {
    emit_literal_ftp_fopen_result(ctx, path)?;
    store_if_result(ctx, inst)
}

/// Emits the boxed result for a literal `ftp://` stream open.
pub(super) fn emit_literal_ftp_fopen_result(ctx: &mut FunctionContext<'_>, path: &str) -> Result<()> {
    match parse_ftp_url_for_fopen(path) {
        Some((ctrl_addr, retr_cmd)) => {
            let (ctrl_sym, ctrl_len) = ctx.data.add_string(ctrl_addr.as_bytes());
            let (retr_sym, retr_len) = ctx.data.add_string(retr_cmd.as_bytes());
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_symbol_address(ctx.emitter, "x0", &ctrl_sym);
                    ctx.emitter.instruction(&format!("mov x1, #{}", ctrl_len)); // pass the FTP control address byte length
                    abi::emit_symbol_address(ctx.emitter, "x2", &retr_sym);
                    ctx.emitter.instruction(&format!("mov x3, #{}", retr_len)); // pass the FTP RETR command byte length
                }
                Arch::X86_64 => {
                    abi::emit_symbol_address(ctx.emitter, "rdi", &ctrl_sym);
                    ctx.emitter.instruction(&format!("mov rsi, {}", ctrl_len)); // pass the FTP control address byte length
                    abi::emit_symbol_address(ctx.emitter, "rdx", &retr_sym);
                    ctx.emitter.instruction(&format!("mov rcx, {}", retr_len)); // pass the FTP RETR command byte length
                }
            }
            abi::emit_call_label(ctx.emitter, "__rt_ftp_open");
        }
        None => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #-1");                         // unparseable ftp:// URL lowers to PHP false
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rax, -1");                         // unparseable ftp:// URL lowers to PHP false
            }
        },
    }
    box_stream_fd_or_false_result(ctx, "fopen_ftp");
    Ok(())
}

/// Parses `ftp://[user[:pass]@]host[:port]/path` into runtime FTP open inputs.
pub(super) fn parse_ftp_url_for_fopen(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("ftp://")?;
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

/// Lowers a literal `fopen("http://...", ...)` through the HTTP runtime wrapper.
pub(super) fn lower_literal_http_fopen(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
) -> Result<()> {
    emit_literal_http_fopen_result(ctx, path)?;
    store_if_result(ctx, inst)
}

/// Emits the boxed result for a literal `http://` stream open.
pub(super) fn emit_literal_http_fopen_result(ctx: &mut FunctionContext<'_>, path: &str) -> Result<()> {
    match parse_http_url_for_fopen(path) {
        Some(parsed) => {
            let (addr_sym, addr_len) = ctx.data.add_string(parsed.addr.as_bytes());
            let (host_sym, host_len) = ctx.data.add_string(parsed.host.as_bytes());
            let (path_sym, path_len) = ctx.data.add_string(parsed.path.as_bytes());
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_symbol_address(ctx.emitter, "x0", &host_sym);
                    ctx.emitter.instruction(&format!("mov x1, #{}", host_len)); // pass the HTTP Host header byte length
                    abi::emit_symbol_address(ctx.emitter, "x2", &path_sym);
                    ctx.emitter.instruction(&format!("mov x3, #{}", path_len)); // pass the request path byte length
                    abi::emit_call_label(ctx.emitter, "__rt_http_build_request");
                    abi::emit_push_reg(ctx.emitter, "x0");
                    abi::emit_symbol_address(ctx.emitter, "x0", &addr_sym);
                    ctx.emitter.instruction(&format!("mov x1, #{}", addr_len)); // pass the TCP address byte length
                    io_scratch::HTTP_REQUEST.emit_address(ctx.emitter, "x2");
                    abi::emit_pop_reg(ctx.emitter, "x3");
                    abi::emit_call_label(ctx.emitter, "__rt_http_open");
                }
                Arch::X86_64 => {
                    abi::emit_symbol_address(ctx.emitter, "rdi", &host_sym);
                    ctx.emitter.instruction(&format!("mov rsi, {}", host_len)); // pass the HTTP Host header byte length
                    abi::emit_symbol_address(ctx.emitter, "rdx", &path_sym);
                    ctx.emitter.instruction(&format!("mov rcx, {}", path_len)); // pass the request path byte length
                    abi::emit_call_label(ctx.emitter, "__rt_http_build_request");
                    abi::emit_push_reg(ctx.emitter, "rax");
                    abi::emit_symbol_address(ctx.emitter, "rdi", &addr_sym);
                    ctx.emitter.instruction(&format!("mov rsi, {}", addr_len)); // pass the TCP address byte length
                    io_scratch::HTTP_REQUEST.emit_address(ctx.emitter, "rdx");
                    abi::emit_pop_reg(ctx.emitter, "rcx");
                    abi::emit_call_label(ctx.emitter, "__rt_http_open");
                }
            }
        }
        None => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #-1");                         // unparseable http:// URL lowers to PHP false
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rax, -1");                         // unparseable http:// URL lowers to PHP false
            }
        },
    }
    box_stream_fd_or_false_result(ctx, "fopen_http");
    Ok(())
}

/// Parsed pieces needed by the HTTP runtime open helper.
pub(super) struct ParsedHttpFopenUrl {
    addr: String,
    host: String,
    path: String,
}

/// Parses a literal `http://[user@]host[:port]/path` URL for EIR `fopen`.
pub(super) fn parse_http_url_for_fopen(url: &str) -> Option<ParsedHttpFopenUrl> {
    let rest = url.strip_prefix("http://")?;
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
    let (host, port) = match authority.rfind(':') {
        Some(colon) => (&authority[..colon], &authority[colon + 1..]),
        None => (authority, "80"),
    };
    if host.is_empty() || port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(ParsedHttpFopenUrl {
        addr: format!("tcp://{}:{}", host, port),
        host: if port == "80" {
            host.to_string()
        } else {
            format!("{}:{}", host, port)
        },
        path: path.to_string(),
    })
}

