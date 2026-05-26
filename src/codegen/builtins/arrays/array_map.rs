//! Purpose:
//! Emits PHP `array_map` builtin calls that invoke user-provided callbacks.
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
use super::array_map_callback_returns_str::callback_returns_str;
use super::callback_env;

/// Emits the `array_map` builtin call.
///
/// Lowers `array_map(callback, array)` to a runtime helper call, selecting
/// between `__rt_array_map` (scalar results) and `__rt_array_map_str` (string
/// results) based on the inferred callback return type.
///
/// ## Evaluation order
/// The callback expression is evaluated first into `call_reg`, then the array
/// argument is evaluated. Both are pushed to the temporary stack before the
/// runtime call so they occupy the first two integer argument registers.
///
/// ## Capture handling
/// When `callback_env::materialize_callback_address` reports captures, a wrapper
/// environment is built on the temporary stack with the callback entry point in
/// slot 0, the array pointer in the last slot, and capture values in between.
/// The wrapper label address is passed as the first argument, the array pointer
/// as the second, and the environment pointer as the third.
///
/// ## Runtime helpers
/// - `__rt_array_map`: result array element type is `PhpType::Int`
/// - `__rt_array_map_str`: result array element type is `PhpType::Str`
///
/// Returns `Some(PhpType::Array(Box::new(element_type)))` where element type is
/// `Str` if `callback_returns_str` is true, otherwise `Int`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_map()");
    let call_reg = abi::nested_call_reg(emitter);
    let result_reg = abi::int_result_reg(emitter);
    let callback_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    let env_arg_reg = abi::int_arg_reg_name(emitter.target, 2);

    // -- determine callback return type at compile time --
    let returns_str = callback_returns_str(args, ctx);
    let source_elem_ty = match crate::codegen::functions::infer_contextual_type(&args[1], ctx) {
        PhpType::Array(elem_ty) => elem_ty.codegen_repr(),
        _ => PhpType::Int,
    };

    // -- evaluate the callback argument first, matching PHP source order --
    let captures =
        callback_env::materialize_callback_address(&args[0], call_reg, emitter, ctx, data);
    abi::emit_push_reg(emitter, call_reg);                                      // save the callback address across mapped-array evaluation

    // -- evaluate the array argument --
    let _arr_ty = emit_expr(&args[1], emitter, ctx, data);

    // -- save array pointer before preparing runtime arguments --
    abi::emit_push_reg(emitter, result_reg);                                    // push the array pointer onto the temporary stack

    if captures.is_empty() {
        abi::emit_pop_reg(emitter, array_arg_reg);                               // pop the mapped array pointer into the second runtime argument register
        abi::emit_pop_reg(emitter, callback_arg_reg);                            // pop the callback address into the first runtime argument register
        abi::emit_load_int_immediate(emitter, env_arg_reg, 0);
    } else {
        abi::emit_pop_reg(emitter, result_reg);                                  // recover the mapped array pointer before building the capture environment
        abi::emit_pop_reg(emitter, call_reg);                                    // recover the callback entry point for env slot zero
        let wrapper = callback_env::emit_captured_callback_env(
            call_reg,
            result_reg,
            &captures,
            vec![source_elem_ty.clone()],
            emitter,
            ctx,
        );
        callback_env::load_env_slot_to_reg(emitter, array_arg_reg, wrapper.array_slot_offset);
        abi::emit_symbol_address(emitter, callback_arg_reg, &wrapper.wrapper_label);
        callback_env::load_env_pointer_to_reg(emitter, env_arg_reg);
        if returns_str {
            abi::emit_call_label(emitter, "__rt_array_map_str");                // call the string-producing array_map runtime helper
            abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);
            return Some(PhpType::Array(Box::new(PhpType::Str)));
        }
        abi::emit_call_label(emitter, "__rt_array_map");                        // call the scalar array_map runtime helper
        abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);
        return Some(PhpType::Array(Box::new(PhpType::Int)));
    }

    if returns_str {
        abi::emit_call_label(emitter, "__rt_array_map_str");                    // call the string-producing array_map runtime helper
        Some(PhpType::Array(Box::new(PhpType::Str)))
    } else {
        abi::emit_call_label(emitter, "__rt_array_map");                        // call the scalar array_map runtime helper
        Some(PhpType::Array(Box::new(PhpType::Int)))
    }
}
