//! Purpose:
//! Emits PHP `is_float` type predicate calls.
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

/// Emits a PHP `is_float` type predicate call.
///
/// For `PhpType::Mixed` or `PhpType::Union`, unpacks the boxed mixed payload via
/// `__rt_mixed_unbox` and tests the runtime tag (2 = float). For all other types,
/// returns the compile-time predicate result directly.
///
/// Arguments:
///   args[0] — the expression to inspect
///
/// Outputs:
///   - Result register: 1 if the value is a float at runtime, 0 otherwise
///   - Return type: `PhpType::Bool`
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_float()");
    let ty = emit_expr(&args[0], emitter, ctx, data);

    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(emitter, "__rt_mixed_unbox");                      // normalize boxed mixed payloads to their concrete runtime tag
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #2");                              // runtime tag 2 = float payload
                emitter.instruction("cset x0, eq");                             // x0 = 1 if the unboxed payload is a float, 0 otherwise
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rax, 2");                              // runtime tag 2 = float payload
                emitter.instruction("sete al");                                 // set al when the unboxed payload is a float
                emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
            }
        }
    } else {
        let val = if matches!(ty, PhpType::Float) { 1 } else { 0 };
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), val); // return the compile-time type predicate result
    }
    Some(PhpType::Bool)
}
