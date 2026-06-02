//! Purpose:
//! Emits PHP `array_udiff` and `array_uintersect` builtins (user-comparator difference/intersection).
//! Materializes both arrays and the comparator callback, then dispatches to the runtime helper.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Comparator is a two-argument callback (string / function / non-capturing closure); equal when `cmp(a, b) === 0`.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;
use super::callback_env;

/// Emits the PHP `array_udiff` / `array_uintersect` builtins.
///
/// `array_udiff($a, $b, $cmp)` returns the elements of `$a` not found in `$b`,
/// `array_uintersect($a, $b, $cmp)` the elements found in both, where two values are equal
/// when the two-argument comparator returns `0`. The result is an indexed array of the kept
/// elements (repacked at sequential indices, like `array_diff`).
///
/// Evaluates `$a`, then `$b`, then the comparator (PHP source order). The runtime helper
/// `__rt_array_udiff_uintersect` receives `(comparator, arr1, arr2, env, mode)` with mode
/// `0` (udiff) or `1` (uintersect).
///
/// Supports string, plain-function, and non-capturing closure comparators (the dominant
/// forms); capturing-closure comparators are not yet supported. Operates on indexed arrays
/// with scalar elements (consistent with `array_diff`).
///
/// # Returns
/// `Some(arr_ty)` — the first array's type (the kept elements share its element type).
pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let mode: i64 = if name == "array_uintersect" { 1 } else { 0 };
    emitter.comment(&format!("{}()", name));

    let call_reg = abi::nested_call_reg(emitter);
    let result_reg = abi::int_result_reg(emitter);
    let cmp_arg = abi::int_arg_reg_name(emitter.target, 0);
    let arr1_arg = abi::int_arg_reg_name(emitter.target, 1);
    let arr2_arg = abi::int_arg_reg_name(emitter.target, 2);
    let env_arg = abi::int_arg_reg_name(emitter.target, 3);
    let mode_arg = abi::int_arg_reg_name(emitter.target, 4);

    // -- evaluate the two arrays in source order, saving each on the temporary stack --
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, result_reg);                                     // save arr1 pointer onto the temporary stack
    emit_expr(&args[1], emitter, ctx, data);
    abi::emit_push_reg(emitter, result_reg);                                     // save arr2 pointer onto the temporary stack

    // -- resolve the comparator callback into the nested-call register --
    let _captures =
        callback_env::materialize_callback_address(&args[2], call_reg, emitter, ctx, data);

    // -- materialize arguments into the runtime registers (comparator, arr1, arr2, env, mode) --
    abi::emit_pop_reg(emitter, arr2_arg);                                        // pop arr2 into the third runtime argument register
    abi::emit_pop_reg(emitter, arr1_arg);                                        // pop arr1 into the second runtime argument register
    emitter.instruction(&format!("mov {}, {}", cmp_arg, call_reg));             // move the comparator address into the first runtime argument register
    abi::emit_load_int_immediate(emitter, env_arg, 0);
    abi::emit_load_int_immediate(emitter, mode_arg, mode);
    abi::emit_call_label(emitter, "__rt_array_udiff_uintersect");

    Some(arr_ty)
}
