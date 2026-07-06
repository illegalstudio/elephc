//! Purpose:
//! Lowers `fopen()` calls whose path is a `compress.zlib://` URL.
//! Opens the underlying file in read-only mode and immediately attaches the
//! `zlib.inflate` filter logic so subsequent reads see decompressed bytes.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::fopen::emit()` when the path literal
//!   begins with `compress.zlib://`.
//!
//! Key details:
//! - The URL must be a string literal; the prefix is stripped at compile time
//!   and the underlying path is opened with mode "r" through `__rt_fopen`.
//! - On `fopen` failure (`fd < 0`) the wrapper short-circuits to PHP `false`
//!   without invoking the inflate logic, matching `compress.zlib://`'s behavior
//!   when the underlying file is missing or unreadable.
//! - The inflate emitter ends with the filtered descriptor already re-boxed as
//!   a resource Mixed cell, so the wrapper does not call `box_fopen_result`
//!   again on the success path.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits a `fopen("compress.zlib://...", ...)` call. The path is known to be a
/// string literal beginning with `compress.zlib://`.
pub fn emit(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fopen() compress.zlib:// stream");
    let underlying = match &args[0].kind {
        ExprKind::StringLiteral(path) => path.strip_prefix("compress.zlib://").map(str::to_string),
        _ => None,
    };
    super::fopen::emit_mode_and_ignored_optional_args(args, emitter, ctx, data);
    let underlying = match underlying {
        Some(p) if !p.is_empty() => p,
        _ => {
            // Unparseable or empty path lowers to PHP false.
            match emitter.target.arch {
                Arch::AArch64 => emitter.instruction("mov x0, #-1"),            // negative fd sentinel for PHP false
                Arch::X86_64 => emitter.instruction("mov rax, -1"),             // negative fd sentinel for PHP false
            }
            super::fopen::box_fopen_result(emitter, ctx);
            return Some(PhpType::Mixed);
        }
    };

    // Materialize the stripped path and "r" mode into the runtime's string-arg
    // registers before calling __rt_fopen.
    let (path_sym, path_len) = data.add_string(underlying.as_bytes());
    let (mode_sym, mode_len) = data.add_string(b"r");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x1", &path_sym);
            emitter.instruction(&format!("mov x2, #{}", path_len));             // path length
            abi::emit_symbol_address(emitter, "x3", &mode_sym);
            emitter.instruction(&format!("mov x4, #{}", mode_len));             // mode length
            abi::emit_call_label(emitter, "__rt_fopen");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rax", &path_sym);
            emitter.instruction(&format!("mov rdx, {}", path_len));             // path length
            abi::emit_symbol_address(emitter, "rdi", &mode_sym);
            emitter.instruction(&format!("mov rsi, {}", mode_len));             // mode length
            abi::emit_call_label(emitter, "__rt_fopen");
        }
    }

    // Branch on fopen failure: negative fd → box false, skip inflate.
    let false_label = ctx.next_label("czlib_false");
    let done_label = ctx.next_label("czlib_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // negative fd = open failed
            emitter.instruction(&format!("b.lt {}", false_label));              // box false when the source open failed
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // negative fd = open failed
            emitter.instruction(&format!("js {}", false_label));                // sign bit set = negative fd
        }
    }
    // Attach inflate; this returns x0/rax = Mixed-boxed resource.
    match emitter.target.arch {
        Arch::AArch64 => super::stream_filter_inflate::emit_arm64(emitter, |prefix| ctx.next_label(prefix)),
        Arch::X86_64 => super::stream_filter_inflate::emit_x86_64(emitter, |prefix| ctx.next_label(prefix)),
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b {}", done_label)),     // skip false boxing after attaching inflate
        Arch::X86_64 => emitter.instruction(&format!("jmp {}", done_label)),    // skip false boxing after attaching inflate
    }
    emitter.label(&false_label);
    super::fopen::box_fopen_result(emitter, ctx);                                   // boxes false (fd < 0)
    emitter.label(&done_label);
    Some(PhpType::Mixed)
}
