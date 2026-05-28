//! Purpose:
//! Lowers top-level call-argument emission from prepared semantic plans.
//! Converts evaluated PHP argument expressions into temporary values ready for ABI assignment.
//!
//! Called from:
//! - `crate::codegen::expr::calls::args`
//!
//! Key details:
//! - Argument checks must happen at PHP-observable points without skipping later side effects.

use crate::codegen::emit::Emitter;
use crate::codegen::{context::Context, data_section::DataSection};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

use super::common::{
    call_target_ty, emit_ref_arg_variable_address, push_arg_value,
    push_expr_arg, push_non_variable_ref_arg_address,
};
use super::named;
use super::normalize::{has_named_args, prepare_call_args};
use super::spread::{emit_spread_into_named_params, emit_spread_tail_variadic_array_arg};
use super::variadic::{emit_empty_variadic_array_arg, emit_variadic_array_arg_from_exprs};
use super::EmittedCallArgs;

/// Emits all call arguments from an expression list, handling named args, spreads, and variadic params.
///
/// Dispatches to `named::emit_source_order_named_call_args` when named arguments are present and a
/// signature is available. Otherwise normalizes arguments via `prepare_call_args` and emits them
/// as regular positional arguments, handling by-ref parameters and building variadic arrays as needed.
///
/// Returns `EmittedCallArgs` containing the collected argument types. The `source_temp_bytes` field
/// is always zero here; it is populated by the caller for source-level temp tracking.
///
/// # Parameters
/// - `args_exprs`: Raw argument expressions from the PHP call site.
/// - `sig`: Function signature when known; `None` forces positional-only path.
/// - `regular_param_count`: Number of caller-visible regular (non-variadic) parameters.
/// - `ref_arg_context_label`: Label for ref-arg address emission diagnostics.
/// - `retain_non_variable_ref_args`: Whether to retain addresses for non-variable ref args.
/// - `coerce_inferred_params`: Whether to coerce arguments to inferred parameter types.
/// - `emitter`/`ctx`/`data`: Codegen state passed through to sub-emitters.
pub(crate) fn emit_pushed_call_args(
    args_exprs: &[Expr],
    sig: Option<&FunctionSig>,
    regular_param_count: usize,
    ref_arg_context_label: &str,
    retain_non_variable_ref_args: bool,
    coerce_inferred_params: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> EmittedCallArgs {
    let has_named = has_named_args(args_exprs);

    if let Some(sig) = sig {
        if has_named {
            return named::emit_source_order_named_call_args(
                args_exprs,
                sig,
                regular_param_count,
                ref_arg_context_label,
                retain_non_variable_ref_args,
                emitter,
                ctx,
                data,
            );
        }
    }

    debug_assert!(
        sig.is_some() || !has_named,
        "codegen reached named-arg call without a known signature; checker should have rejected this"
    );

    let prepared = prepare_call_args(sig, args_exprs, regular_param_count);
    let mut arg_types = emit_pushed_non_variadic_args(
        &prepared.all_args,
        sig,
        ref_arg_context_label,
        retain_non_variable_ref_args,
        coerce_inferred_params,
        emitter,
        ctx,
        data,
    );

    if prepared.spread_into_named {
        if let Some(spread_expr) = prepared.spread_arg.as_ref() {
            emit_spread_into_named_params(
                spread_expr,
                sig,
                prepared.spread_at_index,
                prepared.regular_param_count,
                "named params",
                emitter,
                ctx,
                data,
                &mut arg_types,
            );
        }
    }

    if prepared.is_variadic {
        if let Some(spread_expr) = prepared.spread_arg.as_ref() {
            let tail_start = prepared
                .regular_param_count
                .saturating_sub(prepared.spread_at_index);
            let variadic_ty = emit_spread_tail_variadic_array_arg(
                spread_expr,
                sig,
                tail_start,
                prepared.regular_param_count,
                "spread tail as variadic param",
                emitter,
                ctx,
                data,
            );
            arg_types.push(variadic_ty);
        } else if prepared.variadic_args.is_empty() {
            arg_types.push(emit_empty_variadic_array_arg("empty variadic array", emitter));
        } else {
            let variadic_ty = emit_variadic_array_arg_from_exprs(
                &prepared.variadic_args,
                "build variadic array",
                true,
                true,
                emitter,
                ctx,
                data,
            );
            arg_types.push(variadic_ty);
        }
    }

    EmittedCallArgs {
        arg_types,
        source_temp_bytes: 0,
    }
}

/// Emits regular (non-variadic) call arguments from a prepared argument list.
///
/// Iterates over `all_args` and emits each argument according to its role:
/// - **By-ref parameters** (`is_ref=true`): emits the variable's address (for `Variable` expressions)
///   or the address of a temporary (for non-variable expressions), then pushes `PhpType::Int`.
/// - **Regular parameters**: delegates to `push_expr_arg` which evaluates, materializes, and returns
///   the runtime type for each argument.
///
/// By-ref emission uses `emit_ref_arg_variable_address` for simple variable references, falling back
/// to `push_non_variable_ref_arg_address` for expressions that require a temporary address.
///
/// # Parameters
/// - `all_args`: Prepared argument expressions to emit.
/// - `sig`: Function signature providing `ref_params` and target-type information.
/// - `ref_arg_context_label`: Diagnostic label passed through to ref-arg emitters.
/// - `_retain_non_variable_ref_args`: Currently unused; retained for API compatibility.
/// - `coerce_inferred_params`: Passed to `call_target_ty` to control type coercion behavior.
/// - `emitter`/`ctx`/`data`: Codegen state passed through to sub-emitters.
///
/// # Returns
/// A `Vec<PhpType>` listing the runtime type of each emitted argument, in argument order.
pub(crate) fn emit_pushed_non_variadic_args(
    all_args: &[Expr],
    sig: Option<&FunctionSig>,
    ref_arg_context_label: &str,
    _retain_non_variable_ref_args: bool,
    coerce_inferred_params: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<PhpType> {
    let mut arg_types = Vec::new();

    for (idx, arg) in all_args.iter().enumerate() {
        let is_ref = sig
            .and_then(|sig| sig.ref_params.get(idx))
            .copied()
            .unwrap_or(false);
        let target_ty = call_target_ty(sig, idx, coerce_inferred_params);

        if is_ref {
            if let ExprKind::Variable(var_name) = &arg.kind {
                if !emit_ref_arg_variable_address(var_name, ref_arg_context_label, emitter, ctx) {
                    continue;
                }
                push_arg_value(emitter, &PhpType::Int);
            } else {
                push_non_variable_ref_arg_address(arg, target_ty, emitter, ctx, data);
            }
            arg_types.push(PhpType::Int);
        } else {
            let pushed_ty = push_expr_arg(arg, target_ty, emitter, ctx, data);
            arg_types.push(pushed_ty);
        }
    }

    arg_types
}
