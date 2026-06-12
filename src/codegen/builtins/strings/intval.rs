//! Purpose:
//! Emits PHP `intval` conversion calls from scalar expressions.
//! Keeps PHP conversion lowering close to string builtins because string parsing is the dominant path.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Conversion behavior must stay aligned with type-checker assumptions for scalar-to-int coercion.

use crate::codegen::context::Context;
use crate::codegen::context::HeapOwnership;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{emit_expr, expr_result_heap_ownership};
use crate::codegen::abi;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::PhpType;

/// Emits code for the PHP `intval()` builtin.
///
/// Dispatches on the argument type:
/// - `Str`: calls `__rt_str_to_int` to parse the string with PHP cast rules
/// - `Mixed`/`Union`: calls `__rt_mixed_cast_int` for runtime type coercion
/// - `Float`: truncates the floating-point result into the integer register (toward zero), matching
///   the `(int)` cast — without this the raw IEEE-754 bits would be returned as a bogus integer
/// - Other scalar types (`Int`/`Bool`/`Null`): no-op (already in the integer register)
///
/// Returns `PhpType::Int` unconditionally, matching PHP's `intval()` return type.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("intval()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    match ty {
        PhpType::Str => {
            // -- convert string to integer --
            abi::emit_call_label(emitter, "__rt_str_to_int");                   // parse the current string result through PHP string-to-int cast rules
        }
        PhpType::Mixed | PhpType::Union(_) => {
            // -- coerce a boxed Mixed cell to int per PHP's casting rules --
            let release_arg_after_cast = mixed_arg_result_is_owned(&args[0]);
            if release_arg_after_cast {
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
            }
            abi::emit_call_label(emitter, "__rt_mixed_cast_int");                // dispatch on the runtime cell tag and return the integer payload (or coerced equivalent)
            if release_arg_after_cast {
                release_preserved_mixed_arg_after_int_cast(emitter);
            }
        }
        PhpType::Float => {
            // -- truncate the float result toward zero into the integer register (like the `(int)`
            // cast); otherwise the raw IEEE-754 bits would be reinterpreted as a bogus integer --
            abi::emit_float_result_to_int_result(emitter);
        }
        _ => {}
    }
    Some(PhpType::Int)
}

/// Returns true if the expression result is heap-owned and must be preserved
/// across the `__rt_mixed_cast_int` call.
///
/// Arithmetic binary operations are included because their result may alias
/// argument temporaries that the runtime call could otherwise clobber.
fn mixed_arg_result_is_owned(arg: &Expr) -> bool {
    expr_result_heap_ownership(arg) == HeapOwnership::Owned
        || matches!(
            arg.kind,
            ExprKind::BinaryOp {
                op: BinOp::Add | BinOp::Sub | BinOp::Mul,
                ..
            }
        )
}

/// Restores the preserved `Mixed` argument after a `__rt_mixed_cast_int` call.
///
/// The integer result was pushed onto the stack before the call to protect it
/// from being clobbered. This function decrefs the original `Mixed` cell and
/// pops the preserved integer back into the result register.
fn release_preserved_mixed_arg_after_int_cast(emitter: &mut Emitter) {
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_release_temporary_stack(emitter, 16);
}
