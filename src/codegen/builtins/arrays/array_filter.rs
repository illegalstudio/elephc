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

    // -- save array pointer, then evaluate the callback argument --
    abi::emit_push_reg(emitter, result_reg);                                    // push the source array pointer onto the temporary stack

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
        let runtime_label = if uses_refcounted_runtime {
            "__rt_array_filter_refcounted"
        } else {
            "__rt_array_filter"
        };
        abi::emit_call_label(emitter, runtime_label);
        abi::emit_release_temporary_stack(emitter, wrapper.env_bytes);
        return match arr_ty {
            PhpType::Array(elem_ty) => Some(PhpType::Array(elem_ty)),
            _ => Some(PhpType::Array(Box::new(PhpType::Int))),
        };
    }

    let runtime_label = if uses_refcounted_runtime {
        "__rt_array_filter_refcounted"
    } else {
        "__rt_array_filter"
    };
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

fn filter_elem_type(arr_ty: &PhpType) -> PhpType {
    match arr_ty {
        PhpType::Array(elem_ty) => elem_ty.codegen_repr(),
        _ => PhpType::Int,
    }
}

fn filter_uses_payload_runtime(arr_ty: &PhpType) -> bool {
    matches!(
        &arr_ty,
        PhpType::Array(inner)
            if inner.is_refcounted() || matches!(inner.codegen_repr(), PhpType::Str)
    )
}
