//! Purpose:
//! Finds the `static` locals of one function body that are used BY REFERENCE, so lowering
//! binds them to a persistent reference cell instead of their plain static slot.
//!
//! Called from:
//! - `crate::ir_lower::function`, once per lowered body (`LoweringContext::reference_bound_statics`).
//!
//! Key details:
//! - php binds `static $x` by reference to storage that outlives the call. A plain static slot
//!   holds the value itself, so every by-reference use acted on a copy: `add($labels, $k)` into
//!   `array &$a`, `$r = &$labels`, and a closure's `use (&$labels)` (monolog's
//!   `BrowserConsoleHandler::handleCustomStyles()`, which the backend refused outright).
//! - Only a static some by-reference use names is rebound: the cell costs a boxed read on every
//!   access, which a static used by value never pays.
//! - The scan is conservative. A call whose callee is not a known user function or builtin (a
//!   method, a dynamic call) may take the argument by reference, so it counts; a nested closure's
//!   own variable of the same name counts too. Both only cost the cell, never correctness.

use std::collections::{HashMap, HashSet};

use crate::parser::ast::visit::{walk_nodes, Node};
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::types::FunctionSig;

/// The names of `body`'s `static` locals that some by-reference use names.
pub(crate) fn reference_bound_statics(
    body: &[Stmt],
    functions: &HashMap<String, FunctionSig>,
) -> HashSet<String> {
    let mut statics = HashSet::new();
    walk_nodes(body, &mut |node| {
        if let Node::Stmt(stmt) = node {
            if let StmtKind::StaticVar { name, .. } = &stmt.kind {
                statics.insert(name.clone());
            }
        }
    });
    if statics.is_empty() {
        return statics;
    }
    let mut used = HashSet::new();
    walk_nodes(body, &mut |node| match node {
        Node::Stmt(stmt) => match &stmt.kind {
            StmtKind::RefAssign { target, source } => {
                used.insert(target.clone());
                if let ExprKind::Variable(name) = &source.kind {
                    used.insert(name.clone());
                }
            }
            StmtKind::Foreach { value_var, value_by_ref: true, .. } => {
                used.insert(value_var.clone());
            }
            _ => {}
        },
        Node::Expr(expr) => match &expr.kind {
            ExprKind::Closure { capture_refs, .. } => used.extend(capture_refs.iter().cloned()),
            ExprKind::ArrayReference(inner) => {
                if let ExprKind::Variable(name) = &inner.kind {
                    used.insert(name.clone());
                }
            }
            ExprKind::FunctionCall { name, args } => {
                let callee = name.as_str().trim_start_matches('\\');
                mark_arguments(args, &mut used, |index| function_param_may_be_by_ref(callee, index, functions));
            }
            ExprKind::MethodCall { args, .. }
            | ExprKind::NullsafeMethodCall { args, .. }
            | ExprKind::NullsafeDynamicMethodCall { args, .. }
            | ExprKind::StaticMethodCall { args, .. }
            | ExprKind::ClosureCall { args, .. }
            | ExprKind::ExprCall { args, .. }
            | ExprKind::NewObject { args, .. }
            | ExprKind::NewGeneric { args, .. }
            | ExprKind::NewDynamic { args, .. }
            | ExprKind::NewScopedObject { args, .. } => mark_arguments(args, &mut used, |_| true),
            _ => {}
        },
    });
    statics.retain(|name| used.contains(name));
    statics
}

/// The names of `body`'s `static` locals the body also assigns as a whole (`$x = ...`).
///
/// Such a static can receive any value later, so an array initializer with raw elements is
/// stored with boxed ones (`lower_static_var`): its slot otherwise widened only at the later
/// store, after the raw-element initializer was bound (symfony/http-client's
/// `static $lastProgress = [0, 0]; ... $lastProgress = $progress ?: $lastProgress;`).
pub(crate) fn reassigned_statics(body: &[Stmt]) -> HashSet<String> {
    let mut statics = HashSet::new();
    let mut assigned = HashSet::new();
    walk_nodes(body, &mut |node| {
        if let Node::Stmt(stmt) = node {
            match &stmt.kind {
                StmtKind::StaticVar { name, .. } => {
                    statics.insert(name.clone());
                }
                StmtKind::Assign { name, .. } => {
                    assigned.insert(name.clone());
                }
                _ => {}
            }
        }
    });
    statics.retain(|name| assigned.contains(name));
    statics
}

/// Marks each plain-variable argument whose parameter may take it by reference.
fn mark_arguments(args: &[Expr], used: &mut HashSet<String>, by_ref: impl Fn(usize) -> bool) {
    for (index, arg) in args.iter().enumerate() {
        let value = match &arg.kind {
            ExprKind::NamedArg { value, .. } => value.as_ref(),
            _ => arg,
        };
        if let ExprKind::Variable(name) = &value.kind {
            // A named argument's position is not its parameter's: any by-ref parameter counts.
            let named = matches!(arg.kind, ExprKind::NamedArg { .. });
            if by_ref(index) || (named && (0..16).any(&by_ref)) {
                used.insert(name.clone());
            }
        }
    }
}

/// Whether parameter `index` of the function `callee` may be by reference: a known user function
/// or builtin answers from its signature (a by-reference variadic covers every later position);
/// an unknown callee may be either.
fn function_param_may_be_by_ref(
    callee: &str,
    index: usize,
    functions: &HashMap<String, FunctionSig>,
) -> bool {
    if let Some(sig) = functions.get(callee).or_else(|| {
        functions
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(callee))
            .map(|(_, sig)| sig)
    }) {
        return sig.ref_params.get(index).copied().unwrap_or_else(|| {
            sig.variadic.is_some() && sig.ref_params.last().copied().unwrap_or(false)
        });
    }
    let bare = callee.rsplit('\\').next().unwrap_or(callee);
    match elephc_builtin_contract::lookup(&bare.to_ascii_lowercase()) {
        Some(contract) => contract
            .params
            .get(index)
            .map(|param| param.by_ref)
            .unwrap_or_else(|| contract.variadic.is_some_and(|variadic| variadic.is_by_reference())),
        None => true,
    }
}
