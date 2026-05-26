//! Purpose:
//! Resolves include effects and nested declarations inside expression AST nodes.
//! Recurses through expression children, closures, properties, method bodies, and callable targets.
//!
//! Called from:
//! - `crate::resolver::stmt_exprs` and resolver engine helpers.
//!
//! Key details:
//! - Expression traversal may invoke isolated resolution for nested bodies while preserving include state.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use crate::errors::CompileError;
use crate::parser::ast::{CallableTarget, ClassMethod, Expr, ExprKind, InstanceOfTarget};
use super::discovery::FunctionVariantRegistry;
use super::engine::resolve_isolated;
use super::state::ResolveState;
/// Recursively resolves include effects and nested declarations in an expression AST node.
///
/// Walks the expression tree, dispatching on each `ExprKind` variant. For expression
/// variants that contain child expressions (e.g. `BinaryOp`, `Ternary`, `Assignment`),
/// recursively calls `resolve_expr` on each child. For variants that contain isolated
/// nested bodies (e.g. `Closure`, `Assignment` with a `prelude`), invokes
/// `resolve_isolated`. For `InstanceOf` targets and callable targets, delegates to
/// the focused helpers `resolve_instanceof_target` and `resolve_callable_target`.
///
/// # Args
/// - `expr`: The expression to resolve.
/// - `base_dir`: Base directory for resolving relative include paths.
/// - `declared_once`: Tracks files that have been processed exactly once via include/require.
/// - `include_chain`: Current chain of include paths for cycle detection.
/// - `state`: Shared resolver state (imports, constant definitions, etc.).
/// - `function_variants`: Registry tracking declared function variants across all files.
///
/// # Returns
/// The resolved expression with all nested includes and declarations resolved.
pub(super) fn resolve_expr(
    expr: Expr,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    function_variants: &FunctionVariantRegistry,
) -> Result<Expr, CompileError> {
    let span = expr.span;
    let kind = match expr.kind {
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(resolve_expr(*left, base_dir, declared_once, include_chain, state, function_variants)?),
            op,
            right: Box::new(resolve_expr(*right, base_dir, declared_once, include_chain, state, function_variants)?),
        },
        ExprKind::InstanceOf { value, target } => ExprKind::InstanceOf {
            value: Box::new(resolve_expr(*value, base_dir, declared_once, include_chain, state, function_variants)?),
            target: resolve_instanceof_target(target, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        ExprKind::Negate(inner) => ExprKind::Negate(Box::new(resolve_expr(
            *inner,
            base_dir,
            declared_once,
            include_chain,
            state,
            function_variants,
        )?)),
        ExprKind::Not(inner) => ExprKind::Not(Box::new(resolve_expr(
            *inner,
            base_dir,
            declared_once,
            include_chain,
            state,
            function_variants,
        )?)),
        ExprKind::BitNot(inner) => ExprKind::BitNot(Box::new(resolve_expr(
            *inner,
            base_dir,
            declared_once,
            include_chain,
            state,
            function_variants,
        )?)),
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(resolve_expr(
            *inner,
            base_dir,
            declared_once,
            include_chain,
            state,
            function_variants,
        )?)),
        ExprKind::ErrorSuppress(inner) => ExprKind::ErrorSuppress(Box::new(resolve_expr(
            *inner,
            base_dir,
            declared_once,
            include_chain,
            state,
            function_variants,
        )?)),
        ExprKind::Print(inner) => ExprKind::Print(Box::new(resolve_expr(
            *inner,
            base_dir,
            declared_once,
            include_chain,
            state,
            function_variants,
        )?)),
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(resolve_expr(*value, base_dir, declared_once, include_chain, state, function_variants)?),
            default: Box::new(resolve_expr(*default, base_dir, declared_once, include_chain, state, function_variants)?),
        },
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => ExprKind::Assignment {
            target: Box::new(resolve_expr(*target, base_dir, declared_once, include_chain, state, function_variants)?),
            value: Box::new(resolve_expr(*value, base_dir, declared_once, include_chain, state, function_variants)?),
            result_target: result_target
                .map(|target| resolve_expr(*target, base_dir, declared_once, include_chain, state, function_variants))
                .transpose()?
                .map(Box::new),
            prelude: resolve_isolated(
                prelude,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?,
            conditional_value_temp,
        },
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name,
            args: resolve_exprs(args, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        ExprKind::ArrayLiteral(items) => ExprKind::ArrayLiteral(resolve_exprs(
            items,
            base_dir,
            declared_once,
            include_chain,
            state,
            function_variants,
        )?),
        ExprKind::ArrayLiteralAssoc(entries) => ExprKind::ArrayLiteralAssoc(
            entries
                .into_iter()
                .map(|(key, value)| {
                    Ok((
                        resolve_expr(key, base_dir, declared_once, include_chain, state, function_variants)?,
                        resolve_expr(value, base_dir, declared_once, include_chain, state, function_variants)?,
                    ))
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => ExprKind::Match {
            subject: Box::new(resolve_expr(
                *subject,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?),
            arms: arms
                .into_iter()
                .map(|(patterns, value)| {
                    Ok((
                        resolve_exprs(patterns, base_dir, declared_once, include_chain, state, function_variants)?,
                        resolve_expr(value, base_dir, declared_once, include_chain, state, function_variants)?,
                    ))
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
            default: default
                .map(|expr| resolve_expr(*expr, base_dir, declared_once, include_chain, state, function_variants))
                .transpose()?
                .map(Box::new),
        },
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(resolve_expr(*array, base_dir, declared_once, include_chain, state, function_variants)?),
            index: Box::new(resolve_expr(*index, base_dir, declared_once, include_chain, state, function_variants)?),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(resolve_expr(
                *condition,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?),
            then_expr: Box::new(resolve_expr(
                *then_expr,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?),
            else_expr: Box::new(resolve_expr(
                *else_expr,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary {
            value: Box::new(resolve_expr(*value, base_dir, declared_once, include_chain, state, function_variants)?),
            default: Box::new(resolve_expr(*default, base_dir, declared_once, include_chain, state, function_variants)?),
        },
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target,
            expr: Box::new(resolve_expr(*expr, base_dir, declared_once, include_chain, state, function_variants)?),
        },
        ExprKind::Closure {
            params,
            variadic,
            return_type,
            body,
            is_arrow,
            is_static,
            captures,
            capture_refs,
        } => ExprKind::Closure {
            params: resolve_params(params, base_dir, declared_once, include_chain, state, function_variants)?,
            variadic,
            return_type,
            body: resolve_isolated(
                body,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?,
            is_arrow,
            is_static,
            captures,
            capture_refs,
        },
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name,
            value: Box::new(resolve_expr(*value, base_dir, declared_once, include_chain, state, function_variants)?),
        },
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(resolve_expr(
            *inner,
            base_dir,
            declared_once,
            include_chain,
            state,
            function_variants,
        )?)),
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var,
            args: resolve_exprs(args, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(resolve_expr(
                *callee,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?),
            args: resolve_exprs(args, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name,
            args: resolve_exprs(args, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(resolve_expr(
                *object,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?),
            property,
        },
        ExprKind::DynamicPropertyAccess { object, property } => {
            ExprKind::DynamicPropertyAccess {
                object: Box::new(resolve_expr(
                    *object,
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?),
                property: Box::new(resolve_expr(
                    *property,
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?),
            }
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            ExprKind::NullsafePropertyAccess {
                object: Box::new(resolve_expr(
                    *object,
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?),
                property,
            }
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            ExprKind::NullsafeDynamicPropertyAccess {
                object: Box::new(resolve_expr(
                    *object,
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?),
                property: Box::new(resolve_expr(
                    *property,
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                    function_variants,
                )?),
            }
        }
        ExprKind::MethodCall { object, method, args } => ExprKind::MethodCall {
            object: Box::new(resolve_expr(
                *object,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?),
            method,
            args: resolve_exprs(args, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => ExprKind::NullsafeMethodCall {
            object: Box::new(resolve_expr(
                *object,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?),
            method,
            args: resolve_exprs(args, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver,
            method,
            args: resolve_exprs(args, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        ExprKind::FirstClassCallable(target) => {
            ExprKind::FirstClassCallable(resolve_callable_target(
                target,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?)
        }
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(resolve_expr(*expr, base_dir, declared_once, include_chain, state, function_variants)?),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type,
            len: Box::new(resolve_expr(*len, base_dir, declared_once, include_chain, state, function_variants)?),
        },
        ExprKind::NewScopedObject { receiver, args } => ExprKind::NewScopedObject {
            receiver,
            args: resolve_exprs(args, base_dir, declared_once, include_chain, state, function_variants)?,
        },
        other => other,
    };
    Ok(Expr::new(kind, span))
}

/// Maps `resolve_expr` over a vector of expressions, returning a vector of resolved expressions.
///
/// # Args
/// Same as `resolve_expr`.
///
/// # Returns
/// A vector of resolved expressions in the same order as the input.
fn resolve_exprs(
    exprs: Vec<Expr>,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    function_variants: &FunctionVariantRegistry,
) -> Result<Vec<Expr>, CompileError> {
    exprs
        .into_iter()
        .map(|expr| {
            resolve_expr(
                expr,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )
        })
        .collect()
}

/// Resolves expressions inside a list of function/method parameters.
///
/// Each parameter may have a default value expression; this function resolves those
/// default value expressions recursively. Parameter names and types are passed through
/// unchanged.
///
/// # Args
/// Same as `resolve_expr`.
///
/// # Returns
/// A vector of resolved parameters with the same names, types, and reference flags.
pub(super) fn resolve_params(
    params: Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)>,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    function_variants: &FunctionVariantRegistry,
) -> Result<Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)>, CompileError> {
    params
        .into_iter()
        .map(|(name, type_expr, default, is_ref)| {
            Ok((
                name,
                type_expr,
                default
                    .map(|expr| {
                        resolve_expr(
                            expr,
                            base_dir,
                            declared_once,
                            include_chain,
                            state,
                            function_variants,
                        )
                    })
                    .transpose()?,
                is_ref,
            ))
        })
        .collect()
}

/// Resolves default value expressions in a list of class properties.
///
/// # Args
/// Same as `resolve_expr`.
///
/// # Returns
/// A vector of resolved properties with default values processed.
pub(super) fn resolve_properties(
    properties: Vec<crate::parser::ast::ClassProperty>,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    function_variants: &FunctionVariantRegistry,
) -> Result<Vec<crate::parser::ast::ClassProperty>, CompileError> {
    properties
        .into_iter()
        .map(|mut property| {
            property.default = property
                .default
                .map(|expr| {
                    resolve_expr(
                        expr,
                        base_dir,
                        declared_once,
                        include_chain,
                        state,
                        function_variants,
                    )
                })
                .transpose()?;
            Ok(property)
        })
        .collect()
}

/// Resolves expressions inside a list of class methods.
///
/// Currently only resolves default value expressions in method parameters by delegating
/// to `resolve_params`. The method body itself is handled separately during statement
/// resolution.
///
/// # Args
/// Same as `resolve_expr`.
///
/// # Returns
/// A vector of resolved methods with parameter defaults processed.
pub(super) fn resolve_method_exprs(
    methods: Vec<ClassMethod>,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    function_variants: &FunctionVariantRegistry,
) -> Result<Vec<ClassMethod>, CompileError> {
    methods
        .into_iter()
        .map(|mut method| {
            method.params = resolve_params(
                method.params,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?;
            Ok(method)
        })
        .collect()
}

/// Resolves the object expression inside a callable `InstanceOf` or method-call target.
///
/// For `CallableTarget::Method`, resolves the object expression; for `Function` and
/// `StaticMethod` variants, passes through unchanged since they contain no expressions.
///
/// # Args
/// Same as `resolve_expr`.
///
/// # Returns
/// The callable target with any nested expressions resolved.
fn resolve_callable_target(
    target: CallableTarget,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    function_variants: &FunctionVariantRegistry,
) -> Result<CallableTarget, CompileError> {
    Ok(match target {
        CallableTarget::Function(name) => CallableTarget::Function(name),
        CallableTarget::StaticMethod { receiver, method } => {
            CallableTarget::StaticMethod { receiver, method }
        }
        CallableTarget::Method { object, method } => CallableTarget::Method {
            object: Box::new(resolve_expr(
                *object,
                base_dir,
                declared_once,
                include_chain,
                state,
                function_variants,
            )?),
            method,
        },
    })
}

/// Resolves the target expression inside an `InstanceOf` expression.
///
/// For `InstanceOfTarget::Expr`, resolves the contained expression; for `InstanceOfTarget::Name`,
/// passes through unchanged since it contains only a bare name.
///
/// # Args
/// Same as `resolve_expr`.
///
/// # Returns
/// The instanceof target with any nested expressions resolved.
fn resolve_instanceof_target(
    target: InstanceOfTarget,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
    function_variants: &FunctionVariantRegistry,
) -> Result<InstanceOfTarget, CompileError> {
    match target {
        InstanceOfTarget::Name(name) => Ok(InstanceOfTarget::Name(name)),
        InstanceOfTarget::Expr(expr) => Ok(InstanceOfTarget::Expr(Box::new(resolve_expr(
            *expr,
            base_dir,
            declared_once,
            include_chain,
            state,
            function_variants,
        )?))),
    }
}
