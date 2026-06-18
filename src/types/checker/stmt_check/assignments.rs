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
    /// Returns true when `name` is bound as a `foreach` loop key in the current
    /// scope. A foreach key is a boxed `Mixed` cell at runtime even when the
    /// checker types it as `Int`/`Str` from the source array, so an array write
    /// under such a key must defer the indexed-vs-hash decision to
    /// `Op::ArraySetMixedKey` (destination `Array(Mixed)`) instead of promoting
    /// the destination to `AssocArray` like a statically-known string key.
    pub(crate) fn is_foreach_key(&self, name: &str) -> bool {
        self.foreach_key_locals.contains(name)
    }

    /// Validates assignment-like statements, dispatching to specialized checkers per variant.
    ///
    /// # Parameters
    /// - `stmt`: The assignment statement to check
    /// - `env`: The current type environment (mutated in place)
    ///
    /// # Behavior
    /// Each `StmtKind` variant is dispatched to the appropriate sub-checker:
    /// - Simple assignments → `locals::check_assign`
    /// - Array operations → `arrays::*`
    /// - Property operations → `properties::*` / `static_properties::*`
    ///
    /// Sub-checkers validate type compatibility, mutability, and update `env` with new bindings.
    ///
    /// # Panics
    /// Panics if a non-assignment `StmtKind` reaches this function.
    pub(crate) fn check_assignment_like_stmt(
        &mut self,
        stmt: &Stmt,
        env: &mut TypeEnv,
    ) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                locals::check_assign(self, name, value, stmt.span, env)
            }
            StmtKind::RefAssign { target, source } => {
                locals::check_ref_assign(self, target, source, stmt.span, env)
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
