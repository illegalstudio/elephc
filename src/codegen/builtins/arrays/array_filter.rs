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
use crate::parser::ast::Expr;
use crate::types::PhpType;
use super::callback_env;
use super::runtime_callable_array_callback;
use super::runtime_string_callback;

/// Emits the `array_filter($array, $callback, $flag)` builtin call.
///
/// Evaluates arguments in PHP source order: array first, then callback. The array pointer
/// is saved to the temporary stack before callback materialization to preserve evaluation
/// order. The appropriate runtime helper is selected based on whether the array element
/// type requires refcounted payload handling.
///
/// # Arguments
/// - `_name`: Unused; dispatch is handled at the caller level.
/// - `args`: Exactly three expressions — the input array, the callback, and the optional flag.
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

    // -- evaluate the array argument (first arg) --
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let uses_refcounted_runtime = filter_uses_payload_runtime(&arr_ty);
    let runtime_label = if uses_refcounted_runtime {
        "__rt_array_filter_refcounted"
    } else {
        "__rt_array_filter"
    };

    // -- save array pointer, then evaluate the callback argument --
    abi::emit_push_reg(emitter, result_reg);                                    // push the source array pointer onto the temporary stack

    if runtime_string_callback::emit_after_saved_array(
        &args[1],
        Some(&arr_ty),
        vec![filter_elem_type(&arr_ty)],
        PhpType::Bool,
        array_arg_reg,
        emitter,
        ctx,
        data,
        |wrapper, emitter, _ctx, _data| {
            callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
            abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
            callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
            abi::emit_call_label(emitter, runtime_label);
        },
    ) {
        return match arr_ty {
            PhpType::Array(elem_ty) => Some(PhpType::Array(elem_ty)),
            _ => Some(PhpType::Array(Box::new(PhpType::Int))),
        };
    }

    if let Some(wrapper) = callback_env::emit_callable_array_descriptor_env_after_saved_array(
        &args[1],
        array_arg_reg,
        call_reg,
        vec![filter_elem_type(&arr_ty)],
        PhpType::Bool,
        emitter,
        ctx,
        data,
    ) {
        callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
        abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
        callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
        abi::emit_call_label(emitter, runtime_label);
        callback_env::release_descriptor_callback_env(&wrapper, emitter);
        return match arr_ty {
            PhpType::Array(elem_ty) => Some(PhpType::Array(elem_ty)),
            _ => Some(PhpType::Array(Box::new(PhpType::Int))),
        };
    }

    if runtime_callable_array_callback::emit_after_saved_array(
        &args[1],
        array_arg_reg,
        vec![filter_elem_type(&arr_ty)],
        PhpType::Bool,
        emitter,
        ctx,
        data,
        |wrapper, emitter, _ctx, _data| {
            callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
            abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
            callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
            abi::emit_call_label(emitter, runtime_label);
        },
    ) {
        return match arr_ty {
            PhpType::Array(elem_ty) => Some(PhpType::Array(elem_ty)),
            _ => Some(PhpType::Array(Box::new(PhpType::Int))),
        };
    }

    if callback_env::expr_call_needs_descriptor_callback_env(&args[1], ctx)
        && callback_env::descriptor_callback_env_supported(&args[1])
    {
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction(&format!("mov {}, {}", call_reg, result_reg));      // preserve the selected callable descriptor while recovering the source array
        abi::emit_pop_reg(emitter, array_arg_reg);                               // recover the source array pointer before building the descriptor environment
        emitter.instruction(&format!("mov {}, {}", result_reg, call_reg));      // restore the selected callable descriptor as the current result
        let wrapper = callback_env::emit_descriptor_callback_env_from_result(
            &args[1],
            array_arg_reg,
            vec![filter_elem_type(&arr_ty)],
            PhpType::Bool,
            emitter,
            ctx,
        )
        .expect("descriptor callback env support checked before emitting callback");
        callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
        abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
        callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
        abi::emit_call_label(emitter, runtime_label);
        callback_env::release_descriptor_callback_env(&wrapper, emitter);
        return match arr_ty {
            PhpType::Array(elem_ty) => Some(PhpType::Array(elem_ty)),
            _ => Some(PhpType::Array(Box::new(PhpType::Int))),
        };
    }

    let captures =
        callback_env::materialize_callback_address(&args[1], call_reg, emitter, ctx, data);

    // -- place callback and array pointer into the runtime argument registers --
    if captures.is_empty() {
        abi::emit_pop_reg(emitter, array_arg_reg);                               // pop the source array pointer into the second runtime argument register
        emitter.instruction(&format!("mov {}, {}", callback_arg_reg, call_reg)); // move the callback function address into the first runtime argument register
        abi::emit_load_int_immediate(emitter, env_arg_reg, 0);
    } else {
        abi::emit_pop_reg(emitter, result_reg);                                  // recover the source array pointer before building the capture environment
        let wrapper = callback_env::emit_captured_callback_env(
            call_reg,
            result_reg,
            &captures,
            vec![filter_elem_type(&arr_ty)],
            emitter,
            ctx,
        );
        callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
        abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
        callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
        abi::emit_call_label(emitter, runtime_label);
        abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);
        return match arr_ty {
            PhpType::Array(elem_ty) => Some(PhpType::Array(elem_ty)),
            _ => Some(PhpType::Array(Box::new(PhpType::Int))),
        };
    }

    if emitter.target.arch == Arch::X86_64 {
        abi::emit_call_label(emitter, runtime_label);                            // call the x86_64 callback-driven filter runtime helper
    } else {
        emitter.instruction(&format!("bl {}", runtime_label));                  // call the ARM64 callback-driven filter runtime helper
    }

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
