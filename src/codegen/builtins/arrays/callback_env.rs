//! Purpose:
//! Builds callback capture environments used by array and dynamic-call builtins.
//! Owns hidden capture materialization and deferred wrapper metadata for emitted callbacks.
//!
//! Called from:
//! - Array callback builtins such as `array_map()`, `array_filter()`, `array_reduce()`, and sort/walk helpers.
//! - Dynamic-call builtins such as `call_user_func()` and `call_user_func_array()`.
//!
//! Key details:
//! - Capture slots must preserve source-call evaluation order and ABI argument layout for wrapper calls.

use crate::codegen::abi;
use crate::codegen::context::{Context, DeferredCallbackWrapper};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub(super) struct CallbackEnv {
    pub(super) wrapper_label: String,
    pub(super) env_bytes: usize,
    pub(super) array_slot_offset: usize,
}

pub(super) fn materialize_callback_address(
    callback: &Expr,
    call_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<(String, PhpType)> {
    match &callback.kind {
        ExprKind::StringLiteral(name) => {
            let label = function_symbol(name);
            abi::emit_symbol_address(emitter, call_reg, &label);
            Vec::new()
        }
        ExprKind::Variable(name) => {
            let var = ctx.variables.get(name).expect("undefined callback variable");
            abi::load_at_offset(emitter, call_reg, var.stack_offset);           // load the callback address from the callable variable slot
            crate::codegen::callables::callable_captures(callback, ctx)
        }
        _ => {
            emit_expr(callback, emitter, ctx, data);
            let result_reg = abi::int_result_reg(emitter);
            emitter.instruction(&format!("mov {}, {}", call_reg, result_reg));  // keep the evaluated callback address in the nested-call scratch register
            crate::codegen::callables::callable_captures(callback, ctx)
        }
    }
}

pub(super) fn push_captures_as_hidden_args(
    captures: &[(String, PhpType)],
    emitter: &mut Emitter,
    ctx: &Context,
    arg_types: &mut Vec<PhpType>,
) {
    for (capture_name, capture_ty) in captures {
        emitter.comment(&format!("push callback capture ${}", capture_name));
        let Some(capture_info) = ctx.variables.get(capture_name) else {
            emitter.comment(&format!(
                "WARNING: captured callback variable ${} not found",
                capture_name
            ));
            continue;
        };
        abi::emit_load(emitter, capture_ty, capture_info.stack_offset);
        crate::codegen::expr::calls::args::push_arg_value(emitter, capture_ty);
        arg_types.push(capture_ty.clone());
    }
}

pub(super) fn emit_captured_callback_env(
    callback_reg: &str,
    array_reg: &str,
    captures: &[(String, PhpType)],
    visible_arg_types: Vec<PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> CallbackEnv {
    let wrapper_label = ctx.next_label("callback_wrapper");
    ctx.deferred_callback_wrappers.push(DeferredCallbackWrapper {
        label: wrapper_label.clone(),
        visible_arg_types,
        capture_types: captures.iter().map(|(_, ty)| ty.clone()).collect(),
    });

    let env_slots = captures.len() + 2;
    let env_bytes = env_slots * 16;
    let array_slot_offset = (env_slots - 1) * 16;

    emitter.comment("callback capture environment");
    abi::emit_reserve_temporary_stack(emitter, env_bytes);
    store_reg_to_env_slot(emitter, callback_reg, 0);
    store_reg_to_env_slot(emitter, array_reg, array_slot_offset);

    for (idx, (capture_name, capture_ty)) in captures.iter().enumerate() {
        emitter.comment(&format!("store callback capture ${}", capture_name));
        let Some(capture_info) = ctx.variables.get(capture_name) else {
            emitter.comment(&format!(
                "WARNING: captured callback variable ${} not found",
                capture_name
            ));
            continue;
        };
        abi::emit_load(emitter, capture_ty, capture_info.stack_offset);
        store_current_result_to_env_slot(emitter, capture_ty, (idx + 1) * 16);
    }

    CallbackEnv {
        wrapper_label,
        env_bytes,
        array_slot_offset,
    }
}

pub(super) fn load_env_slot_to_reg(emitter: &mut Emitter, reg: &str, offset: usize) {
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_temporary_stack_address(emitter, scratch, offset);
    abi::emit_load_from_address(emitter, reg, scratch, 0);
}

pub(super) fn load_env_pointer_to_reg(emitter: &mut Emitter, reg: &str) {
    abi::emit_temporary_stack_address(emitter, reg, 0);
}

fn store_reg_to_env_slot(emitter: &mut Emitter, reg: &str, offset: usize) {
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_temporary_stack_address(emitter, scratch, offset);
    abi::emit_store_to_address(emitter, reg, scratch, 0);
}

fn store_current_result_to_env_slot(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_temporary_stack_address(emitter, scratch, offset);
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), scratch, 0);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_store_to_address(emitter, ptr_reg, scratch, 0);
            abi::emit_store_to_address(emitter, len_reg, scratch, 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), scratch, 0);
        }
    }
}
