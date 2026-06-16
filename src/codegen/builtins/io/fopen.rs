//! Purpose:
//! Emits PHP `fopen` file input builtin calls.
//! Coordinates path or stream arguments with runtime helpers that allocate returned strings or arrays.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Failure paths must distinguish PHP false from empty string or empty array results.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits the `fopen` builtin call, evaluating arguments in source order before
/// materializing filename and mode in ABI order for `__rt_fopen`.
///
/// On success, boxes the native file descriptor as a PHP resource (tag 9). On
/// failure (negative descriptor), boxes PHP false (tag 3) to distinguish from
/// empty string or empty array.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fopen()");
    // The php:// wrapper exposes the standard streams. A statically-known
    // php://stdin|stdout|stderr|input|output path resolves to its descriptor
    // without touching the filesystem; the mode is still evaluated for effects.
    if let ExprKind::StringLiteral(path) = &args[0].kind {
        if let Some(fd) = php_standard_stream_fd(path) {
            emit_mode_and_ignored_optional_args(args, emitter, ctx, data);
            emit_standard_stream_resource(fd, emitter);
            return Some(PhpType::Mixed);
        }
        if let Some(fd) = php_fd_stream(path) {
            // php://fd/N opens descriptor N directly. Useful for forwarding
            // pre-opened descriptors into the PHP stream layer.
            emit_mode_and_ignored_optional_args(args, emitter, ctx, data);
            emit_standard_stream_resource(fd, emitter);
            return Some(PhpType::Mixed);
        }
        if is_php_memory_stream(path) {
            // php://memory and php://temp are backed by an anonymous temp
            // file: a real descriptor, so every fd-based stream builtin
            // operates on them unchanged. The mode is evaluated for effects.
            emit_mode_and_ignored_optional_args(args, emitter, ctx, data);
            abi::emit_call_label(emitter, "__rt_tmpfile");                      // create the anonymous backing descriptor
            box_fopen_result(emitter, ctx);
            return Some(PhpType::Mixed);
        }
        if path.starts_with("php://filter/") {
            // php://filter/[read=|write=]<filter>/resource=<path> opens the
            // underlying resource and attaches a built-in filter to it.
            return super::php_filter_stream::emit(_name, args, emitter, ctx, data);
        }
        if path.starts_with("data://") {
            // A data:// URI is decoded at compile time and lowered to a
            // readable stream over its payload.
            return super::data_stream::emit(args, emitter, ctx, data);
        }
        if path.starts_with("phar://") {
            // A read-mode phar:// entry is read from the archive at compile
            // time and lowered to a readable stream over its decoded bytes;
            // write modes route to the PHAR write bridge.
            return super::phar_stream::emit(args, emitter, ctx, data);
        }
        if path.starts_with("ftp://") {
            // An ftp:// URL is opened through the FTP handshake runtime.
            return super::ftp_stream::emit(args, emitter, ctx, data);
        }
        if path.starts_with("ftps://") {
            // ftps:// (RFC 4217 explicit FTP-over-TLS) reuses __rt_ftp_open
            // with the _ftp_use_tls flag set; needs elephc-tls at link time.
            return super::ftps_stream::emit(args, emitter, ctx, data);
        }
        if path.starts_with("http://") {
            // An http:// URL is opened through the HTTP request runtime.
            return super::http_stream::emit(args, emitter, ctx, data);
        }
        if path.starts_with("https://") {
            // An https:// URL is opened through the TLS-secured HTTP runtime;
            // the checker has already flagged the program as needing
            // -lelephc_tls so the elephc-tls staticlib is linked in.
            return super::https_stream::emit(args, emitter, ctx, data);
        }
        if path.starts_with("compress.zlib://") {
            // compress.zlib:// wraps the underlying file with the zlib.inflate
            // read filter so reads see decompressed bytes.
            return super::compress_zlib_stream::emit(args, emitter, ctx, data);
        }
        if path.starts_with("compress.bzip2://") {
            // compress.bzip2:// slurps + bz2-decompresses the underlying file
            // through libbz2's BZ2_bzBuffToBuffDecompress, then dup2's a temp
            // fd carrying the decompressed bytes onto the original fd.
            return super::compress_bzip2_stream::emit(args, emitter, ctx, data);
        }
    }
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push filename ptr/len while the mode expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the mode pointer into the secondary runtime string-argument pair
            emitter.instruction("mov x4, x2");                                  // move the mode length into the secondary runtime string-argument pair
            emitter.instruction("stp x3, x4, [sp, #-16]!");                     // preserve the mode ptr/len while optional args are evaluated
            emit_ignored_optional_args(args, emitter, ctx, data);
            emitter.instruction("ldp x3, x4, [sp], #16");                       // restore the mode ptr/len after optional args
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the filename ptr/len after evaluating later arguments
            abi::emit_call_label(emitter, "__rt_fopen_maybe_phar");             // open the file (routes a non-literal phar:// read URL to the runtime phar reader, else __rt_fopen)
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the filename ptr/len while the mode expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the mode pointer into the x86_64 secondary runtime string-argument slot
            emitter.instruction("mov rsi, rdx");                                // move the mode length into the x86_64 secondary runtime string-argument slot
            abi::emit_push_reg_pair(emitter, "rdi", "rsi");                     // preserve the mode ptr/len while optional args are evaluated
            emit_ignored_optional_args(args, emitter, ctx, data);
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the mode ptr/len after optional args
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the filename ptr/len after evaluating later arguments
            abi::emit_call_label(emitter, "__rt_fopen_maybe_phar");             // open the file (routes a non-literal phar:// read URL to the runtime phar reader, else __rt_fopen)
        }
    }
    box_fopen_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Evaluates the mode argument and any currently-ignored optional fopen arguments.
