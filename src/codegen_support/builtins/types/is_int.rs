//! Purpose:
//! Emits PHP `is_int` type predicate calls.
//! Inspects static or boxed runtime value representation and returns a PHP boolean.
//!
//! Called from:
//! - `crate::codegen_support::builtins::types::emit()`.
//!
//! Key details:
//! - Predicate behavior must match PHP sentinel, Mixed tag, and object/interface layout conventions.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a PHP `is_int` type predicate call.
///
/// For `PhpType::Mixed` or `PhpType::Union`, unpacks the boxed mixed payload via
/// `__rt_mixed_unbox` and tests the runtime tag (0 = integer). For all other types,
/// returns the compile-time predicate result directly.
///
/// Arguments:
///   args[0] — the expression to inspect
///
/// Outputs:
///   - Result register: 1 if the value is an integer at runtime, 0 otherwise
///   - Return type: `PhpType::Bool`
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_int()");
    let ty = emit_expr(&args[0], emitter, ctx, data);

    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(emitter, "__rt_mixed_unbox");                      // normalize boxed mixed payloads to their concrete runtime tag
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #0");                              // runtime tag 0 = integer payload
                emitter.instruction("cset x0, eq");                             // x0 = 1 if the unboxed payload is an int, 0 otherwise
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rax, 0");                              // runtime tag 0 = integer payload
                emitter.instruction("sete al");                                 // set al when the unboxed payload is an int
                emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
            }
        }
    } else {
        let val = if matches!(ty, PhpType::Int) { 1 } else { 0 };
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), val); // return the compile-time type predicate result
    }
    Some(PhpType::Bool)
}
