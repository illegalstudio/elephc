//! Purpose:
//! Emits PHP `fgetc` stream builtin calls.
//! Reads exactly one byte from a stream resource through the runtime helper.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - The runtime helper tail-calls `__rt_fread` with length = 1; length 0 is
//!   boxed as PHP `false` so EOF remains distinguishable from a byte string.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits code for the PHP `fgetc` builtin.
///
/// Reads exactly one byte from a stream resource via the `__rt_fgetc` runtime
/// helper. The result is boxed as `PhpType::Mixed` to accommodate PHP's return
/// type: a one-byte string on success, or `false` on EOF/read failure.
///
/// # Arguments
/// * `name` — builtin name (unused, reserved for future overload resolution)
/// * `args` — call arguments; `args[0]` must be a stream resource
/// * `emitter` — target for emitted instructions
/// * `ctx` — codegen context (label generation, target info)
/// * `data` — data section for relocations
///
/// # Returns
/// Always `Some(PhpType::Mixed)`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fgetc()");
    emit_stream_fd_arg("fgetc", &args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the file descriptor into the first SysV fread helper argument register
    }
    abi::emit_call_label(emitter, "__rt_fgetc");                                // call the runtime helper that reads exactly one byte before PHP result boxing
    box_fgetc_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Boxes the raw `fgetc` result into a `Mixed` runtime value.
///
/// After `__rt_fgetc` returns (x0/x1 = pointer/length, x2/rdx = byte count),
/// this function branches on whether a byte was read:
/// - **AArch64**: `x2 == 0` means EOF/failure → box `false`. Otherwise box a
///   one-byte string (`tag = 1`) via `__rt_mixed_from_value`.
/// - **x86_64**: `rdx == 0` means EOF/failure → box `false`. Otherwise `rax`
///   holds the pointer and `rdx` holds the length; box as string (`eax = 1`).
///
/// # Arguments
/// * `emitter` — target for emitted instructions
/// * `ctx` — codegen context (label generation)
fn box_fgetc_result(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("fgetc_false");
    let done_label = ctx.next_label("fgetc_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x2, #0");                                  // EOF or read failure has no byte to return
            emitter.instruction(&format!("b.le {}", false_label));              // box PHP false for EOF/read failure
            emitter.instruction("mov x0, #1");                                  // runtime tag 1 = string
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // persist and box the one-byte string
            emitter.instruction(&format!("b {}", done_label));                  // skip the false-boxing path
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0 for fgetc() EOF/failure
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads do not use a high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible EOF semantics
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rdx, 0");                                  // EOF or read failure has no byte to return
            emitter.instruction(&format!("jle {}", false_label));               // box PHP false for EOF/read failure
            emitter.instruction("mov rdi, rax");                                // move the one-byte string pointer into the mixed payload low word
            emitter.instruction("mov rsi, rdx");                                // move the one-byte string length into the mixed payload high word
            emitter.instruction("mov eax, 1");                                  // runtime tag 1 = string
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // persist and box the one-byte string
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false-boxing path
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0 for fgetc() EOF/failure
            emitter.instruction("xor esi, esi");                                // bool mixed payloads do not use a high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible EOF semantics
            emitter.label(&done_label);
        }
    }
}
