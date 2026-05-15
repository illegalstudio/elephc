//! Purpose:
//! Emits PHP `usort` builtin calls that invoke user-provided callbacks.
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
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("usort()");

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
    let is_closure = matches!(
        &args[1].kind,
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)
    );
    let mut inline_captures = Vec::new();
    if is_closure {
        emit_expr(&args[1], emitter, ctx, data);
        inline_captures = callback_env::callback_captures(&args[1], ctx);
        emitter.instruction(&format!("mov {}, {}", call_reg, result_reg));      // keep the resolved closure callback address in the nested-call scratch register
    } else if let ExprKind::Variable(var_name) = &args[1].kind {
        let var = ctx.variables.get(var_name).expect("undefined callback variable");
        let offset = var.stack_offset;
        abi::load_at_offset(emitter, call_reg, offset);                         // load the callback address from the variable slot into the nested-call scratch register
    } else {
        let func_name = match &args[1].kind {
            ExprKind::StringLiteral(name) => name.clone(),
            _ => panic!("usort() callback must be a string literal, callable expression, or callable variable"),
        };
        let label = function_symbol(&func_name);
        abi::emit_symbol_address(emitter, call_reg, &label);                    // materialize the comparator function address in the nested-call scratch register
    }
    let captures = if is_closure {
        inline_captures
    } else {
        callback_env::callback_captures(&args[1], ctx)
    };

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
    if is_closure {
        emitter.instruction(&format!("mov {}, {}", callback_arg_reg, result_reg)); // move the resolved closure callback address into the first runtime argument register
    } else if let ExprKind::Variable(var_name) = &args[1].kind {
        let var = ctx.variables.get(var_name).expect("undefined callback variable");
        abi::load_at_offset(emitter, callback_arg_reg, var.stack_offset);       // load the callback address from the variable slot into the first runtime argument register
    } else {
        let func_name = match &args[1].kind {
            ExprKind::StringLiteral(name) => name.clone(),
            _ => panic!("usort() callback must be a string literal, callable expression, or callable variable"),
        };
        let label = function_symbol(&func_name);
        abi::emit_symbol_address(emitter, callback_arg_reg, &label);            // materialize the comparator function address in the first runtime argument register
    }
    abi::emit_load_int_immediate(emitter, env_arg_reg, 0);
    abi::emit_call_label(emitter, "__rt_usort");                                // call the target-aware runtime helper that sorts the indexed array using the comparator callback

    Some(PhpType::Void)
}
