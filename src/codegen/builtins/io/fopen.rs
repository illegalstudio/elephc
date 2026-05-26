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
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `fopen` builtin call, evaluating filename (arg[0]) and mode (arg[1]) in
/// source order before materializing arguments in ABI order for `__rt_fopen`. On success,
/// boxes the native file descriptor as a PHP resource (tag 9). On failure (negative
/// descriptor), boxes PHP false (tag 3) to distinguish from empty string or empty array.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fopen()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push filename ptr/len while the mode expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the mode pointer into the secondary runtime string-argument pair
            emitter.instruction("mov x4, x2");                                  // move the mode length into the secondary runtime string-argument pair
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the filename ptr/len after evaluating the mode expression
            abi::emit_call_label(emitter, "__rt_fopen");                        // open the file through the target-aware runtime helper
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the filename ptr/len while the mode expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the mode pointer into the x86_64 secondary runtime string-argument slot
            emitter.instruction("mov rsi, rdx");                                // move the mode length into the x86_64 secondary runtime string-argument slot
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the filename ptr/len after evaluating the mode expression
            abi::emit_call_label(emitter, "__rt_fopen");                        // open the file through the target-aware runtime helper
        }
    }
    box_fopen_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Boxes the fopen result: if `x0`/`rax` is negative, emits PHP false (tag 3, payload 0);
/// otherwise emits a PHP resource (tag 9, descriptor in low word). Uses `__rt_mixed_from_value`
/// via ABI calling convention.
fn box_fopen_result(emitter: &mut Emitter, ctx: &mut Context) {
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
