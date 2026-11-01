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

pub(in crate::types::checker) use arrays::expr_is_string_offset_target;

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind, NESTED_APPEND_TEMP_PREFIX};
use crate::types::{normalized_array_key_type, PhpType, TypeEnv};

use super::super::Checker;

impl Checker {
    /// Retypes a null-initialised untyped property that is lent to an untyped by-reference
    /// parameter (`proc_open($cmd, $spec, $this->pipes)` with `protected $pipes = null;`).
    ///
    /// The callee may store any value through the reference, so the property can no longer be
    /// described by its `null` default: it becomes gradual (`mixed`) storage, exactly as an
    /// `$this->pipes = <anything>` assignment elsewhere in the class would make it. Without this a
    /// read after the call (`foreach ($this->pipes ...)`, `array_search($x, $this->pipes)`) was
    /// checked against `null` and refused. Properties that already hold a concrete type, and
    /// declared (typed) properties, are left alone: the callee's write is then a type error PHP
    /// itself would raise.
    pub(crate) fn widen_null_property_bound_to_untyped_ref(
        &mut self,
        class_name: &str,
        property: &str,
    ) {
        let holds_only_null = self.classes.get(class_name).is_some_and(|info| {
            info.visible_property_index(property)
                .is_some_and(|slot| info.properties[slot].1 == PhpType::Void)
        });
        if holds_only_null {
            properties::refine_object_property_type(self, class_name, property, &PhpType::Mixed);
        }
    }

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

    /// Binds every local this scope CREATES with an array element write to the empty indexed
    /// shape, before the body is checked.
    ///
    /// `$keys[] = $k` against a name nothing has assigned yet is how PHP makes the array, and
    /// `crate::append_vivify` decides which names those are. Doing it at scope ENTRY rather than
    /// at the write is what makes a growing array inside a loop work: the loop-storage fixed point
    /// (`stabilize_loop_storage`) only records a contract for names its entry environment already
    /// holds, and without that contract EIR lowering reads the slot at the representation it had
    /// before the body widened it. EIR lowering seeds the same names, from the same scan, in the
    /// function's entry block.
    ///
    /// Names already in `env` — parameters, superglobals, and for a closure the enclosing scope's
    /// locals — are not candidates, so passing the environment's own keys as the bound set is what
    /// keeps this in step with the storage EIR lowering will find.
    pub(crate) fn seed_vivified_array_locals(env: &mut TypeEnv, body: &[Stmt]) {
        let bound: Vec<String> = env.keys().cloned().collect();
        let vivified = crate::append_vivify::vivified_array_locals(
            body,
            bound.iter().map(String::as_str),
        );
        for name in vivified {
            if crate::globals_array::is_alias(&name) {
                continue;
            }
            env.insert(name, PhpType::Array(Box::new(PhpType::Never)));
        }
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
        self.check_assignment_like_stmt_with_property_update(stmt, env, false)
    }

    /// Preserves a compound expression's property-write mode after its value becomes a temp.
    pub(crate) fn check_assignment_like_stmt_with_property_update(
        &mut self,
        stmt: &Stmt,
        env: &mut TypeEnv,
        compound_property_update: bool,
    ) -> Result<(), CompileError> {
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
            StmtKind::StaticPropertyElementRefAssign {
                receiver,
                property,
                index,
                source,
            } => {
                self.infer_type(index, env)?;
                self.infer_type(source, env)?;
                static_properties::check_ref_assign_static_prop_element(
                    self,
                    receiver,
                    property,
                    source,
                    stmt.span,
                )
            }
            StmtKind::DynamicStaticPropertyWrite {
                receiver,
                property,
                index,
                value,
                ..
            } => static_properties::check_dynamic_static_property_write(
                self,
                receiver,
                property,
                index.as_ref(),
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
                compound_property_update,
            ),
            StmtKind::PropertyRefAssign {
                object,
                property,
                source,
            } => properties::check_property_ref_assign(
                self,
                object,
                property,
                source,
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
    /// Direct named-property writes clear that property across every possible object alias;
    /// computed writes and calls remain global barriers. Local rebindings clear only facts
    /// rooted at the rebound local. A plain
    /// `$this->p = <non-null>` / `self::$p = <non-null>` write is the one case where the
    /// post-write type of a place is known, so `record_property_assignment_narrowing` puts that
    /// single fact back — this is what lets `if (self::$p === null) { self::$p = new S(); }`
    /// leave `self::$p` non-null on both paths.
    fn invalidate_property_narrowings_after_assignment(&mut self, stmt: &Stmt, env: &mut TypeEnv) {
        match &stmt.kind {
            StmtKind::PropertyAssign { property, .. }
            | StmtKind::PropertyRefAssign { property, .. }
            | StmtKind::StaticPropertyAssign { property, .. } => {
                Self::purge_property_narrowings_for_property(env, property);
                self.record_property_assignment_narrowing(stmt, env);
            }
            StmtKind::PropertyArrayPush { property, .. }
            | StmtKind::PropertyArrayAssign { property, .. }
            | StmtKind::StaticPropertyArrayPush { property, .. }
            | StmtKind::StaticPropertyArrayAssign { property, .. } => {
                Self::purge_property_narrowings_for_property(env, property);
                self.record_property_array_write_narrowing(stmt, env);
            }
            StmtKind::StaticPropertyElementRefAssign { property, .. } => {
                Self::purge_property_narrowings_for_property(env, property)
            }
            StmtKind::NestedArrayAssign { target, .. } => {
                if let Some(property) = assignment_target_property_name(target) {
                    Self::purge_property_narrowings_for_property(env, property);
                }
            }
            StmtKind::Assign { name, .. }
            | StmtKind::TypedAssign { name, .. }
            | StmtKind::StaticVar { name, .. } => {
                Self::purge_property_narrowings_for_root(env, name)
            }
            StmtKind::RefAssign { target, .. } => {
                Self::purge_property_narrowings_for_root(env, target)
            }
            StmtKind::ListUnpack { vars, .. } | StmtKind::Global { vars } => {
                for name in vars {
                    Self::purge_property_narrowings_for_root(env, name);
                }
            }
            _ => {}
        }
    }
}

/// Returns the named property reached by a nested array-assignment target.
fn assignment_target_property_name(target: &Expr) -> Option<&str> {
    match &target.kind {
        ExprKind::ArrayAccess { array, .. } => assignment_target_property_name(array),
        ExprKind::PropertyAccess { property, .. }
        | ExprKind::StaticPropertyAccess { property, .. } => Some(property),
        _ => None,
    }
}
