//! Purpose:
//! Deep-hoists expression-position include/require (`IncludeValue`) that appear anywhere inside a
//! statement's current-scope expressions, expanding them before generic expression resolution runs.
//!
//! Called from:
//! - `crate::resolver::engine::resolve_stmts()`, after the direct-RHS fast path
//!   (`$x = require X;` / `return require X;`) and before `resolve_stmt_exprs`.
//!
//! Key details:
//! - Field selection mirrors `stmt_exprs::resolve_stmt_exprs`: only current-scope expression
//!   fields are rewritten. Nested bodies (if/loop/switch/try bodies, function/class bodies,
//!   methods, `Synthetic`, `IncludeOnceGuard`, `NamespaceBlock`) are left opaque — the engine
//!   resolves them in isolation and re-runs this hoister on them, so a deep include inside a body
//!   is still expanded at the right scope.
//! - `IncludeValue` under a short-circuit operand (`&&`, `||`, `??`, `?:`, ternary branches,
//!   nullsafe chains) is conditionally evaluated in PHP and cannot be eagerly inlined, so it is
//!   reported as an error rather than silently miscompiled. Likewise `IncludeValue` in a
//!   `while`/`do-while` condition or a `for` init/condition/update is rejected (re-evaluated each
//!   iteration).
//! - Expanded includes mutate shared `state` exactly like the direct-RHS path; the inlined
//!   include body is already fully resolved and accumulated in `ctx.hoisted` to be emitted ahead
//!   of the rewritten statement.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::parser::ast::{BinOp, CallableTarget, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind};
use crate::span::Span;

use super::contains::{expr_has_includes, stmt_has_includes};
use super::discovery::FunctionVariantRegistry;
use super::engine_includes::expand_value_include_core;
use super::state::ResolveState;

/// Shared context threaded through expression rewriting for a single statement.
///
/// Carries the resolver inputs plus an accumulator (`hoisted`) for already-resolved inlined
/// include bodies that must be emitted before the rewritten statement, in source order.
struct HoistCtx<'a> {
    base_dir: &'a Path,
    declared_once: &'a mut HashSet<PathBuf>,
    include_chain: &'a mut Vec<PathBuf>,
    state: &'a mut ResolveState,
    function_variants: &'a FunctionVariantRegistry,
    hoisted: Vec<Stmt>,
}

/// Error reported when an `IncludeValue` is found where PHP would evaluate it conditionally.
const CONDITIONAL_INCLUDE_MSG: &str =
    "include/require in a conditionally evaluated context (short-circuit operand, ternary \
     branch, nullsafe chain, or loop control) is not supported; hoist it to a statement that \
     always runs before this expression";

/// Hoists every `IncludeValue` inside `stmt`'s current-scope expression fields.
///
/// Each `IncludeValue` is expanded (the included file is inlined into the caller's scope) and
/// replaced by `Variable(tmp)`; the inlined bodies are returned in `hoisted` to be emitted before
/// the rewritten `stmt`. Bodies and declaration internals are left opaque (resolved in isolation
/// by the engine, which re-runs this hoister). Returns `(empty, stmt)` unchanged when the
/// statement contains no includes at all (fast path), so the common no-include statement pays only
/// for the cheap `stmt_has_includes` scan.
pub(super) fn hoist_value_includes_stmt(
    stmt: Stmt,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
    function_variants: &FunctionVariantRegistry,
) -> Result<(Vec<Stmt>, Stmt), CompileError> {
    let mut ctx = HoistCtx {
        base_dir,
        declared_once,
        include_chain,
        state,
        function_variants,
        hoisted: Vec::new(),
    };
    let rewritten = hoist_stmt(stmt, &mut ctx)?;
    let hoisted = std::mem::take(&mut ctx.hoisted);
    Ok((hoisted, rewritten))
}

