//! Purpose:
//! Emits PHP `stream_resolve_include_path($filename)` calls. Resolves a
//! filename through PHP's `include_path` and returns the resolved path,
//! or false if the file does not exist on any include_path entry.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - elephc has no runtime `include_path` (includes are pre-resolved at
//!   compile time), so this builtin is functionally equivalent to
//!   `realpath($filename)`: if the path resolves on disk, return its
//!   canonical form; otherwise return Mixed(false).
//! - Return type is `Mixed` because PHP's contract is `string|false`.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_resolve_include_path()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_resolve_include_path()");
    // Evaluate the filename arg → string result (x1/x2 on ARM64, rax/rdx on x86_64).
    emit_expr(&args[0], emitter, ctx, data);
    let is_false = ctx.next_label("srip_false");
    let done = ctx.next_label("srip_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(emitter, "__rt_realpath");                     // x1/x2 = canonical path or empty for false
            emitter.instruction(&format!("cbz x2, {}", is_false));              // len 0 → false
            // Mixed(string)
            emitter.instruction("mov x0, #1");                                  // tag = string
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.instruction(&format!("b {}", done));                        // continue at target label
            emitter.label(&is_false);
            emitter.instruction("mov x0, #3");                                  // tag = bool
            emitter.instruction("mov x1, #0");                                  // value = false
            emitter.instruction("mov x2, #0");                                  // prepare AArch64 call argument
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done);
        }
        Arch::X86_64 => {
            // String-result pair is in rax/rdx; realpath helper takes the
            // same pair as input on x86_64 too.
            abi::emit_call_label(emitter, "__rt_realpath");                     // rax/rdx = canonical or empty
            emitter.instruction("test rdx, rdx");                               // check whether the runtime value is zero
            emitter.instruction(&format!("jz {}", is_false));                   // branch when the checked value is zero or equal
            // Mixed(string): __rt_mixed_from_value takes (rax=tag, rdi=lo, rsi=hi).
            emitter.instruction("mov rdi, rax");                                // string ptr → payload lo
            emitter.instruction("mov rsi, rdx");                                // string len → payload hi
            emitter.instruction("mov rax, 1");                                  // tag = string
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.instruction(&format!("jmp {}", done));                      // continue at target label
            emitter.label(&is_false);
            emitter.instruction("xor edi, edi");                                // clear register value
            emitter.instruction("xor esi, esi");                                // clear register value
            emitter.instruction("mov rax, 3");                                  // tag = bool, value 0 = false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done);
        }
    }
    Some(PhpType::Mixed)
}
