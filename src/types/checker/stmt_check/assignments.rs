//! Purpose:
//! Validates statement assignments behavior.
//! Keeps control-flow and assignment effects synchronized with expression inference and return analysis.
//!
//! Called from:
//! - `crate::types::checker::stmt_check`
//!
//! Key details:
//! - Branch and loop handling must preserve PHP execution order and conservative type environments.

mod arrays;
mod locals;
mod properties;
mod properties_null_coalesce;
mod static_properties;

use crate::errors::CompileError;
use crate::parser::ast::{Stmt, StmtKind};
use crate::types::TypeEnv;

use super::super::Checker;

impl Checker {
    pub(crate) fn check_assignment_like_stmt(
        &mut self,
        stmt: &Stmt,
        env: &mut TypeEnv,
    ) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                locals::check_assign(self, name, value, stmt.span, env)
            }
            StmtKind::ArrayAssign {
                array,
                index,
                value,
            } => arrays::check_array_assign(self, array, index, value, stmt.span, env),
            StmtKind::NestedArrayAssign { target, value } => {
                arrays::check_nested_array_assign(self, target, value, stmt.span, env)
            }
            StmtKind::ArrayPush { array, value } => {
                arrays::check_array_push(self, array, value, stmt.span, env)
            }
            StmtKind::TypedAssign {
                type_expr,
                name,
                value,
            } => locals::check_typed_assign(self, type_expr, name, value, stmt.span, env),
            StmtKind::ConstDecl { name, value } => {
                locals::check_const_decl(self, name, value, env)
            }
            StmtKind::ListUnpack { vars, value } => {
                locals::check_list_unpack(self, vars, value, stmt.span, env)
            }
            StmtKind::Global { vars } => locals::check_global(self, vars, env),
            StmtKind::StaticVar { name, init } => {
                locals::check_static_var(self, name, init, env)
            }
            StmtKind::StaticPropertyAssign {
                receiver,
                property,
                value,
            } => static_properties::check_static_property_assign(
                self,
                receiver,
                property,
                value,
                stmt.span,
                env,
            ),
            StmtKind::StaticPropertyArrayPush {
                receiver,
                property,
                value,
            } => static_properties::check_static_property_array_push(
                self,
                receiver,
                property,
                value,
                stmt.span,
                env,
            ),
            StmtKind::StaticPropertyArrayAssign {
                receiver,
                property,
                index,
                value,
            } => static_properties::check_static_property_array_assign(
                self,
                receiver,
                property,
                index,
                value,
                stmt.span,
                env,
            ),
            StmtKind::PropertyAssign {
                object,
                property,
                value,
            } => properties::check_property_assign(
                self,
                object,
                property,
                value,
                stmt.span,
                env,
            ),
            StmtKind::PropertyArrayPush {
                object,
                property,
                value,
            } => properties::check_property_array_push(
                self,
                object,
                property,
                value,
                stmt.span,
                env,
            ),
            StmtKind::PropertyArrayAssign {
                object,
                property,
                index,
                value,
            } => properties::check_property_array_assign(
                self,
                object,
                property,
                index,
                value,
                stmt.span,
                env,
            ),
            _ => unreachable!("non-assignment statement routed to assignment checker"),
        }
    }
}
