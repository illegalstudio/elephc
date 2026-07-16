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
use crate::parser::ast::{ExprKind, Stmt, StmtKind, NESTED_APPEND_TEMP_PREFIX};
use crate::types::{normalized_array_key_type, PhpType, TypeEnv};

use super::super::Checker;

impl Checker {
    /// Type-checks the parser-generated `$array[$index][] = $value` sequence when
    /// an empty indexed base needs its element type auto-vivified to an array.
    ///
    /// Returns `true` only after consuming a recognized synthetic suffix. All
    /// other synthetic groups, associative keys, and already-typed bases remain
    /// on the ordinary statement-by-statement path.
    pub(crate) fn check_empty_indexed_nested_append(
        &mut self,
        body: &[Stmt],
        env: &mut TypeEnv,
    ) -> Result<bool, CompileError> {
        if body.len() < 3 {
            return Ok(false);
        }
        let split = body.len() - 3;
        let (prefix, triple) = body.split_at(split);
        let (temp, base, index) = match &triple[0].kind {
            StmtKind::Assign { name, value }
                if name.starts_with(NESTED_APPEND_TEMP_PREFIX) =>
            {
                match &value.kind {
                    ExprKind::ArrayAccess { array, index } => match &array.kind {
                        ExprKind::Variable(base) => (name.as_str(), base.as_str(), index.as_ref()),
                        _ => return Ok(false),
                    },
                    _ => return Ok(false),
                }
            }
            _ => return Ok(false),
        };
        if !matches!(
            &triple[1].kind,
            StmtKind::ArrayPush { array, .. } if array == temp
        ) || !matches!(
            &triple[2].kind,
            StmtKind::ArrayAssign { array, value, .. }
                if array == base
                    && matches!(&value.kind, ExprKind::Variable(name) if name == temp)
        ) {
            return Ok(false);
        }
        if !matches!(env.get(base), Some(PhpType::Array(element)) if **element == PhpType::Never) {
            return Ok(false);
        }

        for stmt in prefix {
            self.check_stmt(stmt, env)?;
        }
        let index_type = self.infer_type_with_assignment_effects(index, env)?;
        if normalized_array_key_type(index, index_type) != PhpType::Int {
            return Ok(false);
        }

        // PHP auto-vivifies the missing bucket as an empty indexed array. Seeding
        // the parser's hidden read temporary with that shape lets the ordinary
        // push checker infer `Array(T)` and the ordinary write-back checker infer
        // the outer `Array(Array(T))` without weakening user-visible assignments.
        env.insert(
            temp.to_string(),
            PhpType::Array(Box::new(PhpType::Never)),
        );
        self.check_stmt(&triple[1], env)?;
        self.check_stmt(&triple[2], env)?;
        Ok(true)
    }

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
