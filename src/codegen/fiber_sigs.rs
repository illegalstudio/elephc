//! Purpose:
//! Tracks statically known Fiber callback parameter names for `Fiber::start()`.
//! Supplies start-call signatures so associative spreads can be reordered before
//! arguments are stored in the Fiber runtime object.
//!
//! Called from:
//! - `crate::codegen::generate_user_asm()` and Fiber method-call lowering.
//!
//! Key details:
//! - Start arguments are always boxed as `Mixed`; only parameter names/defaults
//!   are borrowed from the callback signature for named/spread normalization.

use std::collections::HashMap;

use crate::codegen::context::Context;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind};
use crate::types::{FunctionSig, PhpType};

/// Collects functions whose body directly returns `new Fiber(<known callback>)`.
pub(crate) fn collect_fiber_return_sigs(program: &Program) -> HashMap<String, FunctionSig> {
    let mut sigs = HashMap::new();
    collect_fiber_return_sigs_from_stmts(program, &mut sigs);
    sigs
}

/// Returns the known Fiber callback start signature associated with an expression.
pub(crate) fn fiber_start_sig_for_expr(expr: &Expr, ctx: &Context) -> Option<FunctionSig> {
    match &expr.kind {
        ExprKind::Variable(name) => ctx.fiber_start_sigs.get(name).cloned(),
        ExprKind::FunctionCall { name, .. } => ctx.fiber_return_sigs.get(name.as_str()).cloned(),
        ExprKind::NewObject { .. } => fiber_start_sig_from_new_object(expr, ctx),
        _ => None,
    }
}

/// Returns the Fiber callback start signature for a `new Fiber(...)` expression.
pub(crate) fn fiber_start_sig_from_new_object(expr: &Expr, ctx: &Context) -> Option<FunctionSig> {
    let ExprKind::NewObject { class_name, args } = &expr.kind else {
        return None;
    };
    if php_symbol_key(class_name.as_str()) != php_symbol_key("Fiber") {
        return None;
    }
    let callback = args.first()?;
    fiber_start_sig_from_callable_expr(callback, ctx)
}

/// Recursively scans statements for function declarations with direct Fiber returns.
fn collect_fiber_return_sigs_from_stmts(
    stmts: &[Stmt],
    sigs: &mut HashMap<String, FunctionSig>,
) {
    let ctx = Context::new();
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl { name, body, .. } => {
                if let Some(sig) = fiber_return_sig_from_body(body, &ctx) {
                    sigs.insert(name.clone(), sig);
                }
            }
            StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
                collect_fiber_return_sigs_from_stmts(body, sigs);
            }
            _ => {}
        }
    }
}

/// Finds a direct `return new Fiber(<known callback>)` in a function body.
fn fiber_return_sig_from_body(body: &[Stmt], ctx: &Context) -> Option<FunctionSig> {
    for stmt in body {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                if let Some(sig) = fiber_start_sig_from_new_object(expr, ctx) {
                    return Some(sig);
                }
            }
            StmtKind::Synthetic(body) => {
                if let Some(sig) = fiber_return_sig_from_body(body, ctx) {
                    return Some(sig);
                }
            }
            _ => {}
        }
    }
    None
}

/// Builds a Fiber start-call signature from a supported callable expression.
fn fiber_start_sig_from_callable_expr(callback: &Expr, ctx: &Context) -> Option<FunctionSig> {
    match &callback.kind {
        ExprKind::Closure {
            params, variadic, ..
        } => fiber_start_sig_from_closure_params(params, variadic.as_ref()),
        ExprKind::Variable(name) => ctx
            .closure_sigs
            .get(name)
            .and_then(fiber_start_sig_from_callback_sig),
        ExprKind::FirstClassCallable(target) => {
            crate::codegen::expr::calls::first_class_callable_sig(target, ctx)
                .and_then(|sig| fiber_start_sig_from_callback_sig(&sig))
        }
        _ => crate::codegen::callables::callable_sig(callback, ctx)
            .and_then(|sig| fiber_start_sig_from_callback_sig(&sig)),
    }
}

/// Builds a start-call signature from closure parameter syntax.
fn fiber_start_sig_from_closure_params(
    params: &[(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&String>,
) -> Option<FunctionSig> {
    if variadic.is_some() {
        return None;
    }
    Some(FunctionSig {
        params: params
            .iter()
            .map(|(name, _, _, _)| (name.clone(), PhpType::Mixed))
            .collect(),
        defaults: params
            .iter()
            .map(|(_, _, default, _)| default.clone())
            .collect(),
        return_type: PhpType::Mixed,
        declared_return: false,
        by_ref_return: false,
        ref_params: params.iter().map(|(_, _, _, is_ref)| *is_ref).collect(),
        declared_params: vec![false; params.len()],
        variadic: None,
        deprecation: None,
    })
}

/// Converts a known callback signature into the synthetic `Fiber::start()` view.
fn fiber_start_sig_from_callback_sig(sig: &FunctionSig) -> Option<FunctionSig> {
    if sig.variadic.is_some() {
        return None;
    }
    Some(FunctionSig {
        params: sig
            .params
            .iter()
            .map(|(name, _)| (name.clone(), PhpType::Mixed))
            .collect(),
        defaults: sig.defaults.clone(),
        return_type: PhpType::Mixed,
        declared_return: false,
        by_ref_return: false,
        ref_params: sig.ref_params.clone(),
        declared_params: vec![false; sig.params.len()],
        variadic: None,
        deprecation: None,
    })
}