/// Internal recursive form operating on a built [`HoistCtx`]; rewrites current-scope expression
/// fields and leaves bodies/declarations opaque.
fn hoist_stmt(stmt: Stmt, ctx: &mut HoistCtx) -> Result<Stmt, CompileError> {
    if !stmt_has_includes(&stmt) {
        return Ok(stmt);
    }
    let span = stmt.span;
    let attributes = stmt.attributes.clone();
    let kind = hoist_stmt_kind(stmt.kind, span, ctx)?;
    Ok(Stmt::with_attributes(kind, span, attributes))
}

/// Dispatches `hoist_value_includes_stmt` over a consumed `StmtKind`, rewriting current-scope
/// expression fields and passing bodies/declarations through unchanged.
fn hoist_stmt_kind(kind: StmtKind, span: Span, ctx: &mut HoistCtx) -> Result<StmtKind, CompileError> {
    Ok(match kind {
        // -- wrappers carrying bodies the engine resolves in isolation; nothing to hoist here --
        StmtKind::Synthetic(body) => StmtKind::Synthetic(body),
        StmtKind::IncludeOnceGuard { label, body } => StmtKind::IncludeOnceGuard { label, body },
        StmtKind::NamespaceBlock { name, body } => StmtKind::NamespaceBlock { name, body },
        StmtKind::IncludeOnceMark { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::RefAssign { .. }
        | StmtKind::IfDef { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => kind,

        // -- single-expression statements: rewrite the value (always evaluated once) --
        StmtKind::Echo(expr) => StmtKind::Echo(rewrite_expr_includes(expr, true, ctx)?),
        StmtKind::Throw(expr) => StmtKind::Throw(rewrite_expr_includes(expr, true, ctx)?),
        StmtKind::ExprStmt(expr) => StmtKind::ExprStmt(rewrite_expr_includes(expr, true, ctx)?),
        StmtKind::Return(expr) => StmtKind::Return(
            expr.map(|e| rewrite_expr_includes(e, true, ctx)).transpose()?,
        ),
        StmtKind::Assign { name, value } => StmtKind::Assign {
            name,
            value: rewrite_expr_includes(value, true, ctx)?,
        },
        StmtKind::TypedAssign { type_expr, name, value } => StmtKind::TypedAssign {
            type_expr,
            name,
            value: rewrite_expr_includes(value, true, ctx)?,
        },
        StmtKind::ConstDecl { name, value } => StmtKind::ConstDecl {
            name,
            value: rewrite_expr_includes(value, true, ctx)?,
        },
        StmtKind::ListUnpack { vars, value } => StmtKind::ListUnpack {
            vars,
            value: rewrite_expr_includes(value, true, ctx)?,
        },
        StmtKind::StaticVar { name, init } => StmtKind::StaticVar {
            name,
            init: rewrite_expr_includes(init, true, ctx)?,
        },
        StmtKind::ArrayAssign { array, index, value } => StmtKind::ArrayAssign {
            array,
            index: rewrite_expr_includes(index, true, ctx)?,
            value: rewrite_expr_includes(value, true, ctx)?,
        },
        StmtKind::NestedArrayAssign { target, value } => StmtKind::NestedArrayAssign {
            target: rewrite_expr_includes(target, true, ctx)?,
            value: rewrite_expr_includes(value, true, ctx)?,
        },
        StmtKind::ArrayPush { array, value } => StmtKind::ArrayPush {
            array,
            value: rewrite_expr_includes(value, true, ctx)?,
        },
        StmtKind::PropertyAssign { object, property, value } => StmtKind::PropertyAssign {
            object: Box::new(rewrite_expr_includes(*object, true, ctx)?),
            property,
            value: rewrite_expr_includes(value, true, ctx)?,
        },
        StmtKind::PropertyArrayPush { object, property, value } => StmtKind::PropertyArrayPush {
            object: Box::new(rewrite_expr_includes(*object, true, ctx)?),
            property,
            value: rewrite_expr_includes(value, true, ctx)?,
        },
        StmtKind::PropertyArrayAssign { object, property, index, value } => {
            StmtKind::PropertyArrayAssign {
                object: Box::new(rewrite_expr_includes(*object, true, ctx)?),
                property,
                index: rewrite_expr_includes(index, true, ctx)?,
                value: rewrite_expr_includes(value, true, ctx)?,
            }
        }
        StmtKind::StaticPropertyAssign { receiver, property, value } => {
            StmtKind::StaticPropertyAssign {
                receiver,
                property,
                value: rewrite_expr_includes(value, true, ctx)?,
            }
        }
        StmtKind::StaticPropertyArrayPush { receiver, property, value } => {
            StmtKind::StaticPropertyArrayPush {
                receiver,
                property,
                value: rewrite_expr_includes(value, true, ctx)?,
            }
        }
        StmtKind::StaticPropertyArrayAssign { receiver, property, index, value } => {
            StmtKind::StaticPropertyArrayAssign {
                receiver,
                property,
                index: rewrite_expr_includes(index, true, ctx)?,
                value: rewrite_expr_includes(value, true, ctx)?,
            }
        }
        StmtKind::Include { path, once, required } => StmtKind::Include {
            path: rewrite_expr_includes(path, true, ctx)?,
            once,
            required,
        },

        // -- control flow: conditions evaluated once (hoistable); bodies opaque --
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => StmtKind::If {
            condition: rewrite_expr_includes(condition, true, ctx)?,
            then_body,
            elseif_clauses: elseif_clauses
                .into_iter()
                .map(|(cond, body)| Ok((rewrite_expr_includes(cond, true, ctx)?, body)))
                .collect::<Result<Vec<_>, CompileError>>()?,
            else_body,
        },
        StmtKind::While { condition, body } => {
            reject_loop_control(span, "while", &condition)?;
            StmtKind::While { condition, body }
        }
        StmtKind::DoWhile { body, condition } => {
            reject_loop_control(span, "do-while", &condition)?;
            StmtKind::DoWhile { body, condition }
        }
        StmtKind::For { init, condition, update, body } => {
            if init.as_ref().is_some_and(|s| stmt_has_includes(s))
                || condition.as_ref().is_some_and(expr_has_includes)
                || update.as_ref().is_some_and(|s| stmt_has_includes(s))
            {
                return Err(CompileError::new(span, CONDITIONAL_INCLUDE_MSG));
            }
            StmtKind::For { init, condition, update, body }
        }
        StmtKind::Foreach { array, key_var, value_var, value_by_ref, body } => StmtKind::Foreach {
            array: rewrite_expr_includes(array, true, ctx)?,
            key_var,
            value_var,
            value_by_ref,
            body,
        },
        StmtKind::Switch { subject, cases, default } => StmtKind::Switch {
            subject: rewrite_expr_includes(subject, true, ctx)?,
            cases: cases
                .into_iter()
                .map(|(values, body)| {
                    Ok((
                        values
                            .into_iter()
                            .map(|v| rewrite_expr_includes(v, true, ctx))
                            .collect::<Result<Vec<_>, CompileError>>()?,
                        body,
                    ))
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
            default,
        },
        // try/catch/finally bodies are resolved in isolation.
        StmtKind::Try { try_body, catches, finally_body } => {
            StmtKind::Try { try_body, catches, finally_body }
        }

        // -- declarations: params/properties/methods/bodies resolved in isolation; opaque here --
        StmtKind::FunctionDecl { name, params, variadic, variadic_type, return_type, body } => {
            StmtKind::FunctionDecl { name, params, variadic, variadic_type, return_type, body }
        }
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_final,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
            constants,
        } => StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_final,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
            constants,
        },
        StmtKind::InterfaceDecl { name, extends, properties, methods, constants } => {
            StmtKind::InterfaceDecl { name, extends, properties, methods, constants }
        }
        StmtKind::TraitDecl { name, trait_uses, properties, methods, constants } => {
            StmtKind::TraitDecl { name, trait_uses, properties, methods, constants }
        }
        StmtKind::EnumDecl { name, backing_type, cases, implements, methods, constants } => {
            StmtKind::EnumDecl { name, backing_type, cases, implements, methods, constants }
        }
    })
}

/// Rejects an `IncludeValue` in a loop condition, which PHP re-evaluates each iteration and so
/// cannot be eagerly inlined once before the loop.
fn reject_loop_control(span: Span, what: &str, condition: &Expr) -> Result<(), CompileError> {
    if expr_has_includes(condition) {
        return Err(CompileError::new(
            span,
            &format!(
                "include/require in a {what} condition is not supported (re-evaluated each \
                 iteration); hoist it to a statement before the loop"
            ),
        ));
    }
    Ok(())
}

/// Reports whether a binary operator short-circuits its right operand (so the right side is
/// conditionally evaluated in PHP).
fn is_short_circuit(op: &BinOp) -> bool {
    matches!(op, BinOp::And | BinOp::Or | BinOp::NullCoalesce)
}

/// Rewrites every `IncludeValue` inside `expr`, replacing each with `Variable(tmp)` and pushing
/// the inlined include body onto `ctx.hoisted`.
///
/// `always` indicates the expression is guaranteed to be evaluated (true at statement top level
/// and through non-short-circuiting parents). It drops to `false` under short-circuit operands
/// (`&&`/`||`/`??`/`?:`/ternary branches/nullsafe chains), where an include is conditionally
/// evaluated in PHP; finding an `IncludeValue` there is an error rather than an eager miscompile.
fn rewrite_expr_includes(expr: Expr, always: bool, ctx: &mut HoistCtx) -> Result<Expr, CompileError> {
    let span = expr.span;
    let kind = match expr.kind {
        ExprKind::IncludeValue { path, once, required } => {
            if !always {
                return Err(CompileError::new(span, CONDITIONAL_INCLUDE_MSG));
            }
            let (mut hoisted, tmp) = expand_value_include_core(
                span,
                &path,
                once,
                required,
                ctx.base_dir,
                ctx.declared_once,
                ctx.include_chain,
                ctx.state,
                ctx.function_variants,
            )?;
            ctx.hoisted.append(&mut hoisted);
            ExprKind::Variable(tmp)
        }
        ExprKind::BinaryOp { left, op, right } => {
            let right_always = always && !is_short_circuit(&op);
            ExprKind::BinaryOp {
                left: Box::new(rewrite_expr_includes(*left, always, ctx)?),
                op,
                right: Box::new(rewrite_expr_includes(*right, right_always, ctx)?),
            }
        }
        ExprKind::InstanceOf { value, target } => ExprKind::InstanceOf {
            value: Box::new(rewrite_expr_includes(*value, always, ctx)?),
            target: rewrite_instanceof_target(target, always, ctx)?,
        },
        ExprKind::Negate(inner) => ExprKind::Negate(Box::new(rewrite_expr_includes(*inner, always, ctx)?)),
        ExprKind::Not(inner) => ExprKind::Not(Box::new(rewrite_expr_includes(*inner, always, ctx)?)),
        ExprKind::BitNot(inner) => ExprKind::BitNot(Box::new(rewrite_expr_includes(*inner, always, ctx)?)),
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(rewrite_expr_includes(*inner, always, ctx)?)),
        ExprKind::ErrorSuppress(inner) => {
            ExprKind::ErrorSuppress(Box::new(rewrite_expr_includes(*inner, always, ctx)?))
        }
        ExprKind::Print(inner) => ExprKind::Print(Box::new(rewrite_expr_includes(*inner, always, ctx)?)),
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(rewrite_expr_includes(*inner, always, ctx)?)),
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target,
            expr: Box::new(rewrite_expr_includes(*expr, always, ctx)?),
        },
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(rewrite_expr_includes(*expr, always, ctx)?),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type,
            len: Box::new(rewrite_expr_includes(*len, always, ctx)?),
        },
        ExprKind::YieldFrom(inner) => ExprKind::YieldFrom(Box::new(rewrite_expr_includes(*inner, always, ctx)?)),
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(rewrite_expr_includes(*value, always, ctx)?),
            default: Box::new(rewrite_expr_includes(*default, false, ctx)?),
        },
        ExprKind::Pipe { value, callable } => ExprKind::Pipe {
            value: Box::new(rewrite_expr_includes(*value, always, ctx)?),
            callable: Box::new(rewrite_expr_includes(*callable, always, ctx)?),
        },
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => {
            // `prelude` is parser-generated compound-assignment desugaring (no includes); it is
            // resolved in isolation downstream, so it is left opaque here.
            ExprKind::Assignment {
                target: Box::new(rewrite_expr_includes(*target, always, ctx)?),
                value: Box::new(rewrite_expr_includes(*value, always, ctx)?),
                result_target: result_target
                    .map(|t| rewrite_expr_includes(*t, always, ctx))
                    .transpose()?
                    .map(Box::new),
                prelude,
                conditional_value_temp,
            }
        }
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name,
            args: rewrite_exprs_includes(args, always, ctx)?,
        },
        ExprKind::ArrayLiteral(items) => ExprKind::ArrayLiteral(rewrite_exprs_includes(items, always, ctx)?),
        ExprKind::ArrayLiteralAssoc(entries) => ExprKind::ArrayLiteralAssoc(
            entries
                .into_iter()
                .map(|(k, v)| {
                    Ok((
                        rewrite_expr_includes(k, always, ctx)?,
                        rewrite_expr_includes(v, always, ctx)?,
                    ))
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
        ),
        ExprKind::Match { subject, arms, default } => ExprKind::Match {
            subject: Box::new(rewrite_expr_includes(*subject, always, ctx)?),
            // Only the matching arm's value runs in PHP, so patterns and arm values are
            // conditionally evaluated; an include there is rejected.
            arms: arms
                .into_iter()
                .map(|(patterns, value)| {
                    Ok((
                        rewrite_exprs_includes(patterns, false, ctx)?,
                        rewrite_expr_includes(value, false, ctx)?,
                    ))
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
            default: default
                .map(|d| rewrite_expr_includes(*d, false, ctx))
                .transpose()?
                .map(Box::new),
        },
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(rewrite_expr_includes(*array, always, ctx)?),
            index: Box::new(rewrite_expr_includes(*index, always, ctx)?),
        },
        ExprKind::Ternary { condition, then_expr, else_expr } => ExprKind::Ternary {
            condition: Box::new(rewrite_expr_includes(*condition, always, ctx)?),
            then_expr: Box::new(rewrite_expr_includes(*then_expr, false, ctx)?),
            else_expr: Box::new(rewrite_expr_includes(*else_expr, false, ctx)?),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary {
            value: Box::new(rewrite_expr_includes(*value, always, ctx)?),
            default: Box::new(rewrite_expr_includes(*default, false, ctx)?),
        },
        // Closures form a nested scope resolved in isolation; their internals (params, body) are
        // handled when the closure body is isolated-resolved, so the whole node is left opaque.
        ExprKind::Closure {
            params,
            variadic,
            variadic_type,
            return_type,
            body,
            is_arrow,
            is_static,
            captures,
            capture_refs,
        } => ExprKind::Closure {
            params,
            variadic,
            variadic_type,
            return_type,
            body,
            is_arrow,
            is_static,
            captures,
            capture_refs,
        },
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name,
            value: Box::new(rewrite_expr_includes(*value, always, ctx)?),
        },
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var,
            args: rewrite_exprs_includes(args, always, ctx)?,
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(rewrite_expr_includes(*callee, always, ctx)?),
            args: rewrite_exprs_includes(args, always, ctx)?,
        },
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name,
            args: rewrite_exprs_includes(args, always, ctx)?,
        },
        ExprKind::NewDynamic { name_expr, args } => ExprKind::NewDynamic {
            name_expr: Box::new(rewrite_expr_includes(*name_expr, always, ctx)?),
            args: rewrite_exprs_includes(args, always, ctx)?,
        },
        ExprKind::NewDynamicObject { class_name, fallback_class, required_parent, args } => {
            ExprKind::NewDynamicObject {
                class_name: Box::new(rewrite_expr_includes(*class_name, always, ctx)?),
                fallback_class,
                required_parent,
                args: rewrite_exprs_includes(args, always, ctx)?,
            }
        }
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(rewrite_expr_includes(*object, always, ctx)?),
            property,
        },
        ExprKind::DynamicPropertyAccess { object, property } => ExprKind::DynamicPropertyAccess {
            object: Box::new(rewrite_expr_includes(*object, always, ctx)?),
            property: Box::new(rewrite_expr_includes(*property, always, ctx)?),
        },
        ExprKind::NullsafePropertyAccess { object, property } => ExprKind::NullsafePropertyAccess {
            object: Box::new(rewrite_expr_includes(*object, always, ctx)?),
            property,
        },
        // `?->$prop`: the property expression is only evaluated when the object is non-null.
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            ExprKind::NullsafeDynamicPropertyAccess {
                object: Box::new(rewrite_expr_includes(*object, always, ctx)?),
                property: Box::new(rewrite_expr_includes(*property, false, ctx)?),
            }
        }
        ExprKind::MethodCall { object, method, args } => ExprKind::MethodCall {
            object: Box::new(rewrite_expr_includes(*object, always, ctx)?),
            method,
            args: rewrite_exprs_includes(args, always, ctx)?,
        },
        // `$obj?->method(args)`: args are only evaluated when the object is non-null.
        ExprKind::NullsafeMethodCall { object, method, args } => ExprKind::NullsafeMethodCall {
            object: Box::new(rewrite_expr_includes(*object, always, ctx)?),
            method,
            args: rewrite_exprs_includes(args, false, ctx)?,
        },
        ExprKind::StaticMethodCall { receiver, method, args } => ExprKind::StaticMethodCall {
            receiver,
            method,
            args: rewrite_exprs_includes(args, always, ctx)?,
        },
        ExprKind::StaticPropertyAccess { receiver, property } => {
            ExprKind::StaticPropertyAccess { receiver, property }
        }
        ExprKind::NewScopedObject { receiver, args } => ExprKind::NewScopedObject {
            receiver,
            args: rewrite_exprs_includes(args, always, ctx)?,
        },
        ExprKind::FirstClassCallable(target) => {
            ExprKind::FirstClassCallable(rewrite_callable_target(target, always, ctx)?)
        }
        ExprKind::Yield { key, value } => ExprKind::Yield {
            key: key.map(|k| rewrite_expr_includes(*k, always, ctx)).transpose()?.map(Box::new),
            value: value.map(|v| rewrite_expr_includes(*v, always, ctx)).transpose()?.map(Box::new),
        },
        // Leaf expressions with no sub-expressions.
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
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => expr.kind,
    };
    Ok(Expr::new(kind, span))
}

