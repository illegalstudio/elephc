//! Purpose:
//! Emits PHP `uksort` builtin calls that invoke user-provided callbacks.
//! Owns callback argument materialization, result shape selection, and runtime helper calls.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Callback lowering must preserve PHP source evaluation order, captures, and callable return ownership.

use crate::codegen::abi;
use super::callback_env;
use super::ensure_unique_arg::emit_ensure_unique_arg;
use super::store_mutating_arg::emit_store_mutating_arg;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the PHP `uksort` builtin call, which sorts an array by keys using a user-provided callback.
///
/// # Arguments
/// - `_name`: Unused name for dispatch (actual function is determined by catalog).
/// - `args`:[0] the array to sort, [1] the user callback callable.
/// - `emitter`: Assembly emitter for the current target.
/// - `ctx`: Codegen context (carries function metadata, variable layout).
/// - `data`: Data section for literals and runtime symbols.
///
/// # Returns
/// `Some(PhpType::Void)` since `uksort` has no return value.
///
/// # Behavior
/// 1. Evaluates the array argument and extracts its element type.
/// 2. Ensures array is unique (COW) and marks it as mutating.
/// 3. Preserves the array pointer on the stack while resolving the callback address.
/// 4. If the callback has captures: builds a captured wrapper environment, loads the array
///    slot and wrapper address into argument registers, calls `__rt_usort`.
/// 5. If no captures: restores the array pointer to the second argument register, moves the
///    resolved callback address to the first argument register, sets env=0, calls `__rt_usort`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("uksort()");

    // -- evaluate the array argument (first arg) --
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let elem_ty = match &arr_ty {
        PhpType::Array(elem_ty) => elem_ty.codegen_repr(),
        _ => PhpType::Int,
    };
    emit_ensure_unique_arg(emitter, &arr_ty);
    emit_store_mutating_arg(emitter, ctx, &args[0]);

    // -- save array pointer --
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the array pointer while the callback address is resolved for the target ABI

    // -- resolve callback function address --
    let call_reg = abi::nested_call_reg(emitter);
    let result_reg = abi::int_result_reg(emitter);
    let callback_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(emitter.target, 2);
    let captures =
        callback_env::materialize_callback_address(&args[1], call_reg, emitter, ctx, data);

    // -- call runtime: callback_addr + array_ptr --
    if !captures.is_empty() {
        abi::emit_pop_reg(emitter, result_reg);                                  // recover the array pointer before building the comparator capture environment
        let wrapper = callback_env::emit_captured_callback_env(
            call_reg,
            result_reg,
            &captures,
            vec![elem_ty.clone(), elem_ty],
            emitter,
            ctx,
        );
        callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
        abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
        callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
        abi::emit_call_label(emitter, "__rt_usort");                            // call the sort runtime helper with a captured comparator wrapper
        abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);
        return Some(PhpType::Void);
    }
    abi::emit_pop_reg(emitter, array_arg_reg);                                  // restore the array pointer into the second runtime argument register
    emitter.instruction(&format!("mov {}, {}", callback_arg_reg, call_reg));    // move the resolved comparator address into the first runtime argument register
    abi::emit_load_int_immediate(emitter, env_arg_reg, 0);
    abi::emit_call_label(emitter, "__rt_usort");                                // call the target-aware runtime helper that sorts the indexed array using the comparator callback

    Some(PhpType::Void)
}
