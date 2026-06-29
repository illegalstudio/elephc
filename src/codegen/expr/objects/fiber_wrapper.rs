//! Purpose:
//! Lowers deferred wrapper registration for object methods used as fiber callables.
//! Produces object-related expression results while respecting runtime metadata and ownership rules.
//!
//! Called from:
//! - `crate::codegen::expr::objects`
//!
//! Key details:
//! - Object handles, property storage, and class ids must stay consistent with emitted class tables.

use crate::codegen::context::{Context, DeferredFiberWrapper};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{fibers, FunctionSig, PhpType};

/// Registers a fiber wrapper entry point for a callable used as a Fiber start routine.
///
/// Dispatches on `callable_expr` to extract the signature and capture metadata:
/// - `ExprKind::Closure`: uses deferred closure signature, hidden params, and return analysis
/// - `ExprKind::FirstClassCallable`: uses deferred closure signature directly
/// - `ExprKind::Variable`: looks up closure captures and signatures, searches deferred closures for matching params/captures
///
/// On success, pushes a `DeferredFiberWrapper` to `ctx.deferred_fiber_wrappers` and returns
/// the unique label for the wrapper entry point. Returns `None` if validation fails or
/// the callable kind is unsupported.
///
/// # Panics
/// Panics if `ctx.deferred_closures` is empty when processing a closure or first-class callable.
pub(super) fn prepare_fiber_wrapper(callable_expr: &Expr, ctx: &mut Context) -> Option<String> {
    let (mut sig, visible_param_count, hidden_arg_types) = match &callable_expr.kind {
        ExprKind::Closure {
            params,
            variadic,
            body,
            ..
        } => {
            let visible_param_count = fibers::visible_param_count(params.len(), variadic.is_some());
            let no_terminal_return = !fibers::closure_body_has_return(body);
            let deferred = ctx.deferred_closures.last_mut()?;
            fibers::adapt_entry_sig(&mut deferred.sig, visible_param_count, no_terminal_return);
            fibers::validate_callback_signature(&deferred.sig, visible_param_count, callable_expr.span)
                .ok()?;
            (
                deferred.sig.clone(),
                visible_param_count,
                deferred
                    .hidden_params
                    .iter()
                    .map(hidden_capture_arg_type)
                    .collect(),
            )
        }
        ExprKind::FirstClassCallable(_) => {
            let deferred = ctx.deferred_closures.last_mut()?;
            let visible_param_count = deferred.sig.params.len();
            fibers::adapt_entry_sig(&mut deferred.sig, visible_param_count, false);
            fibers::validate_callback_signature(&deferred.sig, visible_param_count, callable_expr.span)
                .ok()?;
            (
                deferred.sig.clone(),
                visible_param_count,
                deferred
                    .hidden_params
                    .iter()
                    .map(hidden_capture_arg_type)
                    .collect(),
            )
        }
        ExprKind::Variable(name) if variable_needs_descriptor_invoker(name, callable_expr, ctx) => {
            return Some(prepare_descriptor_invoker_wrapper(ctx));
        }
        ExprKind::Variable(name) => {
            ctx.mark_fcc_used(name);
            let captures = ctx.closure_captures.get(name).cloned().unwrap_or_default();
            let mut sig = ctx.closure_sigs.get(name).cloned()?;
            let visible_param_count = sig.params.len();
            let mut hidden_arg_types = captures
                .iter()
                .map(hidden_capture_arg_type)
                .collect::<Vec<_>>();
            if let Some(deferred) = ctx.deferred_closures.iter_mut().rev().find(|deferred| {
                deferred.sig.params == sig.params && deferred.captures == captures
            }) {
                let no_terminal_return = !fibers::closure_body_has_return(&deferred.body);
                fibers::adapt_entry_sig(
                    &mut deferred.sig,
                    visible_param_count,
                    no_terminal_return,
                );
                fibers::validate_callback_signature(&deferred.sig, visible_param_count, callable_expr.span)
                    .ok()?;
                hidden_arg_types = deferred
                    .hidden_params
                    .iter()
                    .map(hidden_capture_arg_type)
                    .collect();
                sig = deferred.sig.clone();
            } else {
                fibers::adapt_entry_sig(&mut sig, visible_param_count, false);
                fibers::validate_callback_signature(&sig, visible_param_count, callable_expr.span)
                    .ok()?;
            }
            ctx.closure_sigs.insert(name.clone(), sig.clone());
            (sig, visible_param_count, hidden_arg_types)
        }
        _ if expr_is_descriptor_backed_callable(callable_expr, ctx) => {
            return Some(prepare_descriptor_invoker_wrapper(ctx));
        }
        _ => return None,
    };

    fibers::adapt_entry_sig(&mut sig, visible_param_count, false);
    let label = ctx.next_label("fiber_entry_wrapper");
    ctx.deferred_fiber_wrappers.push(DeferredFiberWrapper {
        label: label.clone(),
        sig,
        visible_param_count,
        hidden_arg_types,
        retain_hidden_args_for_closure_call: true,
        use_descriptor_invoker: false,
    });
    Some(label)
}

/// Returns true when a variable's callable value must be invoked through its runtime descriptor.
fn variable_needs_descriptor_invoker(name: &str, callable_expr: &Expr, ctx: &Context) -> bool {
    ctx.runtime_callable_vars.contains(name)
        || ctx.callable_param_names.contains(name)
        || (!ctx.closure_sigs.contains_key(name) && expr_is_descriptor_backed_callable(callable_expr, ctx))
}

/// Returns true when `expr` is already represented by a runtime callable descriptor.
fn expr_is_descriptor_backed_callable(expr: &Expr, ctx: &Context) -> bool {
    matches!(
        crate::codegen::functions::infer_contextual_type(expr, ctx).codegen_repr(),
        PhpType::Callable
    )
}

/// Registers or reuses the generic Fiber wrapper that calls a descriptor invoker.
pub(super) fn prepare_descriptor_invoker_wrapper(ctx: &mut Context) -> String {
    if let Some(existing) = ctx
        .deferred_fiber_wrappers
        .iter()
        .find(|wrapper| wrapper.use_descriptor_invoker)
    {
        return existing.label.clone();
    }

    let label = ctx.next_label("fiber_descriptor_invoker");
    ctx.deferred_fiber_wrappers.push(DeferredFiberWrapper {
        label: label.clone(),
        sig: descriptor_invoker_placeholder_sig(),
        visible_param_count: 0,
        hidden_arg_types: Vec::new(),
        retain_hidden_args_for_closure_call: true,
        use_descriptor_invoker: true,
    });
    label
}

/// Builds a placeholder signature for a descriptor-backed Fiber wrapper.
fn descriptor_invoker_placeholder_sig() -> FunctionSig {
    FunctionSig {
        params: Vec::new(),
        defaults: Vec::new(),
        return_type: PhpType::Mixed,
        declared_return: false,
        by_ref_return: false,
        ref_params: Vec::new(),
        declared_params: Vec::new(),
        variadic: None,
        deprecation: None,
    }
}

/// Maps a closure capture tuple to the `PhpType` used for the hidden argument passing slot.
///
/// Reference captures are encoded as `PhpType::Int` in the hidden arg type vector
/// because they are passed as integer pointers in the ABI.
fn hidden_capture_arg_type((_, ty, by_ref): &(String, PhpType, bool)) -> PhpType {
    if *by_ref {
        PhpType::Int
    } else {
        ty.clone()
    }
}
