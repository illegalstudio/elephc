//! Purpose:
//! Emits PHP `array_filter` builtin calls that invoke user-provided callbacks.
//! Owns callback argument materialization, result shape selection, and runtime helper calls.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Callback lowering must preserve PHP source evaluation order, captures, and callable return ownership.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::array_constants::ARRAY_INT_CONSTANTS;
use crate::types::PhpType;
use super::callback_env;
use super::runtime_callable_array_callback;
use super::runtime_string_callback;

/// Emits the `array_filter($array, $callback, $mode)` builtin call.
///
/// Evaluates arguments in PHP source order: array first, then callback. The array pointer
/// is saved to the temporary stack before callback materialization to preserve evaluation
/// order. The appropriate runtime helper is selected based on whether the array element
/// type requires refcounted payload handling.
///
/// # Arguments
/// - `_name`: Unused; dispatch is handled at the caller level.
/// - `args`: Two or three expressions — the input array, the callback, and optional mode.
/// - `emitter`: Target-aware instruction emitter.
/// - `ctx`: Codegen context carrying variable layout and ownership state.
/// - `data`: Mutable data section for relocations and constants.
///
/// # Returns
/// `Some(PhpType::Array(...))` with the element type preserved from the input array
/// if known, otherwise `PhpType::Array(Int)` as a safe default.
///
/// # ABI constraints
/// - Uses `nested_call_reg` for the callback address.
/// - Uses `int_result_reg` as a temporary to hold the array pointer during callback lowering.
/// - Pushes the array pointer before callback materialization and pops it after.
/// - On x86_64: uses `emit_call_label`; on ARM64: uses `bl` directly.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_filter()");
    let call_reg = abi::nested_call_reg(emitter);
    let result_reg = abi::int_result_reg(emitter);
    let callback_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(emitter.target, 2);
    let mode_arg_reg = abi::int_arg_reg_name(emitter.target, 3);
    let mode_expr = args.get(2);
    let static_mode = mode_expr.and_then(static_filter_mode_value);
    let has_dynamic_mode = mode_expr.is_some() && static_mode.is_none();

    // -- evaluate the array argument (first arg) --
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let uses_refcounted_runtime = filter_uses_payload_runtime(&arr_ty);
    let runtime_label = if uses_refcounted_runtime {
        "__rt_array_filter_refcounted"
    } else {
        "__rt_array_filter"
    };
    let mode_for_callback_shape = static_mode.unwrap_or(0);
    let visible_arg_types = filter_visible_arg_types(&arr_ty, mode_for_callback_shape);

    // -- save array pointer, then evaluate the callback argument --
    abi::emit_push_reg(emitter, result_reg);                                    // push the source array pointer onto the temporary stack

    if !has_dynamic_mode {
        if runtime_string_callback::emit_after_saved_array(
            &args[1],
            Some(&arr_ty),
            visible_arg_types.clone(),
            PhpType::Bool,
            array_arg_reg,
            emitter,
            ctx,
            data,
            |wrapper, emitter, _ctx, _data| {
                callback_env::load_env_slot_to_reg(
                    emitter,
                    array_arg_reg,
                    wrapper.array_slot_offset,
                );
                abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
                callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
                emit_static_filter_mode_arg(emitter, mode_arg_reg, mode_for_callback_shape);
                abi::emit_call_label(emitter, runtime_label);
            },
        ) {
            return filter_return_type(arr_ty);
        }

        if let Some(wrapper) = callback_env::emit_callable_array_descriptor_env_after_saved_array(
            &args[1],
            array_arg_reg,
            call_reg,
            visible_arg_types.clone(),
            PhpType::Bool,
            emitter,
            ctx,
            data,
        ) {
            callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
            abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
            callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
            emit_static_filter_mode_arg(emitter, mode_arg_reg, mode_for_callback_shape);
            abi::emit_call_label(emitter, runtime_label);
            callback_env::release_descriptor_callback_env(&wrapper, emitter);
            return filter_return_type(arr_ty);
        }

        if runtime_callable_array_callback::emit_after_saved_array(
            &args[1],
            array_arg_reg,
            visible_arg_types.clone(),
            PhpType::Bool,
            emitter,
            ctx,
            data,
            |wrapper, emitter, _ctx, _data| {
                callback_env::load_env_slot_to_reg(
                    emitter,
                    array_arg_reg,
                    wrapper.array_slot_offset,
                );
                abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
                callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
                emit_static_filter_mode_arg(emitter, mode_arg_reg, mode_for_callback_shape);
                abi::emit_call_label(emitter, runtime_label);
            },
        ) {
            return filter_return_type(arr_ty);
        }

        if callback_env::expr_call_needs_descriptor_callback_env(&args[1], ctx)
            && callback_env::descriptor_callback_env_supported(&args[1])
        {
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction(&format!("mov {}, {}", call_reg, result_reg));  // preserve the selected callable descriptor while recovering the source array
            abi::emit_pop_reg(emitter, array_arg_reg);                           // recover the source array pointer before building the descriptor environment
            emitter.instruction(&format!("mov {}, {}", result_reg, call_reg));  // restore the selected callable descriptor as the current result
            let wrapper = callback_env::emit_descriptor_callback_env_from_result(
                &args[1],
                array_arg_reg,
                visible_arg_types.clone(),
                PhpType::Bool,
                emitter,
                ctx,
            )
            .expect("descriptor callback env support checked before emitting callback");
            callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
            abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
            callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
            emit_static_filter_mode_arg(emitter, mode_arg_reg, mode_for_callback_shape);
            abi::emit_call_label(emitter, runtime_label);
            callback_env::release_descriptor_callback_env(&wrapper, emitter);
            return filter_return_type(arr_ty);
        }
    }

    let captures =
        callback_env::materialize_callback_address(&args[1], call_reg, emitter, ctx, data);

    // -- place callback and array pointer into the runtime argument registers --
    if captures.is_empty() {
        let dynamic_mode_loaded = if let Some(mode) = mode_expr.filter(|_| has_dynamic_mode) {
            abi::emit_push_reg(emitter, call_reg);                              // preserve callback address while evaluating the mode argument
            emit_expr(mode, emitter, ctx, data);
            abi::emit_pop_reg(emitter, call_reg);                               // restore callback address after mode evaluation
            true
        } else {
            false
        };
        abi::emit_pop_reg(emitter, array_arg_reg);                               // pop the source array pointer into the second runtime argument register
        if dynamic_mode_loaded {
            emitter.instruction(&format!("mov {}, {}", mode_arg_reg, result_reg)); // forward the runtime-computed mode to the filter helper
        } else {
            emit_static_filter_mode_arg(emitter, mode_arg_reg, mode_for_callback_shape);
        }
        emitter.instruction(&format!("mov {}, {}", callback_arg_reg, call_reg)); // move the callback function address into the first runtime argument register
        abi::emit_load_int_immediate(emitter, env_arg_reg, 0);
    } else {
        abi::emit_pop_reg(emitter, result_reg);                                  // recover the source array pointer before building the capture environment
        let wrapper = callback_env::emit_captured_callback_env(
            call_reg,
            result_reg,
            &captures,
            visible_arg_types,
            emitter,
            ctx,
        );
        let dynamic_mode_loaded = if let Some(mode) = mode_expr.filter(|_| has_dynamic_mode) {
            emit_expr(mode, emitter, ctx, data);
            true
        } else {
            false
        };
        if dynamic_mode_loaded {
            emitter.instruction(&format!("mov {}, {}", mode_arg_reg, result_reg)); // preserve the runtime-computed mode before loading callback runtime arguments
        } else {
            emit_static_filter_mode_arg(emitter, mode_arg_reg, mode_for_callback_shape);
        }
        callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
        abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
        callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
        abi::emit_call_label(emitter, runtime_label);
        abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);
        return filter_return_type(arr_ty);
    }

    if emitter.target.arch == Arch::X86_64 {
        abi::emit_call_label(emitter, runtime_label);                            // call the x86_64 callback-driven filter runtime helper
    } else {
        emitter.instruction(&format!("bl {}", runtime_label));                  // call the ARM64 callback-driven filter runtime helper
    }

    filter_return_type(arr_ty)
}

