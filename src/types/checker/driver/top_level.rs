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

    pub(super) fn can_suppress_initial_top_level_errors(
        stmt: &Stmt,
        errors: &[CompileError],
    ) -> bool {
        !errors.is_empty()
            && Self::stmt_contains_method_call(stmt)
            && errors.iter().all(|error| {
                matches!(
                    error.message.as_str(),
                    "Cannot index non-array"
                        | "Property access requires an object or typed pointer"
                )
            })
    }

    fn seed_global_env(&self) -> TypeEnv {
        let mut global_env: TypeEnv = HashMap::new();
        global_env.insert("argc".to_string(), PhpType::Int);
        global_env.insert("argv".to_string(), PhpType::Array(Box::new(PhpType::Str)));
        for (name, ty) in &self.extern_globals {
            global_env.insert(name.clone(), ty.clone());
        }
        global_env
    }

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

    fn instanceof_target_contains_method_call(target: &InstanceOfTarget) -> bool {
        match target {
            InstanceOfTarget::Name(_) => false,
            InstanceOfTarget::Expr(expr) => Self::expr_contains_method_call(expr),
        }
    }
}
