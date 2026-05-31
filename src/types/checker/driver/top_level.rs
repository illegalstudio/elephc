//! Purpose:
//! Implements the checker driver top level phase.
//! Owns one ordered step in building checker state and validating the program before optimization/codegen.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()`
//!
//! Key details:
//! - Phase order controls diagnostics, available declarations, required libraries, and function-local environments.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, InstanceOfTarget, Program, Stmt, StmtKind};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

impl Checker {
    /// Runs the top-level type-checking pass over the full program.
    ///
    /// Processes each statement in order, maintaining a shared `global_env` that accumulates
    /// declarations across the entire program. Each statement is checked in a fresh `top_level_env`
    /// cloned from the current global state. Returns the final `TypeEnv` and a vector of error
    /// vectors (one per statement) for structured diagnostics.
    pub(super) fn check_top_level_program(
        &mut self,
        program: &Program,
    ) -> (TypeEnv, Vec<Vec<CompileError>>) {
        let mut global_env = self.seed_global_env();
        let mut all_errors = Vec::with_capacity(program.len());
        for stmt in program {
            self.top_level_env = global_env.clone();
            let stmt_errors = self
                .check_stmt(stmt, &mut global_env)
                .err()
                .map(|error| error.flatten())
                .unwrap_or_default();
            all_errors.push(stmt_errors);
        }
        (global_env, all_errors)
    }

    /// Determines whether top-level errors for a statement can be suppressed.
    ///
    /// Suppression is allowed for stale diagnostics from the initial pass when the final pass has
    /// no error for the same statement. This covers method/property forward-reference diagnostics
    /// plus callable metadata and undefined-variable cascades that disappear after method schemas
    /// have been updated.
    pub(super) fn can_suppress_initial_top_level_errors(
        stmt: &Stmt,
        errors: &[CompileError],
    ) -> bool {
        if Self::can_suppress_stale_undefined_variable_errors(errors) {
            return true;
        }
        if Self::can_suppress_late_callable_metadata_errors(errors) {
            return true;
        }
        !errors.is_empty()
            && (Self::stmt_contains_method_call(stmt)
                || Self::stmt_contains_property_access(stmt))
            && errors
                .iter()
                .all(|error| Self::is_suppressible_initial_top_level_error(&error.message))
    }

    /// Returns true for initial-pass callable metadata errors that disappeared in the final pass.
    fn can_suppress_late_callable_metadata_errors(errors: &[CompileError]) -> bool {
        errors
            .iter()
            .any(|error| Self::is_late_callable_metadata_error(&error.message))
            && errors.iter().all(|error| {
                Self::is_late_callable_metadata_error(&error.message)
                    || error.message.starts_with("Undefined variable: $")
            })
    }

    /// Returns true for undefined-variable cascades that disappeared in the final pass.
    fn can_suppress_stale_undefined_variable_errors(errors: &[CompileError]) -> bool {
        !errors.is_empty()
            && errors
                .iter()
                .all(|error| error.message.starts_with("Undefined variable: $"))
    }

    /// Returns true for stale diagnostics caused by method-return callable metadata.
    fn is_late_callable_metadata_error(message: &str) -> bool {
        message.contains("must have a statically known callable signature")
    }

    /// Returns `true` if the given error message is in the suppressible set for initial top-level errors.
    ///
    /// Suppressible messages include array-index, property-access, and callable-related diagnostics
    /// that commonly arise when a class is referenced before its definition.
    fn is_suppressible_initial_top_level_error(message: &str) -> bool {
        matches!(
            message,
            "Array index must be integer"
                | "Cannot index non-array"
                | "Property access requires an object or typed pointer"
        ) || (message.starts_with("Cannot call $") && message.contains("not a callable"))
    }

