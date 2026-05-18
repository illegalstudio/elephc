//! Purpose:
//! Emits PHP `preg_replace_callback` PCRE-style regex builtin calls.
//! Wires a statically known callback into the regex replacement runtime.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - The callback receives `array<string>` matches so untyped closure params
//!   must be specialized before deferred closure emission.

use crate::codegen::abi;
use crate::codegen::context::{Context, DeferredCallbackWrapper};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::super::callable_lookup::{lookup_function, FunctionLookup};

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("preg_replace_callback()");

    // -- evaluate pattern first, matching PHP source order --
    emit_expr(&args[0], emitter, ctx, data);
    let (string_ptr_reg, string_len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, string_ptr_reg, string_len_reg);

    // -- evaluate callback second and remember its address --
    let call_reg = abi::nested_call_reg(emitter);
    let captures = materialize_callback_address(&args[1], call_reg, emitter, ctx, data);
    specialize_recent_inline_callback(&args[1], ctx);
    abi::emit_push_reg(emitter, call_reg);

    // -- evaluate subject last --
    emit_expr(&args[2], emitter, ctx, data);

    match emitter.target.arch {
        Arch::AArch64 => {
            // -- stage runtime arguments away from helper scratch registers --
            abi::emit_push_reg_pair(emitter, "x1", "x2");
            abi::emit_pop_reg_pair(emitter, "x5", "x6");
            abi::emit_pop_reg(emitter, call_reg);
            abi::emit_pop_reg_pair(emitter, "x7", "x8");

            let env_bytes = materialize_capture_env(&captures, call_reg, "x3", "x4", emitter, ctx);
            emitter.instruction("mov x1, x7");                                  // pass pattern pointer to the regex callback runtime
            emitter.instruction("mov x2, x8");                                  // pass pattern length to the regex callback runtime
            emitter.instruction("bl __rt_preg_replace_callback");               // run regex replacement through the callback → x1=ptr, x2=len
            if env_bytes > 0 {
                abi::emit_release_temporary_stack(emitter, env_bytes);
            }
        }
        Arch::X86_64 => {
            // -- stage runtime arguments away from helper scratch registers --
            abi::emit_push_reg_pair(emitter, "rax", "rdx");
            abi::emit_pop_reg_pair(emitter, "r8", "r9");
            abi::emit_pop_reg(emitter, call_reg);
            abi::emit_pop_reg_pair(emitter, "r13", "r14");

            let env_bytes =
                materialize_capture_env(&captures, call_reg, "rdx", "rcx", emitter, ctx);
            emitter.instruction("mov rdi, r13");                                // pass pattern pointer to the regex callback runtime
            emitter.instruction("mov rsi, r14");                                // pass pattern length to the regex callback runtime
            abi::emit_call_label(emitter, "__rt_preg_replace_callback");        // run regex replacement through the callback → rax=ptr, rdx=len
            if env_bytes > 0 {
                abi::emit_release_temporary_stack(emitter, env_bytes);
            }
        }
    }

    Some(PhpType::Str)
}

fn preg_matches_type() -> PhpType {
    PhpType::Array(Box::new(PhpType::Str))
}

fn specialize_recent_inline_callback(callback: &Expr, ctx: &mut Context) {
    if !matches!(callback.kind, ExprKind::Closure { .. }) {
        return;
    }
    let Some(deferred) = ctx.deferred_closures.last_mut() else {
        return;
    };
    if let Some((_, ty)) = deferred.sig.params.first_mut() {
        *ty = preg_matches_type();
    }
    if let Some(declared) = deferred.sig.declared_params.first_mut() {
        *declared = false;
    }
}

fn materialize_callback_address(
    callback: &Expr,
    call_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<(String, PhpType, bool)> {
    match &callback.kind {
        ExprKind::StringLiteral(name) => {
            let resolved_name = match lookup_function(ctx, name) {
                Some(FunctionLookup::UserFunction(name))
                | Some(FunctionLookup::IncludeVariant(name)) => name,
                _ => name.clone(),
            };
            abi::emit_symbol_address(emitter, call_reg, &function_symbol(&resolved_name));
            Vec::new()
        }
        ExprKind::Variable(name) => {
            let var = ctx.variables.get(name).expect("undefined callback variable");
            abi::load_at_offset(emitter, call_reg, var.stack_offset);
            crate::codegen::callables::callable_captures(callback, ctx)
        }
        _ => {
            emit_expr(callback, emitter, ctx, data);
            emitter.instruction(&format!("mov {}, {}", call_reg, abi::int_result_reg(emitter))); // keep the evaluated callback address in the nested-call scratch register
            crate::codegen::callables::callable_captures(callback, ctx)
        }
    }
}

fn materialize_capture_env(
    captures: &[(String, PhpType, bool)],
    callback_reg: &str,
    runtime_callback_reg: &str,
    runtime_env_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> usize {
    if captures.is_empty() {
        emitter.instruction(&format!("mov {}, {}", runtime_callback_reg, callback_reg)); // pass the direct callback address to the regex runtime
        abi::emit_load_int_immediate(emitter, runtime_env_reg, 0);
        return 0;
    }

    let wrapper_label = ctx.next_label("callback_wrapper");
    ctx.deferred_callback_wrappers.push(DeferredCallbackWrapper {
        label: wrapper_label.clone(),
        visible_arg_types: vec![preg_matches_type()],
        capture_types: captures
            .iter()
            .map(|(_, ty, by_ref)| if *by_ref { PhpType::Int } else { ty.clone() })
            .collect(),
    });

    let env_bytes = (captures.len() + 1) * 16;
    abi::emit_reserve_temporary_stack(emitter, env_bytes);
    store_reg_to_env_slot(emitter, callback_reg, 0);
    for (idx, (capture_name, capture_ty, by_ref)) in captures.iter().enumerate() {
        emitter.comment(&format!("store preg_replace_callback capture ${}", capture_name));
        if *by_ref {
            if !crate::codegen::expr::calls::args::emit_ref_arg_variable_address(
                capture_name,
                "preg_replace_callback capture ref",
                emitter,
                ctx,
            ) {
                emitter.comment(&format!(
                    "WARNING: captured callback variable ${} not found",
                    capture_name
                ));
                continue;
            }
            store_current_result_to_env_slot(emitter, &PhpType::Int, (idx + 1) * 16);
        } else {
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
    }

    abi::emit_symbol_address(emitter, runtime_callback_reg, &wrapper_label);
    abi::emit_temporary_stack_address(emitter, runtime_env_reg, 0);
    env_bytes
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