pub(super) fn emit_mode_and_ignored_optional_args(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emit_expr(&args[1], emitter, ctx, data);
    emit_ignored_optional_args(args, emitter, ctx, data);
}

/// Evaluates fopen's optional 3rd/4th arguments in source order for side effects.
pub(super) fn emit_ignored_optional_args(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    for arg in &args[2..] {
        emit_expr(arg, emitter, ctx, data);
    }
}

/// Maps a `php://` standard-stream URL to its file descriptor. The `php://memory`
/// and `php://temp` streams are handled by [`is_php_memory_stream`]; `php://filter`
/// is handled elsewhere.
fn php_standard_stream_fd(path: &str) -> Option<i64> {
    match path {
        "php://stdin" | "php://input" => Some(0),
        "php://stdout" | "php://output" => Some(1),
        "php://stderr" => Some(2),
        _ => None,
    }
}

/// Recognizes the `php://memory` and `php://temp` in-memory stream URLs.
/// `php://temp` accepts an optional `/maxmemory:N` suffix, which elephc ignores
/// because the stream is always backed by an anonymous temp file.
fn is_php_memory_stream(path: &str) -> bool {
    path == "php://memory" || path == "php://temp" || path.starts_with("php://temp/")
}

/// Recognizes `php://fd/N` URLs and returns the embedded descriptor N.
/// The descriptor is treated as already open — elephc trusts the caller
/// to have prepared it through whatever side channel (e.g. an inherited
/// file descriptor from a parent process or a previously-opened
/// `dup`/`pipe` pair).
fn php_fd_stream(path: &str) -> Option<i64> {
    let suffix = path.strip_prefix("php://fd/")?;
    suffix.parse::<i64>().ok()
}

/// Boxes a well-known descriptor as a PHP stream `resource`.
fn emit_standard_stream_resource(fd: i64, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x1, #{}", fd));                   // payload = the standard-stream descriptor
            emitter.instruction("mov x2, #0");                                  // resource mixed payloads have no high word
            emitter.instruction("mov x0, #9");                                  // runtime tag 9 = resource
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov edi, {}", fd));                   // payload = the standard-stream descriptor
            emitter.instruction("xor esi, esi");                                // resource mixed payloads have no high word
            emitter.instruction("mov eax, 9");                                  // runtime tag 9 = resource
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
        }
    }
}

/// Boxes the fopen result: if `x0`/`rax` is negative, emits PHP false (tag 3, payload 0);
/// otherwise emits a PHP resource (tag 9, descriptor in low word). Uses `__rt_mixed_from_value`
/// via ABI calling convention.
pub(super) fn box_fopen_result(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("fopen_false");
    let done_label = ctx.next_label("fopen_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // did fopen() return a negative descriptor for failure?
            emitter.instruction(&format!("b.lt {}", false_label));              // box PHP false when opening the stream failed
            emitter.instruction("mov x1, x0");                                  // move the native stream descriptor into the mixed payload low word
            emitter.instruction("mov x2, #0");                                  // resource mixed payloads do not use a high word
            emitter.instruction("mov x0, #9");                                  // runtime tag 9 = resource
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the successful stream resource result
            emitter.instruction(&format!("b {}", done_label));                  // skip the false-boxing path after a successful open
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0 for fopen() failure
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads do not use a high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible fopen() failure semantics
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // did fopen() return a negative descriptor for failure?
            emitter.instruction(&format!("js {}", false_label));                // box PHP false when opening the stream failed
            emitter.instruction("mov rdi, rax");                                // move the native stream descriptor into the mixed payload low word
            emitter.instruction("xor esi, esi");                                // resource mixed payloads do not use a high word
            emitter.instruction("mov eax, 9");                                  // runtime tag 9 = resource
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the successful stream resource result
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false-boxing path after a successful open
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0 for fopen() failure
            emitter.instruction("xor esi, esi");                                // bool mixed payloads do not use a high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible fopen() failure semantics
            emitter.label(&done_label);
        }
    }
}
