//! Purpose:
//! Discovers include effects and declarations referenced inside expression trees.
//! Recurses through nested expression-owned bodies and parameters during include discovery.
//!
//! Called from:
//! - `crate::resolver::discovery::stmts` and branch discovery.
//!
//! Key details:
//! - Expression discovery must stay in lockstep with resolver expression walking to avoid hidden declarations.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, InstanceOfTarget};

use super::branches::discover_isolated;
use super::members::discover_params;
use super::output::DiscoveryOutput;
use super::super::state::ResolveState;

/// Recursively discovers include effects and declarations within an expression tree.
///
/// Visits every `ExprKind` variant, recursing into child expressions and callable arguments.
/// Accumulates loaded paths in `loaded_paths` to prevent duplicate includes and tracks
/// the include chain for cycle detection. Declaration effects are accumulated in `output`.
///
/// # Arguments
/// * `expr` - the expression node to walk
/// * `base_dir` - base directory for resolving relative include paths
/// * `loaded_paths` - mutable set of already-loaded paths; updated in-place
/// * `include_chain` - mutable Vec tracking the current include stack for cycle detection
/// * `state` - resolver state (conditions, function variants, etc.)
/// * `output` - mutable discovery output accumulating includes, declarations, and errors
pub(super) fn discover_expr(
    expr: &Expr,
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    match &expr.kind {
        ExprKind::BinaryOp { left, right, .. } => {
            discover_expr(left, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(right, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::InstanceOf { value, target } => {
            discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
            discover_instanceof_target(target, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Throw(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Spread(value)
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::BufferNew { len: value, .. } => {
            discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ArrayAccess { array: value, index: default }
        | ExprKind::ShortTernary { value, default } => {
            discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(default, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::Pipe { value, callable } => {
            discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(callable, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::Assignment { target, value, result_target, prelude, .. } => {
            discover_expr(target, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
            if let Some(result_target) = result_target {
                discover_expr(
                    result_target,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    output,
                )?;
            }
            discover_isolated(
                prelude,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            discover_exprs(args, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::NewDynamic { name_expr, args } => {
            discover_expr(name_expr, base_dir, loaded_paths, include_chain, state, output)?;
            discover_exprs(args, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            discover_expr(class_name, base_dir, loaded_paths, include_chain, state, output)?;
            discover_exprs(args, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::ArrayLiteral(items) => {
            discover_exprs(items, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::ArrayLiteralAssoc(entries) => {
            for (key, value) in entries {
                discover_expr(key, base_dir, loaded_paths, include_chain, state, output)?;
                discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
            }
        }
        ExprKind::Match { subject, arms, default } => {
            discover_expr(subject, base_dir, loaded_paths, include_chain, state, output)?;
            for (patterns, value) in arms {
                discover_exprs(
                    patterns,
                    base_dir,
                    loaded_paths,
                    include_chain,
                    state,
                    output,
                )?;
                discover_expr(value, base_dir, loaded_paths, include_chain, state, output)?;
            }
            if let Some(default) = default {
                discover_expr(default, base_dir, loaded_paths, include_chain, state, output)?;
            }
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            discover_expr(condition, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(then_expr, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(else_expr, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::Cast { expr, .. } | ExprKind::NamedArg { value: expr, .. } => {
            discover_expr(expr, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::Closure { params, body, .. } => {
            discover_params(params, base_dir, loaded_paths, include_chain, state, output)?;
            discover_isolated(
                body,
                base_dir,
                loaded_paths,
                include_chain,
                state,
                output,
            )?;
        }
        ExprKind::ExprCall { callee, args } => {
            discover_expr(callee, base_dir, loaded_paths, include_chain, state, output)?;
            discover_exprs(args, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            discover_expr(object, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            discover_expr(object, base_dir, loaded_paths, include_chain, state, output)?;
            discover_expr(property, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            discover_expr(object, base_dir, loaded_paths, include_chain, state, output)?;
            discover_exprs(args, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::FirstClassCallable(crate::parser::ast::CallableTarget::Method { object, .. }) => {
            discover_expr(object, base_dir, loaded_paths, include_chain, state, output)?;
        }
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => {}
        ExprKind::Yield { key, value } => {
            if let Some(k) = key {
                discover_expr(k, base_dir, loaded_paths, include_chain, state, output)?;
            }
            if let Some(v) = value {
                discover_expr(v, base_dir, loaded_paths, include_chain, state, output)?;
            }
        }
        ExprKind::YieldFrom(inner) => {
            discover_expr(inner, base_dir, loaded_paths, include_chain, state, output)?;
        }
    }
    Ok(())
}

/// Iterates over a slice of expressions, calling `discover_expr` on each.
///
/// Consumes the slice in order, accumulating include effects and declarations from
/// each expression into the shared `output`. Short-circuits on the first error.
pub(super) fn discover_exprs(
    exprs: &[Expr],
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    for expr in exprs {
        discover_expr(expr, base_dir, loaded_paths, include_chain, state, output)?;
    }
    Ok(())
}

/// Visits the target of an `InstanceOf` expression.
///
/// `InstanceOfTarget::Name` carries no expression to recurse — nothing to discover.
/// `InstanceOfTarget::Expr` wraps an expression that must be walked for includes and declarations.
fn discover_instanceof_target(
    target: &InstanceOfTarget,
    base_dir: &Path,
    loaded_paths: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    output: &mut DiscoveryOutput,
) -> Result<(), CompileError> {
    match target {
        InstanceOfTarget::Name(_) => Ok(()),
        InstanceOfTarget::Expr(expr) => {
            discover_expr(expr, base_dir, loaded_paths, include_chain, state, output)
        }
    }
}