/// Returns the static integer value for a known `array_filter()` mode expression.
///
/// Recognizes integer literals and the predefined `ARRAY_FILTER_USE_*` constants.
fn static_filter_mode_value(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) => ARRAY_INT_CONSTANTS
            .iter()
            .find_map(|(constant, value)| (*constant == name.as_str()).then_some(*value)),
        _ => None,
    }
}

/// Builds the callback visible argument list for the selected `array_filter()` mode.
fn filter_visible_arg_types(arr_ty: &PhpType, mode: i64) -> Vec<PhpType> {
    match mode {
        1 => vec![filter_elem_type(arr_ty), PhpType::Int],
        2 => vec![PhpType::Int],
        _ => vec![filter_elem_type(arr_ty)],
    }
}

/// Loads a static `array_filter()` mode into the runtime helper's fourth argument register.
fn emit_static_filter_mode_arg(emitter: &mut Emitter, mode_arg_reg: &str, mode: i64) {
    abi::emit_load_int_immediate(emitter, mode_arg_reg, mode);
}

/// Returns the filtered array type, preserving known input element type when possible.
fn filter_return_type(arr_ty: PhpType) -> Option<PhpType> {
    match arr_ty {
        PhpType::Array(elem_ty) => Some(PhpType::Array(elem_ty)),
        _ => Some(PhpType::Array(Box::new(PhpType::Int))),
    }
}

/// Returns the element type to store in the capture environment for the filtered array.
///
/// Uses `codegen_repr()` so the environment slot reflects the actual lowered type rather
/// than the PHP-level type (e.g., `Str` becomes `Array(Int)` after array-of-strings encoding).
fn filter_elem_type(arr_ty: &PhpType) -> PhpType {
    match arr_ty {
        PhpType::Array(elem_ty) => elem_ty.codegen_repr(),
        _ => PhpType::Int,
    }
}

/// Returns `true` if the array element type requires the refcounted runtime helper.
///
/// An element type requires refcounted handling when its `codegen_repr()` is a string
/// (strings are refcounted in the runtime) or when `is_refcounted()` is true for the
/// inner type. This determines whether `__rt_array_filter_refcounted` or `__rt_array_filter`
/// is called.
fn filter_uses_payload_runtime(arr_ty: &PhpType) -> bool {
    matches!(
        &arr_ty,
        PhpType::Array(inner)
            if inner.is_refcounted() || matches!(inner.codegen_repr(), PhpType::Str)
    )
}
