//! Purpose:
//! Emits PHP `array_reduce` builtin calls that invoke user-provided callbacks.
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
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::callback_env;
use super::runtime_callable_array_callback;
use super::runtime_string_callback;

/// Emits the `array_reduce($input, $callback, $initial)` builtin call.
///
/// `args[0]` (array) is evaluated first, `args[1]` (callback) second, and
/// `args[2]` (initial) last — preserving PHP source evaluation order.
///
/// For non-capturing callbacks: pushes the array pointer to the temporary stack,
/// materializes the callback address, evaluates the initial value, then calls
/// `__rt_array_reduce` with registers set to [callback, array, initial, 0].
///
/// For capturing callbacks: recovers the array pointer, builds a capture
/// environment via `callback_env::emit_captured_callback_env` (which rewrites
/// the callback to a wrapper), then calls `__rt_array_reduce` with the wrapped
/// callback and environment pointer. Branch-shaped captured callable expressions
/// use descriptor-backed environments so runtime-selected receivers/captures are
/// preserved through the uniform invoker. Releases the temporary stack after the call.
///
/// # Returns
/// `Some(PhpType::Int)` — `array_reduce` always returns an integer in this compiler.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_reduce()");
    let call_reg = abi::nested_call_reg(emitter);
    let result_reg = abi::int_result_reg(emitter);
    let callback_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    let initial_arg_reg = abi::int_arg_reg_name(emitter.target, 2);
    let env_arg_reg = abi::int_arg_reg_name(emitter.target, 3);
    let source_elem_ty = match crate::codegen::functions::infer_contextual_type(&args[0], ctx) {
        PhpType::Array(elem_ty) => elem_ty.codegen_repr(),
        _ => PhpType::Int,
    };

    // -- evaluate the array argument, then the callback argument --
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, result_reg);                                    // push the source array pointer onto the temporary stack

    if runtime_string_callback::emit_after_saved_array(
        &args[1],
        None,
        vec![PhpType::Int, source_elem_ty.clone()],
        PhpType::Int,
        array_arg_reg,
        emitter,
        ctx,
        data,
        |wrapper, emitter, ctx, data| {
            // -- evaluate initial value (third arg) --
            emit_expr(&args[2], emitter, ctx, data);
            emitter.instruction(&format!("mov {}, {}", initial_arg_reg, result_reg)); //place the initial accumulator in the third runtime argument register

            callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
            abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
            callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
            abi::emit_call_label(emitter, "__rt_array_reduce");                 // call the callback-driven reduce runtime helper with a runtime string descriptor
        },
    ) {
        return Some(PhpType::Int);
    }

    if let Some(wrapper) = callback_env::emit_callable_array_descriptor_env_after_saved_array(
        &args[1],
        array_arg_reg,
        call_reg,
        vec![PhpType::Int, source_elem_ty.clone()],
        PhpType::Int,
        emitter,
        ctx,
        data,
    ) {
        // -- evaluate initial value (third arg) --
        emit_expr(&args[2], emitter, ctx, data);
        emitter.instruction(&format!("mov {}, {}", initial_arg_reg, result_reg)); //place the initial accumulator in the third runtime argument register

        callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
        abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
        callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
        abi::emit_call_label(emitter, "__rt_array_reduce");                     // call the callback-driven reduce runtime helper with a callable-array descriptor environment
        callback_env::release_descriptor_callback_env(&wrapper, emitter);
        return Some(PhpType::Int);
    }

    if runtime_callable_array_callback::emit_after_saved_array(
        &args[1],
        array_arg_reg,
        vec![PhpType::Int, source_elem_ty.clone()],
        PhpType::Int,
        emitter,
        ctx,
        data,
        |wrapper, emitter, ctx, data| {
            // -- evaluate initial value (third arg) --
            emit_expr(&args[2], emitter, ctx, data);
            emitter.instruction(&format!("mov {}, {}", initial_arg_reg, result_reg)); //place the initial accumulator in the third runtime argument register

            callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
            abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
            callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
            abi::emit_call_label(emitter, "__rt_array_reduce");                 // call the callback-driven reduce runtime helper with a runtime callable-array descriptor
        },
    ) {
        return Some(PhpType::Int);
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
            vec![PhpType::Int, source_elem_ty.clone()],
            PhpType::Int,
            emitter,
            ctx,
        )
        .expect("descriptor callback env support checked before emitting callback");

        // -- evaluate initial value (third arg) --
        emit_expr(&args[2], emitter, ctx, data);
        emitter.instruction(&format!("mov {}, {}", initial_arg_reg, result_reg)); //place the initial accumulator in the third runtime argument register

        callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
        abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
        callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
        abi::emit_call_label(emitter, "__rt_array_reduce");                     // call the callback-driven reduce runtime helper with a descriptor environment
        callback_env::release_descriptor_callback_env(&wrapper, emitter);
        return Some(PhpType::Int);
    }

    let captures =
        callback_env::materialize_callback_address(&args[1], call_reg, emitter, ctx, data);

    if !captures.is_empty() {
        abi::emit_pop_reg(emitter, result_reg);                                  // recover the source array pointer before building the capture environment
        let wrapper = callback_env::emit_captured_callback_env(
            call_reg,
            result_reg,
            &captures,
            vec![PhpType::Int, source_elem_ty],
            emitter,
            ctx,
        );

        // -- evaluate initial value (third arg) --
        emit_expr(&args[2], emitter, ctx, data);
        emitter.instruction(&format!("mov {}, {}", initial_arg_reg, result_reg)); //place the initial accumulator in the third runtime argument register

        callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
        abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
        callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
        abi::emit_call_label(emitter, "__rt_array_reduce");                     // call the callback-driven reduce runtime helper with a capture environment
        abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);
        return Some(PhpType::Int);
    }

    abi::emit_push_reg(emitter, call_reg);                                      // save the callback address across initial-value evaluation

    // -- evaluate initial value (third arg) --
    emit_expr(&args[2], emitter, ctx, data);
    emitter.instruction(&format!("mov {}, {}", initial_arg_reg, result_reg));   // place the initial accumulator in the third runtime argument register

    // -- place callback and array pointer into the runtime argument registers --
    abi::emit_pop_reg(emitter, callback_arg_reg);                                // restore the callback function address into the first runtime argument register
    abi::emit_pop_reg(emitter, array_arg_reg);                                   // pop the source array pointer into the second runtime argument register
    abi::emit_load_int_immediate(emitter, env_arg_reg, 0);
    abi::emit_call_label(emitter, "__rt_array_reduce");                         // call the callback-driven reduce runtime helper

    Some(PhpType::Int)
}
