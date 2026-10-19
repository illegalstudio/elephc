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
pub(crate) use properties::{refine_array_slots_for_dynamic_array_write, refine_array_slots_for_dynamic_push};
mod properties_null_coalesce;
mod reference_places;
mod static_properties;

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind, NESTED_APPEND_TEMP_PREFIX};
use crate::types::{normalized_array_key_type, PhpType, TypeEnv};

use super::super::Checker;

/// Returns the type a `static $x = <init>;` local carries at its declaration, from the type of
/// its initializer.
///
/// The value a static holds when the declaration is reached is whatever the PREVIOUS call left
/// in it, not the initializer. An initializer that says nothing about that value (`static $x;`,
/// `static $x = null;`, `static $x = [];`) would otherwise make every later read fold against
/// null or an empty array: `$x ??= make()` re-ran `make()` on every call and `$c[$k] ?? 0` never
/// saw the keys an earlier call stored. Such a static is boxed `Mixed` storage, read at run time.
/// An informative initializer (`static $n = 0;`, `static $m = ['a' => 1];`) keeps its type, and an
/// assignment of another type is refused or widened by the ordinary local rules.
/// An array initializer keeps the array contract as PHP's general (boxed) `array`, so array
/// builtins still accept it while its keys and elements are read at run time: a later call can
/// store any key or value into it (`static $u = ['x' => 0]; $u[3] = 1;` rewrote a string-keyed
/// hash slot as another layout and corrupted it).
///
/// A non-empty literal array keeps its precise element types when the body never writes the
/// static after its declaration (`static $map = ['x' => [1]]; return $map[$k];`): nothing can
/// change its layout, and `$map['x']` stays an `array` for `: array`, `array_map`, `sort`...
/// `written` is that body scan (`crate::ir_lower::written_static_locals`).
pub(crate) fn static_local_declared_type(init_ty: PhpType, written: bool) -> PhpType {
    match init_ty.codegen_repr() {
        PhpType::Void | PhpType::Never => PhpType::Mixed,
        PhpType::Array(element) if matches!(element.codegen_repr(), PhpType::Never | PhpType::Void) => {
            PhpType::php_array()
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } if written => PhpType::php_array(),
        _ => init_ty,
    }
}

