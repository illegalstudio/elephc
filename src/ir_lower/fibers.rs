//! Purpose:
//! Tracks statically known Fiber callback signatures during AST-to-EIR lowering.
//! Supplies callback-shaped `Fiber::start()` signatures for named and spread
//! argument normalization.
//!
//! Called from:
//! - `crate::ir_lower::program`, `crate::ir_lower::stmt`, and
//!   `crate::ir_lower::expr`.
//!
//! Key details:
//! - Fixed callback parameter names are reused only for start-argument
//!   planning; all start values remain boxed as `Mixed`.
//! - Variadic callbacks deliberately return no start-call signature so the
//!   raw start arguments reach the Fiber wrapper and can be packed there.

use std::collections::HashMap;

use crate::ir_lower::context::{LoweringContext, StaticCallableBinding};
use crate::names::php_symbol_key;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind};
use crate::types::{FunctionSig, PhpType};

/// Collects functions whose body directly returns `new Fiber(<known callback>)`.
pub(crate) fn collect_fiber_return_sigs(program: &Program) -> HashMap<String, FunctionSig> {
    let mut sigs = HashMap::new();
    collect_fiber_return_sigs_from_stmts(program, &mut sigs);
    sigs
}

/// Returns the known Fiber start signature associated with a Fiber expression.
pub(crate) fn start_sig_for_expr(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<FunctionSig> {
    match &expr.kind {
        ExprKind::Variable(name) => ctx.fiber_start_sig_for_local(name),
        ExprKind::FunctionCall { name, .. } => ctx.fiber_return_sig(name),
        ExprKind::NewObject { .. } => start_sig_from_new_object(ctx, expr),
        _ => None,
    }
}

/// Returns the Fiber callback start signature for a `new Fiber(...)` expression.
fn start_sig_from_new_object(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<FunctionSig> {
    let ExprKind::NewObject { class_name, args } = &expr.kind else {
        return None;
    };
    if php_symbol_key(class_name.as_str()) != php_symbol_key("Fiber") {
        return None;
    }
    let callback = args.first()?;
    callback_sig_for_expr(ctx, callback).and_then(|sig| start_sig_from_callback_sig(&sig))
}

/// Recursively scans statements for function declarations with direct Fiber returns.
fn collect_fiber_return_sigs_from_stmts(
    stmts: &[Stmt],
    sigs: &mut HashMap<String, FunctionSig>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl { name, body, .. } => {
                if let Some(sig) = fiber_return_sig_from_body(body) {
                    sigs.insert(name.clone(), sig);
                }
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                collect_fiber_return_sigs_from_stmts(body, sigs);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_fiber_return_sigs_from_stmts(then_body, sigs);
                for (_, body) in elseif_clauses {
                    collect_fiber_return_sigs_from_stmts(body, sigs);
                }
                if let Some(body) = else_body {
                    collect_fiber_return_sigs_from_stmts(body, sigs);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                collect_fiber_return_sigs_from_stmts(then_body, sigs);
                if let Some(body) = else_body {
                    collect_fiber_return_sigs_from_stmts(body, sigs);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                collect_fiber_return_sigs_from_stmts(body, sigs);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_fiber_return_sigs_from_stmts(body, sigs);
                }
                if let Some(body) = default {
                    collect_fiber_return_sigs_from_stmts(body, sigs);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_fiber_return_sigs_from_stmts(try_body, sigs);
                for catch in catches {
                    collect_fiber_return_sigs_from_stmts(&catch.body, sigs);
                }
                if let Some(body) = finally_body {
                    collect_fiber_return_sigs_from_stmts(body, sigs);
                }
            }
            _ => {}
        }
    }
}

/// Finds a direct `return new Fiber(<known callback>)` in a function body.
fn fiber_return_sig_from_body(body: &[Stmt]) -> Option<FunctionSig> {
    for stmt in body {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                if let Some(sig) = direct_new_fiber_callback_sig(expr) {
                    return start_sig_from_callback_sig(&sig);
                }
            }
            StmtKind::Synthetic(body) => {
                if let Some(sig) = fiber_return_sig_from_body(body) {
                    return Some(sig);
                }
            }
            _ => {}
        }
    }
    None
}

/// Returns the callback signature from a direct `new Fiber(<closure>)` return.
fn direct_new_fiber_callback_sig(expr: &Expr) -> Option<FunctionSig> {
    let ExprKind::NewObject { class_name, args } = &expr.kind else {
        return None;
    };
    if php_symbol_key(class_name.as_str()) != php_symbol_key("Fiber") {
        return None;
    }
    let callback = args.first()?;
    match &callback.kind {
        ExprKind::Closure {
            params, variadic, ..
        } => Some(callback_sig_from_closure_params(params, variadic.as_deref())),
        _ => None,
    }
}

/// Returns the callback signature for the callable passed to `new Fiber(...)`.
fn callback_sig_for_expr(
    ctx: &LoweringContext<'_, '_>,
    callback: &Expr,
) -> Option<FunctionSig> {
    match &callback.kind {
        ExprKind::Closure {
            params, variadic, ..
        } => Some(callback_sig_from_closure_params(params, variadic.as_deref())),
        ExprKind::Variable(name) => ctx
            .callable_param_signature(name)
            .cloned()
            .or_else(|| ctx.static_callable_local(name).and_then(|target| callback_sig_for_binding(ctx, target))),
        ExprKind::StringLiteral(name) => ctx.functions.get(name.as_str()).cloned(),
        ExprKind::FirstClassCallable(target) => callback_sig_for_first_class_callable(ctx, target),
        _ => None,
    }
}