    /// Builds the initial `TypeEnv` with built-in globals `$argc`, `$argv`, and external globals.
    ///
    /// `$argc` is typed as `Int`; `$argv` is typed as `Array<Str>`. External globals from
    /// `self.extern_globals` are inserted verbatim. The returned environment serves as the
    /// starting point for top-level type checking.
    fn seed_global_env(&self) -> TypeEnv {
        let mut global_env: TypeEnv = HashMap::new();
        global_env.insert("argc".to_string(), PhpType::Int);
        global_env.insert("argv".to_string(), PhpType::Array(Box::new(PhpType::Str)));
        for (name, ty) in &self.extern_globals {
            global_env.insert(name.clone(), ty.clone());
        }
        global_env
    }

    /// Returns `true` if the statement contains a method call anywhere in its expression tree.
    ///
    /// Recursively walks `StmtKind` variants that carry expressions, delegating to
    /// `expr_contains_method_call` for expression-level traversal. Used to detect whether
    /// a top-level statement may reference a class not yet defined, enabling error suppression.
    fn stmt_contains_method_call(stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::Synthetic(stmts) => stmts.iter().any(Self::stmt_contains_method_call),
            StmtKind::ExprStmt(expr)
            | StmtKind::Echo(expr)
            | StmtKind::Return(Some(expr)) => Self::expr_contains_method_call(expr),
            StmtKind::Assign { value, .. }
            | StmtKind::TypedAssign { value, .. }
            | StmtKind::ConstDecl { value, .. }
            | StmtKind::ListUnpack { value, .. } => Self::expr_contains_method_call(value),
            StmtKind::RefAssign { .. } => false,
            StmtKind::ArrayAssign { index, value, .. } => {
                Self::expr_contains_method_call(index) || Self::expr_contains_method_call(value)
            }
            StmtKind::NestedArrayAssign { target, value } => {
                Self::expr_contains_method_call(target) || Self::expr_contains_method_call(value)
            }
            StmtKind::ArrayPush { value, .. } => Self::expr_contains_method_call(value),
            StmtKind::StaticPropertyAssign { value, .. }
            | StmtKind::StaticPropertyArrayPush { value, .. } => {
                Self::expr_contains_method_call(value)
            }
            StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
                Self::expr_contains_method_call(index) || Self::expr_contains_method_call(value)
            }
            StmtKind::PropertyAssign { object, value, .. } => {
                Self::expr_contains_method_call(object) || Self::expr_contains_method_call(value)
            }
            StmtKind::PropertyArrayPush { object, value, .. } => {
                Self::expr_contains_method_call(object) || Self::expr_contains_method_call(value)
            }
            StmtKind::PropertyArrayAssign {
                object,
                index,
                value,
                ..
            } => {
                Self::expr_contains_method_call(object)
                    || Self::expr_contains_method_call(index)
                    || Self::expr_contains_method_call(value)
            }
            _ => false,
        }
    }

    /// Returns `true` if the expression tree contains a method call.
    ///
    /// Recursively traverses all `ExprKind` variants, returning `true` for `MethodCall` and
    /// `NullsafeMethodCall`, and recursing into sub-expressions for container variants.
    /// Returns `false` for literal and variable expressions that cannot contain calls.
    fn expr_contains_method_call(expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::MethodCall { object, args, .. }
            | ExprKind::NullsafeMethodCall { object, args, .. } => {
                Self::expr_contains_method_call(object)
                    || args.iter().any(Self::expr_contains_method_call)
                    || true
            }
            ExprKind::PropertyAccess { object, .. }
            | ExprKind::NullsafePropertyAccess { object, .. }
            | ExprKind::Negate(object)
            | ExprKind::Not(object)
            | ExprKind::BitNot(object)
            | ExprKind::Spread(object)
            | ExprKind::ErrorSuppress(object)
            | ExprKind::Print(object)
            | ExprKind::Throw(object) => Self::expr_contains_method_call(object),
            ExprKind::DynamicPropertyAccess { object, property }
            | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
                Self::expr_contains_method_call(object)
                    || Self::expr_contains_method_call(property)
            }
            ExprKind::ArrayAccess { array, index } => {
                Self::expr_contains_method_call(array) || Self::expr_contains_method_call(index)
            }
            ExprKind::BinaryOp { left, right, .. } => {
                Self::expr_contains_method_call(left) || Self::expr_contains_method_call(right)
            }
            ExprKind::InstanceOf { value, target } => Self::expr_contains_method_call(value)
                || Self::instanceof_target_contains_method_call(target),
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                Self::expr_contains_method_call(condition)
                    || Self::expr_contains_method_call(then_expr)
                    || Self::expr_contains_method_call(else_expr)
            }
            ExprKind::ShortTernary { value, default }
            | ExprKind::NullCoalesce { value, default } => {
                Self::expr_contains_method_call(value)
                    || Self::expr_contains_method_call(default)
            }
            ExprKind::Pipe { value, callable } => {
                Self::expr_contains_method_call(value)
                    || Self::expr_contains_method_call(callable)
            }
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                ..
            } => {
                Self::expr_contains_method_call(target)
                    || Self::expr_contains_method_call(value)
                    || result_target
                        .as_deref()
                        .is_some_and(Self::expr_contains_method_call)
                    || prelude.iter().any(Self::stmt_contains_method_call)
            }
            ExprKind::FunctionCall { args, .. }
            | ExprKind::ClosureCall { args, .. }
            | ExprKind::ExprCall { args, .. }
            | ExprKind::StaticMethodCall { args, .. }
            | ExprKind::NewObject { args, .. } => {
                args.iter().any(Self::expr_contains_method_call)
            }
            ExprKind::NewDynamicObject {
                class_name, args, ..
            } => {
                Self::expr_contains_method_call(class_name)
                    || args.iter().any(Self::expr_contains_method_call)
            }
            ExprKind::Match {
                subject,
                arms,
                default,
            } => {
                Self::expr_contains_method_call(subject)
                    || arms.iter().any(|(conditions, result)| {
                        conditions.iter().any(Self::expr_contains_method_call)
                            || Self::expr_contains_method_call(result)
                    })
                    || default
                        .as_ref()
                        .map(|expr| Self::expr_contains_method_call(expr))
                        .unwrap_or(false)
            }
            ExprKind::ArrayLiteral(items) => {
                items.iter().any(Self::expr_contains_method_call)
            }
            ExprKind::ArrayLiteralAssoc(items) => items.iter().any(
                |(key, value)| {
                    Self::expr_contains_method_call(key) || Self::expr_contains_method_call(value)
                },
            ),
            ExprKind::Cast { expr, .. }
            | ExprKind::PtrCast { expr, .. }
            | ExprKind::NamedArg { value: expr, .. } => {
                Self::expr_contains_method_call(expr)
            }
            ExprKind::Closure { .. }
            | ExprKind::FirstClassCallable(_)
            | ExprKind::StaticPropertyAccess { .. }
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::Variable(_)
            | ExprKind::PreIncrement(_)
            | ExprKind::PostIncrement(_)
            | ExprKind::PreDecrement(_)
            | ExprKind::PostDecrement(_)
            | ExprKind::ConstRef(_)
            | ExprKind::This
            | ExprKind::BufferNew { .. } => false,
            ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. } => false,
            ExprKind::NewScopedObject { args, .. } => {
                args.iter().any(Self::expr_contains_method_call)
            }
            ExprKind::Yield { key, value } => {
                key.as_ref().is_some_and(|k| Self::expr_contains_method_call(k))
                    || value
                        .as_ref()
                        .is_some_and(|v| Self::expr_contains_method_call(v))
            }
            ExprKind::YieldFrom(inner) => Self::expr_contains_method_call(inner),
            ExprKind::MagicConstant(_) => {
                unreachable!("MagicConstant must be lowered before type checking")
            }
        }
    }

    /// Returns `true` if the statement contains a property access anywhere in its expression tree.
    ///
    /// Recursively walks `StmtKind` variants that carry expressions, delegating to
    /// `expr_contains_property_access` for expression-level traversal. Used to detect whether
    /// a top-level statement may reference a class not yet defined, enabling error suppression.
    fn stmt_contains_property_access(stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::Synthetic(stmts) => stmts.iter().any(Self::stmt_contains_property_access),
            StmtKind::ExprStmt(expr)
            | StmtKind::Echo(expr)
            | StmtKind::Return(Some(expr)) => Self::expr_contains_property_access(expr),
            StmtKind::Assign { value, .. }
            | StmtKind::TypedAssign { value, .. }
            | StmtKind::ConstDecl { value, .. }
            | StmtKind::ListUnpack { value, .. } => Self::expr_contains_property_access(value),
            StmtKind::RefAssign { .. } => false,
            StmtKind::ArrayAssign { index, value, .. } => {
                Self::expr_contains_property_access(index)
                    || Self::expr_contains_property_access(value)
            }
            StmtKind::NestedArrayAssign { target, value } => {
                Self::expr_contains_property_access(target)
                    || Self::expr_contains_property_access(value)
            }
            StmtKind::ArrayPush { value, .. } => Self::expr_contains_property_access(value),
            StmtKind::StaticPropertyAssign { value, .. }
            | StmtKind::StaticPropertyArrayPush { value, .. } => {
                Self::expr_contains_property_access(value)
            }
            StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
                Self::expr_contains_property_access(index)
                    || Self::expr_contains_property_access(value)
            }
            StmtKind::PropertyAssign { .. } | StmtKind::PropertyArrayPush { .. } => true,
            StmtKind::PropertyArrayAssign {
                ..
            } => true,
            _ => false,
        }
    }

    /// Returns `true` if the expression tree contains a property access.
    ///
    /// Recursively traverses all `ExprKind` variants, returning `true` for `PropertyAccess`,
    /// `NullsafePropertyAccess`, `DynamicPropertyAccess`, and `NullsafeDynamicPropertyAccess`,
    /// and recursing into sub-expressions for container variants.
    fn expr_contains_property_access(expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::PropertyAccess { .. }
            | ExprKind::NullsafePropertyAccess { .. }
            | ExprKind::DynamicPropertyAccess { .. }
            | ExprKind::NullsafeDynamicPropertyAccess { .. } => true,
            ExprKind::MethodCall { object, args, .. }
            | ExprKind::NullsafeMethodCall { object, args, .. } => {
                Self::expr_contains_property_access(object)
                    || args.iter().any(Self::expr_contains_property_access)
            }
            ExprKind::ArrayAccess { array, index } => {
                Self::expr_contains_property_access(array)
                    || Self::expr_contains_property_access(index)
            }
            ExprKind::BinaryOp { left, right, .. } => {
                Self::expr_contains_property_access(left)
                    || Self::expr_contains_property_access(right)
            }
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                Self::expr_contains_property_access(condition)
                    || Self::expr_contains_property_access(then_expr)
                    || Self::expr_contains_property_access(else_expr)
            }
            ExprKind::ShortTernary { value, default }
            | ExprKind::NullCoalesce { value, default } => {
                Self::expr_contains_property_access(value)
                    || Self::expr_contains_property_access(default)
            }
            ExprKind::Pipe { value, callable } => {
                Self::expr_contains_property_access(value)
                    || Self::expr_contains_property_access(callable)
            }
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                ..
            } => {
                Self::expr_contains_property_access(target)
                    || Self::expr_contains_property_access(value)
                    || result_target
                        .as_deref()
                        .is_some_and(Self::expr_contains_property_access)
                    || prelude.iter().any(Self::stmt_contains_property_access)
            }
            ExprKind::FunctionCall { args, .. }
            | ExprKind::ClosureCall { args, .. }
            | ExprKind::ExprCall { args, .. }
            | ExprKind::StaticMethodCall { args, .. }
            | ExprKind::NewObject { args, .. }
            | ExprKind::NewScopedObject { args, .. } => {
                args.iter().any(Self::expr_contains_property_access)
            }
            ExprKind::NewDynamicObject {
                class_name, args, ..
            } => {
                Self::expr_contains_property_access(class_name)
                    || args.iter().any(Self::expr_contains_property_access)
            }
            ExprKind::Match {
                subject,
                arms,
                default,
            } => {
                Self::expr_contains_property_access(subject)
                    || arms.iter().any(|(conditions, result)| {
                        conditions
                            .iter()
                            .any(Self::expr_contains_property_access)
                            || Self::expr_contains_property_access(result)
                    })
                    || default
                        .as_ref()
                        .is_some_and(|expr| Self::expr_contains_property_access(expr))
            }
            ExprKind::ArrayLiteral(items) => {
                items.iter().any(Self::expr_contains_property_access)
            }
            ExprKind::ArrayLiteralAssoc(items) => items.iter().any(|(key, value)| {
                Self::expr_contains_property_access(key)
                    || Self::expr_contains_property_access(value)
            }),
            ExprKind::InstanceOf { value, target } => {
                Self::expr_contains_property_access(value)
                    || Self::instanceof_target_contains_property_access(target)
            }
            ExprKind::Negate(inner)
            | ExprKind::Not(inner)
            | ExprKind::BitNot(inner)
            | ExprKind::Spread(inner)
            | ExprKind::ErrorSuppress(inner)
            | ExprKind::Print(inner)
            | ExprKind::Throw(inner)
            | ExprKind::Cast { expr: inner, .. }
            | ExprKind::PtrCast { expr: inner, .. }
            | ExprKind::NamedArg { value: inner, .. }
            | ExprKind::YieldFrom(inner) => Self::expr_contains_property_access(inner),
            ExprKind::Yield { key, value } => {
                key.as_ref()
                    .is_some_and(|expr| Self::expr_contains_property_access(expr))
                    || value
                        .as_ref()
                        .is_some_and(|expr| Self::expr_contains_property_access(expr))
            }
            ExprKind::Closure { .. }
            | ExprKind::FirstClassCallable(_)
            | ExprKind::StaticPropertyAccess { .. }
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::Variable(_)
            | ExprKind::PreIncrement(_)
            | ExprKind::PostIncrement(_)
            | ExprKind::PreDecrement(_)
            | ExprKind::PostDecrement(_)
            | ExprKind::ConstRef(_)
            | ExprKind::This
            | ExprKind::BufferNew { .. }
            | ExprKind::ClassConstant { .. }
            | ExprKind::ScopedConstantAccess { .. } => false,
            ExprKind::MagicConstant(_) => {
                unreachable!("MagicConstant must be lowered before type checking")
            }
        }
    }

    /// Returns `true` if the `InstanceOfTarget` contains a property access.
    ///
    /// `InstanceOfTarget::Name` returns `false`; `InstanceOfTarget::Expr` delegates to
    /// `expr_contains_property_access`.
    fn instanceof_target_contains_property_access(target: &InstanceOfTarget) -> bool {
        match target {
            InstanceOfTarget::Name(_) => false,
            InstanceOfTarget::Expr(expr) => Self::expr_contains_property_access(expr),
        }
    }

    /// Returns `true` if the `InstanceOfTarget` contains a method call.
    ///
    /// `InstanceOfTarget::Name` returns `false`; `InstanceOfTarget::Expr` delegates to
    /// `expr_contains_method_call`.
    fn instanceof_target_contains_method_call(target: &InstanceOfTarget) -> bool {
        match target {
            InstanceOfTarget::Name(_) => false,
            InstanceOfTarget::Expr(expr) => Self::expr_contains_method_call(expr),
        }
    }
}
