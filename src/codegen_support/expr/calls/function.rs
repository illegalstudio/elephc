//! Purpose:
//! Lowers direct user-defined and builtin function calls.
//! Resolves the callable shape, prepares arguments, and leaves the call result for expression consumers.
//!
//! Called from:
//! - `crate::codegen_support::expr::calls`
//!
//! Key details:
//! - Callable metadata and argument signatures must stay synchronized with type checking and runtime dispatch.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

use super::args;

/// Emits a direct user-defined or builtin function call.
///
/// Saves concat offset on x86_64 before nested calls, prepares arguments via
/// `emit_pushed_call_args`, materializes outgoing ABI arguments, emits the call
/// label, and restores concat offset for string returns. Returns the function's
/// return type from the signature lookup, or `PhpType::Void` if not found.
pub(super) fn emit_function_call(
    name: &str,
    args_exprs: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("call {}()", name));

    let save_concat_before_args =
        emitter.target.arch == crate::codegen_support::platform::Arch::X86_64;
    if save_concat_before_args {
        super::super::save_concat_offset_before_nested_call(emitter, ctx);
    }

    let sig = ctx.functions.get(name).cloned();
    if let Some(sig) = sig.as_ref() {
        specialize_callable_arguments(name, args_exprs, sig, ctx);
    }
    let emitted_args = args::emit_pushed_call_args(
        args_exprs,
        sig.as_ref(),
        args::regular_param_count(sig.as_ref(), args_exprs.len()),
        "ref arg",
        false,
        true,
        emitter,
        ctx,
        data,
    );
    let arg_types = emitted_args.arg_types;

    let assignments =
        crate::codegen_support::abi::build_outgoing_arg_assignments_for_target(emitter.target, &arg_types, 0);
    let overflow_bytes = crate::codegen_support::abi::materialize_outgoing_args(emitter, &assignments);

    let ret_ty = ctx
        .functions
        .get(name)
        .map(|sig| sig.return_type.clone())
        .unwrap_or(PhpType::Void);

    if !save_concat_before_args {
        super::super::save_concat_offset_before_nested_call(emitter, ctx);
    }
    crate::codegen_support::abi::emit_call_label(emitter, &function_symbol(name));
    if save_concat_before_args {
        crate::codegen_support::abi::emit_release_temporary_stack(emitter, overflow_bytes);
        crate::codegen_support::abi::emit_release_temporary_stack(emitter, emitted_args.source_temp_bytes);
        if ret_ty == PhpType::Str {
            super::super::restore_concat_offset_after_owned_string_call(emitter, ctx);
        } else {
            super::super::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
        }
    } else {
        if ret_ty == PhpType::Str {
            super::super::restore_concat_offset_after_owned_string_call(emitter, ctx);
        } else {
            super::super::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
        }
        crate::codegen_support::abi::emit_release_temporary_stack(emitter, overflow_bytes);
        crate::codegen_support::abi::emit_release_temporary_stack(emitter, emitted_args.source_temp_bytes);
    }

    ret_ty
}

/// For each argument whose corresponding parameter has type `PhpType::Callable`, looks
/// up the concrete callable signature from `callable_param_sigs` and registers it on
/// the argument expression tree so the callee can be specialized at emit time.
///
/// Iterates positional and named arguments, skipping spreads, maps them to
/// parameter indices, and calls `specialize_callable_expr` for eligible arguments.
fn specialize_callable_arguments(
    function_name: &str,
    args_exprs: &[Expr],
    sig: &FunctionSig,
    ctx: &mut Context,
) {
    let mut positional_idx = 0usize;
    for arg in args_exprs {
        let (param_idx, value) = match &arg.kind {
            ExprKind::NamedArg { name, value } => {
                let Some(param_idx) = sig
                    .params
                    .iter()
                    .position(|(param_name, _)| param_name == name)
                else {
                    continue;
                };
                (param_idx, value.as_ref())
            }
            ExprKind::Spread(_) => {
                continue;
            }
            _ => {
                let param_idx = positional_idx;
                positional_idx += 1;
                (param_idx, arg)
            }
        };
        let Some((param_name, param_ty)) = sig.params.get(param_idx) else {
            continue;
        };
        if param_ty != &PhpType::Callable {
            continue;
        }
        if let Some(callable_sig) = ctx
            .callable_param_sigs
            .get(&(function_name.to_string(), param_name.clone()))
            .cloned()
        {
            specialize_callable_expr(value, &callable_sig, ctx);
        }
    }
}

/// Recursively traverses `expr` to find the storage location that will hold a
/// callable and delegates to `specialize_callable_var`.
///
/// Handles `Variable`, `ArrayAccess` (when the array is a `Variable`), and
/// `Assignment` (traverses the value side). Other expression kinds are ignored.
fn specialize_callable_expr(expr: &Expr, callable_sig: &FunctionSig, ctx: &mut Context) {
    match &expr.kind {
        ExprKind::Variable(name) => specialize_callable_var(name, callable_sig, ctx),
        ExprKind::ArrayAccess { array, .. } => {
            if let ExprKind::Variable(name) = &array.kind {
                specialize_callable_var(name, callable_sig, ctx);
            }
        }
        ExprKind::Assignment { value, .. } => specialize_callable_expr(value, callable_sig, ctx),
        _ => {}
    }
}

/// Associates `callable_sig` with the variable `name` in `ctx.closure_sigs`, then
/// updates any previously deferred closures with matching parameter signatures and
/// closure captures to use the new signature.
///
/// This propagates callable type information forward through deferred closure
/// bodies that were already queued before the concrete signature was known.
fn specialize_callable_var(name: &str, callable_sig: &FunctionSig, ctx: &mut Context) {
    let previous_sig = ctx
        .closure_sigs
        .insert(name.to_string(), callable_sig.clone());
    let Some(previous_sig) = previous_sig else {
        return;
    };
    let captures = ctx.closure_captures.get(name).cloned().unwrap_or_default();
    for deferred in ctx.deferred_closures.iter_mut().rev() {
        if deferred.sig.params == previous_sig.params && deferred.captures == captures {
            deferred.sig = callable_sig.clone();
            break;
        }
    }
}
