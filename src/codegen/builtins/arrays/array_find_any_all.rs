//! Purpose:
//! Emits PHP `array_find`, `array_any`, and `array_all` (PHP 8.4) predicate builtins.
//! Resolves the predicate callback and dispatches to the unified `__rt_array_find_any_all` helper.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Reuses the array-callback machinery for string and closure/function callbacks; a mode selects find/any/all.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;
use super::callback_env;
use super::runtime_string_callback;

/// Emits the PHP 8.4 `array_find` / `array_any` / `array_all` predicate builtins.
///
/// Evaluates the array (first arg), then resolves the predicate callback. The unified
/// runtime helper `__rt_array_find_any_all` receives `(callback, array, env, mode)` where
/// mode is `0` (find — returns the first matching element or `null`), `1` (any — boolean),
/// or `2` (all — boolean).
///
/// Supports string callbacks (`"is_positive"`) and closures / plain function callbacks,
/// covering the dominant predicate usage. Operates on indexed arrays with scalar elements
/// (consistent with `array_filter`).
///
/// # Returns
/// `Some(PhpType::Mixed)` for `array_find` (element or null), `Some(PhpType::Bool)` for
/// `array_any` / `array_all`.
pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let mode: i64 = match name {
        "array_any" => 1,
        "array_all" => 2,
        _ => 0,
    };
    let ret_ty = if name == "array_find" {
        PhpType::Mixed
    } else {
        PhpType::Bool
    };
    emitter.comment(&format!("{}()", name));

    let call_reg = abi::nested_call_reg(emitter);
    let result_reg = abi::int_result_reg(emitter);
    let cb_arg = abi::int_arg_reg_name(emitter.target, 0);
    let arr_arg = abi::int_arg_reg_name(emitter.target, 1);
    let env_arg = abi::int_arg_reg_name(emitter.target, 2);
    let mode_arg = abi::int_arg_reg_name(emitter.target, 3);

    // -- evaluate the array argument, then the callback (PHP source order) --
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let elem_ty = match &arr_ty {
        PhpType::Array(elem) => elem.codegen_repr(),
        _ => PhpType::Int,
    };
    abi::emit_push_reg(emitter, result_reg);                                     // push the source array pointer onto the temporary stack

    // -- string callback path ("is_positive") --
    if runtime_string_callback::emit_after_saved_array(
        &args[1],
        Some(&arr_ty),
        vec![elem_ty.clone()],
        PhpType::Bool,
        arr_arg,
        emitter,
        ctx,
        data,
        |wrapper, emitter, _ctx, _data| {
            callback_env::load_env_slot_to_reg(emitter, arr_arg, wrapper.array_slot_offset);
            abi::emit_symbol_address(emitter, cb_arg, &wrapper.wrapper_label);
            callback_env::load_env_pointer_to_reg(emitter, env_arg);
            abi::emit_load_int_immediate(emitter, mode_arg, mode);
            abi::emit_call_label(emitter, "__rt_array_find_any_all");
        },
    ) {
        return Some(ret_ty);
    }

    // -- closure / plain function callback path --
    let captures =
        callback_env::materialize_callback_address(&args[1], call_reg, emitter, ctx, data);
    if captures.is_empty() {
        abi::emit_pop_reg(emitter, arr_arg);                                     // pop the source array pointer into the array argument register
        emitter.instruction(&format!("mov {}, {}", cb_arg, call_reg));          // move the callback function address into the callback argument register
        abi::emit_load_int_immediate(emitter, env_arg, 0);
        abi::emit_load_int_immediate(emitter, mode_arg, mode);
        abi::emit_call_label(emitter, "__rt_array_find_any_all");
    } else {
        abi::emit_pop_reg(emitter, result_reg);                                  // recover the source array pointer before building the capture environment
        let wrapper = callback_env::emit_captured_callback_env(
            call_reg,
            result_reg,
            &captures,
            vec![elem_ty],
            emitter,
            ctx,
        );
        callback_env::load_env_slot_to_reg(emitter, arr_arg, wrapper.array_slot_offset);
        abi::emit_symbol_address(emitter, cb_arg, &wrapper.wrapper_label);
        callback_env::load_env_pointer_to_reg(emitter, env_arg);
        abi::emit_load_int_immediate(emitter, mode_arg, mode);
        abi::emit_call_label(emitter, "__rt_array_find_any_all");
        abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);
    }

    Some(ret_ty)
}
