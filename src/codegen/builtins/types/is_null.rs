//! Purpose:
//! Emits PHP `is_null` type predicate calls.
//! Inspects static or boxed runtime value representation and returns a PHP boolean.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`.
//!
//! Key details:
//! - Predicate behavior must match PHP sentinel, Mixed tag, and object/interface layout conventions.

use crate::codegen::context::Context;
use crate::codegen::NULL_SENTINEL;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits PHP `is_null($value)` builtin call.
///
/// Inspects the runtime value of `args[0]` and sets the integer result register
/// to 1 (true) if the value is null, or 0 (false) otherwise.
///
/// For `Mixed` or `Union` types, peels nested mixed wrappers via `__rt_mixed_unbox`
/// before testing the null sentinel (runtime tag 8). For scalar types, directly
/// compares against the null sentinel (all-bits-set except LSB).
///
/// # Arguments
/// - `args[0]`: the expression to check for null
/// - `emitter`: assembly emitter
/// - `ctx`: codegen context (variable layout, ownership state)
/// - `data`: data section for relocations
///
/// # Returns
/// `Some(PhpType::Bool)` since the result is always a PHP boolean.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_null()");
    let ty = emit_expr(&args[0], emitter, ctx, data);

    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        // Mixed/Union values are boxed cells — peel nested mixed wrappers first
        abi::emit_call_label(emitter, "__rt_mixed_unbox");                      // normalize boxed mixed payloads to their concrete runtime tag
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #8");                              // runtime tag 8 = null
                emitter.instruction("cset x0, eq");                             // x0 = 1 if the unboxed payload is null, 0 otherwise
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rax, 8");                              // runtime tag 8 = null
                emitter.instruction("sete al");                                 // set al when the unboxed payload is null
                emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
            }
        }
    } else if matches!(ty, PhpType::TaggedScalar) {
        // Tagged scalars carry a runtime tag word — null means tag 8
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x1, #8");                              // runtime tag 8 means the tagged scalar is PHP null
                emitter.instruction("cset x0, eq");                             // x0 = 1 if the tagged scalar is null, 0 otherwise
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rdx, 8");                              // runtime tag 8 means the tagged scalar is PHP null
                emitter.instruction("sete al");                                 // set al when the tagged scalar is null
                emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
            }
        }
    } else if matches!(ty, PhpType::Int) && crate::codegen::sentinels::null_repr_is_tagged() {
        // Under the tagged representation a plain Int can never hold null
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    } else {
        // Scalar types — check directly against the null sentinel
        match emitter.target.arch {
            Arch::AArch64 => {
                let sentinel = NULL_SENTINEL as u64;
                emitter.instruction(&format!("movz x9, #0x{:X}", sentinel & 0xFFFF)); // load null sentinel bits [15:0]
                emitter.instruction(&format!("movk x9, #0x{:X}, lsl #16", (sentinel >> 16) & 0xFFFF)); // load null sentinel bits [31:16]
                emitter.instruction(&format!("movk x9, #0x{:X}, lsl #32", (sentinel >> 32) & 0xFFFF)); // load null sentinel bits [47:32]
                emitter.instruction(&format!("movk x9, #0x{:X}, lsl #48", (sentinel >> 48) & 0xFFFF)); // load null sentinel bits [63:48]
                emitter.instruction("cmp x0, x9");                              // compare value against null sentinel
                emitter.instruction("cset x0, eq");                             // x0 = 1 if value is null, 0 otherwise
            }
            Arch::X86_64 => {
                abi::emit_load_int_immediate(emitter, "r10", NULL_SENTINEL);
                emitter.instruction("cmp rax, r10");                            // compare value against the runtime null sentinel
                emitter.instruction("sete al");                                 // set al when the value is null
                emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
            }
        }
    }

    Some(PhpType::Bool)
}