/// Returns a callback signature from a tracked static callable binding.
fn callback_sig_for_binding(
    ctx: &LoweringContext<'_, '_>,
    target: StaticCallableBinding,
) -> Option<FunctionSig> {
    match target {
        StaticCallableBinding::UserFunction(name) => ctx.functions.get(name.as_str()).cloned(),
        StaticCallableBinding::ExternFunction(name) => {
            ctx.extern_functions.get(name.as_str()).map(|sig| FunctionSig {
                params: sig.params.clone(),
                defaults: vec![None; sig.params.len()],
                return_type: sig.return_type.clone(),
                declared_return: true,
                by_ref_return: false,
                ref_params: vec![false; sig.params.len()],
                declared_params: vec![true; sig.params.len()],
                variadic: None,
                deprecation: None,
            })
        }
        StaticCallableBinding::Builtin(_) => None,
        StaticCallableBinding::Closure { signature, .. } => Some(signature),
        StaticCallableBinding::StaticMethod { receiver, method }
        | StaticCallableBinding::StaticMethodDescriptor { receiver, method } => {
            static_method_sig(ctx, &receiver, &method)
        }
        StaticCallableBinding::InstanceMethod { signature, .. } => Some(signature),
    }
}

/// Resolves a first-class callable target to its callback signature when static.
fn callback_sig_for_first_class_callable(
    ctx: &LoweringContext<'_, '_>,
    target: &CallableTarget,
) -> Option<FunctionSig> {
    match target {
        CallableTarget::Function(name) => ctx.functions.get(name.as_str()).cloned(),
        CallableTarget::StaticMethod { receiver, method } => static_method_sig(ctx, receiver, method),
        CallableTarget::Method { object, method } => {
            let ExprKind::Variable(name) = &object.kind else {
                return None;
            };
            let object_ty = ctx.local_type(name).codegen_repr();
            let PhpType::Object(class_name) = object_ty else {
                return None;
            };
            class_method_sig(ctx, &class_name, method)
        }
    }
}

/// Returns a static method signature for a first-class callable or binding.
fn static_method_sig(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
) -> Option<FunctionSig> {
    let class_name = match receiver {
        StaticReceiver::Named(name) => name.as_str().trim_start_matches('\\').to_string(),
        StaticReceiver::Self_ | StaticReceiver::Static => ctx.current_class.clone()?,
        StaticReceiver::Parent => {
            let current = ctx.current_class.as_ref()?;
            ctx.classes.get(current.as_str()).and_then(|class| class.parent.clone())?
        }
    };
    let method_key = php_symbol_key(method);
    ctx.classes
        .get(class_name.as_str())
        .and_then(|class_info| {
            let impl_class = class_info
                .static_method_impl_classes
                .get(&method_key)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            ctx.classes
                .get(impl_class)
                .and_then(|impl_info| impl_info.static_methods.get(&method_key))
                .or_else(|| class_info.static_methods.get(&method_key))
        })
        .cloned()
}

/// Returns an instance method signature for a statically known object class.
fn class_method_sig(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method: &str,
) -> Option<FunctionSig> {
    let normalized = class_name.trim_start_matches('\\');
    let method_key = php_symbol_key(method);
    ctx.classes
        .get(normalized)
        .and_then(|class_info| {
            let impl_class = class_info
                .method_impl_classes
                .get(&method_key)
                .map(String::as_str)
                .unwrap_or(normalized);
            ctx.classes
                .get(impl_class)
                .and_then(|impl_info| impl_info.methods.get(&method_key))
                .or_else(|| class_info.methods.get(&method_key))
        })
        .cloned()
}

/// Builds a callback signature from closure parameter syntax.
fn callback_sig_from_closure_params(
    params: &[(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&str>,
) -> FunctionSig {
    let mut sig = FunctionSig {
        params: params
            .iter()
            .map(|(name, ty, _, _)| {
                (
                    name.clone(),
                    ty.as_ref()
                        .map(crate::ir_lower::context::type_expr_to_php_type)
                        .unwrap_or(PhpType::Mixed),
                )
            })
            .collect(),
        defaults: params
            .iter()
            .map(|(_, _, default, _)| default.clone())
            .collect(),
        return_type: PhpType::Mixed,
        declared_return: false,
        by_ref_return: false,
        ref_params: params.iter().map(|(_, _, _, is_ref)| *is_ref).collect(),
        declared_params: params.iter().map(|(_, ty, _, _)| ty.is_some()).collect(),
        variadic: variadic.map(str::to_string),
        deprecation: None,
    };
    if let Some(variadic_name) = variadic {
        if !sig.params.iter().any(|(name, _)| name == variadic_name) {
            sig.params
                .push((variadic_name.to_string(), PhpType::Array(Box::new(PhpType::Mixed))));
            sig.defaults.push(None);
            sig.ref_params.push(false);
            sig.declared_params.push(false);
        }
    }
    sig
}

/// Converts a known callback signature into the synthetic fixed `Fiber::start()` view.
fn start_sig_from_callback_sig(sig: &FunctionSig) -> Option<FunctionSig> {
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
