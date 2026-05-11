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
use crate::types::fibers;

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
                    .map(|(_, ty)| ty.clone())
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
                    .map(|(_, ty)| ty.clone())
                    .collect(),
            )
        }
        ExprKind::Variable(name) => {
            let captures = ctx.closure_captures.get(name).cloned().unwrap_or_default();
            let mut sig = ctx.closure_sigs.get(name).cloned()?;
            let visible_param_count = sig.params.len();
            let mut hidden_arg_types = captures
                .iter()
                .map(|(_, ty)| ty.clone())
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
                    .map(|(_, ty)| ty.clone())
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
        _ => return None,
    };

    fibers::adapt_entry_sig(&mut sig, visible_param_count, false);
    let label = ctx.next_label("fiber_entry_wrapper");
    ctx.deferred_fiber_wrappers.push(DeferredFiberWrapper {
        label: label.clone(),
        sig,
        visible_param_count,
        hidden_arg_types,
    });
    Some(label)
}
