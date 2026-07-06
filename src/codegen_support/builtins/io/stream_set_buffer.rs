//! Purpose:
//! Emits PHP `stream_set_chunk_size` / `stream_set_read_buffer` /
//! `stream_set_write_buffer` calls.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - `stream_set_chunk_size($stream, $size): int` tracks a per-fd chunk size in
//!   the `_stream_chunk_size` table (indexed by raw fd up to 256, default 8192)
//!   and returns the PREVIOUS value — PHP's observable contract for save/restore
//!   patterns. Out-of-range / synthetic fds report the default and are not
//!   stored. The size does not currently change read granularity (reads return
//!   identical data); only the returned previous value is meaningful.
//! - `stream_set_read_buffer` / `stream_set_write_buffer` return `0` ("success").
//!   elephc streams are unbuffered (direct read/write syscalls), so the buffer
//!   size has no effect — `0` is the correct PHP result for an unbuffered stream
//!   (`stream_set_write_buffer($s, 0)` is exactly the unbuffered mode elephc uses).

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits codegen for PHP `stream_set_buffer()` stream and I/O builtin calls.
pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment(&format!("{}()", name));
    if name == "stream_set_chunk_size" && args.len() == 2 {
        return emit_chunk_size(args, emitter, ctx, data);
    }
    // stream_set_read_buffer / stream_set_write_buffer: evaluate args for side
    // effects and report success (0). elephc streams are unbuffered.
    for arg in args {
        emit_expr(arg, emitter, ctx, data);
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("mov x0, #0"),                     // return 0 (success — elephc streams are unbuffered)
        Arch::X86_64 => emitter.instruction("xor eax, eax"),                    // return 0 (success — elephc streams are unbuffered)
    }
    Some(PhpType::Int)
}

/// Emits `stream_set_chunk_size($stream, $size)`: store `$size` in the per-fd
/// `_stream_chunk_size` table and return the previous chunk size (default 8192).
/// Out-of-range fds report 8192 without storing.
fn emit_chunk_size(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let default_label = ctx.next_label("scs_default");
    let done_label = ctx.next_label("scs_done");
    // Unique label: a program may call stream_set_chunk_size more than once, so
    // a fixed label would be defined twice and fail to assemble.
    let have_old_label = ctx.next_label("scs_have_old");

    // -- fd from the stream arg, preserved across the size evaluation --
    emit_stream_fd_arg("stream_set_chunk_size", &args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                   // save fd across the $size evaluation
    emit_expr(&args[1], emitter, ctx, data);                                    // $size → result reg

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // x1 = new chunk size
            abi::emit_pop_reg(emitter, "x2");                                    // x2 = fd
            emitter.instruction("cmp x2, #0");                                  // negative fd?
            emitter.instruction(&format!("b.lt {}", default_label));            // → report the default without storing
            emitter.instruction("cmp x2, #256");                                // fd outside the per-fd table?
            emitter.instruction(&format!("b.ge {}", default_label));            // → report the default without storing
            abi::emit_symbol_address(emitter, "x9", "_stream_chunk_size");
            emitter.instruction("ldr x10, [x9, x2, lsl #3]");                   // x10 = previous chunk size (0 = unset)
            emitter.instruction(&format!("cbnz x10, {}", have_old_label));      // a stored value exists → use it
            emitter.instruction("mov x10, #8192");                              // unset → PHP default chunk size
            emitter.label(&have_old_label);
            emitter.instruction("str x1, [x9, x2, lsl #3]");                    // store the new chunk size for this fd
            emitter.instruction("mov x0, x10");                                 // return the previous chunk size
            emitter.instruction(&format!("b {}", done_label));                  // continue at target label
            emitter.label(&default_label);
            emitter.instruction("mov x0, #8192");                               // out-of-range fd → report the default chunk size
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // rsi = new chunk size
            abi::emit_pop_reg(emitter, "rdi");                                   // rdi = fd
            emitter.instruction("cmp rdi, 0");                                  // negative fd?
            emitter.instruction(&format!("jl {}", default_label));              // → report the default without storing
            emitter.instruction("cmp rdi, 256");                                // fd outside the per-fd table?
            emitter.instruction(&format!("jge {}", default_label));             // → report the default without storing
            abi::emit_symbol_address(emitter, "r9", "_stream_chunk_size");      // base of the per-fd chunk-size table
            emitter.instruction("mov rax, QWORD PTR [r9 + rdi * 8]");           // rax = previous chunk size (0 = unset)
            emitter.instruction("test rax, rax");                               // a stored value exists?
            emitter.instruction(&format!("jnz {}", have_old_label));            // → use it
            emitter.instruction("mov eax, 8192");                               // unset → PHP default chunk size
            emitter.label(&have_old_label);
            emitter.instruction("mov QWORD PTR [r9 + rdi * 8], rsi");           // store the new chunk size for this fd
            emitter.instruction(&format!("jmp {}", done_label));                // rax holds the previous chunk size
            emitter.label(&default_label);
            emitter.instruction("mov eax, 8192");                               // out-of-range fd → report the default chunk size
            emitter.label(&done_label);
        }
    }
    Some(PhpType::Int)
}