/// Maps [`rewrite_expr_includes`] over a vector of expressions, preserving order.
fn rewrite_exprs_includes(
    exprs: Vec<Expr>,
    always: bool,
    ctx: &mut HoistCtx,
) -> Result<Vec<Expr>, CompileError> {
    exprs.into_iter().map(|e| rewrite_expr_includes(e, always, ctx)).collect()
}

/// Rewrites the object expression inside a callable target; `Function` and `StaticMethod`
/// variants carry no expressions and pass through unchanged.
fn rewrite_callable_target(
    target: CallableTarget,
    always: bool,
    ctx: &mut HoistCtx,
) -> Result<CallableTarget, CompileError> {
    Ok(match target {
        CallableTarget::Function(name) => CallableTarget::Function(name),
        CallableTarget::StaticMethod { receiver, method } => {
            CallableTarget::StaticMethod { receiver, method }
        }
        CallableTarget::Method { object, method } => CallableTarget::Method {
            object: Box::new(rewrite_expr_includes(*object, always, ctx)?),
            method,
        },
    })
}

/// Rewrites the target expression inside an `InstanceOf`; the `Name` form carries no expression.
fn rewrite_instanceof_target(
    target: InstanceOfTarget,
    always: bool,
    ctx: &mut HoistCtx,
) -> Result<InstanceOfTarget, CompileError> {
    match target {
        InstanceOfTarget::Name(name) => Ok(InstanceOfTarget::Name(name)),
        InstanceOfTarget::Expr(expr) => Ok(InstanceOfTarget::Expr(Box::new(rewrite_expr_includes(
            *expr,
            always,
            ctx,
        )?))),
    }
}