/// Widens the untyped static array property an element chain starts from (`C::$p[$k]..`) to
/// the boxed PHP array representation its addressable (reference/unset) path needs.
/// `widen_null_default` also widens a null-defaulted slot, which a reference autovivifies into
/// an array; an `unset` leaves it null.
pub(crate) fn widen_static_property_element_root(
    checker: &mut Checker,
    container: &Expr,
    widen_null_default: bool,
) -> Result<(), CompileError> {
    if let Some((receiver, property)) = locals::element_static_property_root(container) {
        static_properties::widen_referenced_untyped_static_property(
            checker,
            receiver,
            property,
            container.span,
            widen_null_default,
        )?;
    }
    Ok(())
}

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

    /// Returns true when `index` is the counter of the `for` loop this write runs directly in.
    ///
    /// That is the one index a write into a still-EMPTY array can carry without leaving packed
    /// storage: the counter is `0` at the first write and advances one slot per iteration, so it
    /// never runs ahead of the array's length and no gap is reachable. The depth comparison is
    /// what "directly in" means — a write nested one conditional deeper may be skipped, and a
    /// skipped iteration is exactly how a gap appears.
    pub(crate) fn index_is_packed_loop_counter(&self, array: &str, index: &Expr) -> bool {
        let ExprKind::Variable(name) = &index.kind else {
            return false;
        };
        self.packed_loop_counter.as_ref().is_some_and(|counter| {
            counter.name == *name
                && counter.depth == self.local_conditional_depth
                && !counter.rebound_locals.contains(array)
        })
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
        // A by-reference array literal (`[&$this->p]`) lowers into its expression's prelude as
        // ordered writes, `Synthetic` groups and `unset(...)` statements (see
        // `parser::expr::prefix::build_reference_literal`); those are checked as the ordinary
        // statements they are.
        if matches!(stmt.kind, StmtKind::Synthetic(_) | StmtKind::ExprStmt(_)) {
            return self.check_stmt(stmt, env);
        }
        let result = match &stmt.kind {
            StmtKind::Assign { name, value } => {
                // The one statement-form local assignment: `stmt_form: true` makes an
                // incompatible reassignment eligible for the permissive retype (warn and
                // re-bind) instead of the hard `cannot reassign` error.
                locals::check_assign(self, name, value, stmt.span, env, true)
            }
            StmtKind::RefAssign { target, source } => {
                locals::check_ref_assign(self, target, source, stmt.span, env)
            }
            StmtKind::RefAssignPlace { target, source } => {
                reference_places::check_ref_assign_place(self, target, source, stmt.span, env)
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
        };
        if result.is_ok() {
            self.invalidate_property_narrowings_after_assignment(stmt, env);
        }
        result
    }

    /// Invalidates synthetic property facts affected by a completed statement assignment, then
    /// re-establishes the fact for the storage the statement itself wrote.
    ///
    /// Property writes can mutate an aliased object and therefore clear every fact; local
    /// rebindings clear only facts rooted at the rebound local. A plain
    /// `$this->p = <non-null>` / `self::$p = <non-null>` write is the one case where the
    /// post-write type of a place is known, so `record_property_assignment_narrowing` puts that
    /// single fact back — this is what lets `if (self::$p === null) { self::$p = new S(); }`
    /// leave `self::$p` non-null on both paths.
    fn invalidate_property_narrowings_after_assignment(&mut self, stmt: &Stmt, env: &mut TypeEnv) {
        match &stmt.kind {
            StmtKind::PropertyAssign { .. } | StmtKind::StaticPropertyAssign { .. } => {
                Self::purge_property_narrowings(env);
                self.record_property_assignment_narrowing(stmt, env);
            }
            StmtKind::PropertyArrayPush { .. } | StmtKind::PropertyArrayAssign { .. } => {
                Self::purge_property_narrowings(env)
            }
            StmtKind::StaticPropertyArrayPush { .. }
            | StmtKind::StaticPropertyArrayAssign { .. } => Self::purge_property_narrowings(env),
            StmtKind::NestedArrayAssign { target, .. }
                if assignment_target_may_write_property(target) =>
            {
                Self::purge_property_narrowings(env)
            }
            StmtKind::Assign { name, .. }
            | StmtKind::TypedAssign { name, .. }
            | StmtKind::StaticVar { name, .. } => {
                Self::purge_property_narrowings_for_root(env, name)
            }
            StmtKind::RefAssign { target, .. } => {
                Self::purge_property_narrowings_for_root(env, target)
            }
            // The place may be (or reach) a property, and its new cell can be written through.
            StmtKind::RefAssignPlace { .. } => Self::purge_property_narrowings(env),
            StmtKind::ListUnpack { vars, .. } | StmtKind::Global { vars } => {
                for name in vars {
                    Self::purge_property_narrowings_for_root(env, name);
                }
            }
            _ => {}
        }
    }
}

/// Returns whether a nested assignment target reaches an object property rather than only a local
/// array, in which case every aliased property fact must be invalidated.
fn assignment_target_may_write_property(target: &Expr) -> bool {
    match &target.kind {
        ExprKind::Variable(_) => false,
        ExprKind::ArrayAccess { array, .. } => assignment_target_may_write_property(array),
        _ => true,
    }
}

/// Widens the untyped property or static property at the root of an element write chain.
///
/// Called for nested writes (`$o->p[$k1][$k2] = v`) and for element `unset()`s whose receiver is
/// a property (`unset($o->p[$k])`, `unset($o->p[$k1][$k2])`): both mutate the slot through its
/// stored cell, which only the boxed PHP-array representation provides.
pub(crate) fn widen_property_root_for_element_write(
    checker: &mut Checker,
    target: &Expr,
    span: crate::span::Span,
    env: &TypeEnv,
) -> Result<(), CompileError> {
    let mut root = target;
    while let ExprKind::ArrayAccess { array, .. } = &root.kind {
        root = array;
    }
    match &root.kind {
        ExprKind::PropertyAccess { object, property } => {
            properties::widen_property_root_for_nested_write(checker, object, property, env)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_properties::widen_static_property_root_for_nested_write(
                checker, receiver, property, span,
            )
        }
        _ => Ok(()),
    }
}

/// Models write-context autovivification on the local root of a nested `unset()` target
/// (`unset($c[$k1][$k2])`): the removal goes through the same boxed parent cells as a nested write,
/// so the root's element type is widened the same way.
pub(crate) fn widen_local_root_for_nested_unset(
    checker: &mut Checker,
    target: &Expr,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    match &target.kind {
        ExprKind::ArrayAccess { array, .. } if matches!(array.kind, ExprKind::ArrayAccess { .. }) => {
            arrays::widen_nested_write_local_root(checker, target, env)
        }
        _ => Ok(()),
    }
}
