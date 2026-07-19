//! Purpose:
//! Infers expression effects forms for the checker.
//! Handles type facts and diagnostics for expression shapes that need more than scalar/operator inference.
//!
//! Called from:
//! - `crate::types::checker::inference::expr`
//!
//! Key details:
//! - Expression inference shares environments with statement checking, so variable and effect updates must stay synchronized.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{BinOp, CallableTarget, Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;
use super::super::syntactic::wider_type_syntactic;

impl Checker {
    /// Infers the type of an expression while tracking assignment effects through the environment.
    ///
    /// Handles expression forms where variable assignments within sub-expressions must be
    /// visible to later parts of the same expression (e.g., `$a = 1, $a + 2` in ternary/loop contexts).
    /// For most expressions, simply delegates to `infer_type`; for control-flow expressions
    /// (ternary, null coalesce, match), clones the environment to isolate branch-specific bindings
    /// from influencing other branches.
    ///
    /// # Arguments
    /// * `expr` - The expression to infer
    /// * `env` - The type environment, mutated in-place for side-effectful sub-expressions
    ///
    /// # Returns
    /// The inferred `PhpType` on success, or a `CompileError` if type checking fails.
    ///
    /// # Key details
    /// - Assignment expressions call `check_assignment_expression` to properly register the binding.
    /// - Binary `&&`/`||` clone the environment before the right branch to prevent assignments
    ///   in the left branch from leaking into the right branch (PHP semantics).
    /// - Ternary, null coalesce, and match clone the environment per branch; the result type is
    ///   the wider of all branch types via `wider_type_syntactic`.
    /// - A branch's ARRAY STORAGE conversions are merged back out of the clone
    ///   (`merge_array_storage_effects`): the branch's other bindings are conditional and must not
    ///   leak, but a conversion rewrites the array in place and the lowering hoists it to the
    ///   statement's entry, so it happens on EVERY path. Dropping it here compiles the callee of
    ///   `h($m, match ($c) { 1 => $m[0] = "s", default => "d" })` for `array<int>` while the caller
    ///   hands it boxed slots.
    /// - `preg_replace_callback` argument at index 1 is skipped (special handling for capture groups).
    pub(crate) fn infer_type_with_assignment_effects(
        &mut self,
        expr: &Expr,
        env: &mut TypeEnv,
    ) -> Result<PhpType, CompileError> {
        match &expr.kind {
            ExprKind::Variable(name) if self.eval_barrier_active && !env.contains_key(name) => {
                env.insert(name.clone(), PhpType::Mixed);
                Ok(PhpType::Mixed)
            }
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                ..
            } => {
                let ty = self.check_assignment_expression(
                    target,
                    value,
                    result_target.as_deref(),
                    prelude,
                    expr.span,
                    env,
                )?;
                // A write through to a property (directly or via one of its elements)
                // invalidates every property narrowing.
                match &target.kind {
                    ExprKind::Variable(name) => {
                        Self::purge_property_narrowings_for_root(env, name)
                    }
                    _ if assignment_may_write_property(target) => {
                        Self::purge_property_narrowings(env)
                    }
                    _ => {}
                }
                Ok(ty)
            }
            ExprKind::PreIncrement(name) | ExprKind::PreDecrement(name) => {
                let old_ty = env.get(name).cloned();
                let result_ty = self.infer_type(expr, env)?;
                if matches!(old_ty, Some(PhpType::Int)) {
                    env.insert(name.clone(), PhpType::Mixed);
                }
                Ok(result_ty)
            }
            ExprKind::PostIncrement(name) | ExprKind::PostDecrement(name) => {
                let old_ty = env.get(name).cloned();
                let result_ty = self.infer_type(expr, env)?;
                if matches!(old_ty, Some(PhpType::Int)) {
                    env.insert(name.clone(), PhpType::Mixed);
                }
                Ok(result_ty)
            }
            ExprKind::BinaryOp { left, op, right } => {
                self.infer_type_with_assignment_effects(left, env)?;
                if matches!(op, BinOp::And | BinOp::Or) {
                    let mut right_env = env.clone();
                    self.infer_type_with_assignment_effects(right, &mut right_env)?;
                    merge_array_storage_effects(env, &right_env);
                    Ok(PhpType::Bool)
                } else {
                    self.infer_type_with_assignment_effects(right, env)?;
                    self.infer_type(expr, env)
                }
            }
            ExprKind::NullCoalesce { value, default } => {
                let value_ty = self.infer_type_with_assignment_effects(value, env)?;
                let default_ty = if value_ty == PhpType::Void {
                    self.infer_type_with_assignment_effects(default, env)?
                } else {
                    let mut default_env = env.clone();
                    let default_ty =
                        self.infer_type_with_assignment_effects(default, &mut default_env)?;
                    merge_array_storage_effects(env, &default_env);
                    default_ty
                };
                if Self::union_contains_void(&value_ty) {
                    Ok(wider_type_syntactic(
                        &self.strip_void_from_union(&value_ty),
                        &default_ty,
                    ))
                } else {
                    Ok(wider_type_syntactic(&value_ty, &default_ty))
                }
            }
            ExprKind::ShortTernary { value, default } => {
                let value_ty = self.infer_type_with_assignment_effects(value, env)?;
                let default_ty = if value_ty == PhpType::Void {
                    self.infer_type_with_assignment_effects(default, env)?
                } else {
                    let mut default_env = env.clone();
                    let default_ty =
                        self.infer_type_with_assignment_effects(default, &mut default_env)?;
                    merge_array_storage_effects(env, &default_env);
                    default_ty
                };
                Ok(wider_type_syntactic(&value_ty, &default_ty))
            }
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                self.infer_type_with_assignment_effects(condition, env)?;
                // Flow-narrowing across the branches (see guard_narrowing): `$x instanceof X`
                // and simple `$x->prop instanceof X` guards narrow the branch envs. A ternary is a
                // single expression, so branch narrowing is write-invalidation-safe.
                let guard = self.guard_narrowing(condition, env)?;
                let mut then_env = env.clone();
                let mut else_env = env.clone();
                if let Some(guard) = guard {
                    then_env.insert(guard.var.clone(), guard.then_ty);
                    else_env.insert(guard.var, guard.else_ty);
                }
                let then_ty = self.infer_type_with_assignment_effects(then_expr, &mut then_env)?;
                // Representation changes are hoisted to the enclosing statement entry,
                // so both the outer environment and the sibling arm must observe them.
                merge_array_storage_effects(env, &then_env);
                merge_array_storage_effects(&mut else_env, &then_env);
                let else_ty = self.infer_type_with_assignment_effects(else_expr, &mut else_env)?;
                merge_array_storage_effects(env, &else_env);
                Ok(wider_type_syntactic(&then_ty, &else_ty))
            }
            ExprKind::ArrayLiteral(elems) => {
                for elem in elems {
                    self.infer_type_with_assignment_effects(elem, env)?;
                }
                self.infer_type(expr, env)
            }
            ExprKind::ArrayLiteralAssoc(pairs) => {
                for (key, value) in pairs {
                    self.infer_type_with_assignment_effects(key, env)?;
                    self.infer_type_with_assignment_effects(value, env)?;
                }
                self.infer_type(expr, env)
            }
            ExprKind::Match {
                subject,
                arms,
                default,
            } => {
                self.infer_type_with_assignment_effects(subject, env)?;
                let mut result_ty = None;
                for (conditions, result) in arms {
                    let mut arm_env = env.clone();
                    for condition in conditions {
                        self.infer_type_with_assignment_effects(condition, &mut arm_env)?;
                    }
                    let arm_ty = self.infer_type_with_assignment_effects(result, &mut arm_env)?;
                    merge_array_storage_effects(env, &arm_env);
                    result_ty = Some(match result_ty {
                        Some(current) => wider_type_syntactic(&current, &arm_ty),
                        None => arm_ty,
                    });
                }
                if let Some(default) = default {
                    let mut default_env = env.clone();
                    let default_ty =
                        self.infer_type_with_assignment_effects(default, &mut default_env)?;
                    merge_array_storage_effects(env, &default_env);
                    result_ty = Some(match result_ty {
                        Some(current) => wider_type_syntactic(&current, &default_ty),
                        None => default_ty,
                    });
                }
                Ok(result_ty.unwrap_or(PhpType::Void))
            }
            ExprKind::ArrayAccess { array, index } => {
                self.infer_type_with_assignment_effects(array, env)?;
                self.infer_type_with_assignment_effects(index, env)?;
                self.infer_type(expr, env)
            }
            ExprKind::Negate(inner)
            | ExprKind::Not(inner)
            | ExprKind::BitNot(inner)
            | ExprKind::Throw(inner)
            | ExprKind::ErrorSuppress(inner)
            | ExprKind::Print(inner)
            | ExprKind::Spread(inner) => {
                self.infer_type_with_assignment_effects(inner, env)?;
                self.infer_type(expr, env)
            }
            ExprKind::Cast { expr: inner, .. } | ExprKind::PtrCast { expr: inner, .. } => {
                self.infer_type_with_assignment_effects(inner, env)?;
                self.infer_type(expr, env)
            }
            ExprKind::FunctionCall { name, args } => {
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                let builtin_name = name.trim_start_matches('\\');
                // `isset`/`unset` are lazy language constructs: an operand may be
                // an undeclared property routed to `__isset`/`__unset`, which must
                // not be inferred as a bare property access here. The call's own
                // inference handles the operands (with magic routing).
                if matches!(
                    php_symbol_key(builtin_name).as_str(),
                    "isset" | "unset"
                ) {
                    for arg in &expanded_args {
                        self.infer_non_reading_arg_assignment_effects(arg, env)?;
                    }
                } else if !builtin_name.eq_ignore_ascii_case("unset") {
                    for (idx, arg) in expanded_args.iter().enumerate() {
                        if builtin_name.eq_ignore_ascii_case("preg_replace_callback") && idx == 1 {
                            continue;
                        }
                        if builtin_name.eq_ignore_ascii_case("preg_match") && idx == 2 {
                            continue;
                        }
                        // The user-sort comparator is type-checked by `check_builtin`
                        // with its parameters typed from the array element (so an
                        // unannotated object comparator type-checks). Skip the eager
                        // pass here, which would otherwise check the comparator body
                        // with default `Int` parameters and reject object access.
                        if idx == 1
                            && (builtin_name.eq_ignore_ascii_case("usort")
                                || builtin_name.eq_ignore_ascii_case("uasort")
                                || builtin_name.eq_ignore_ascii_case("uksort"))
                        {
                            continue;
                        }
                        self.infer_type_with_assignment_effects(arg, env)?;
                    }
                }
                let ty = self.infer_type(expr, env)?;
                // The callee may mutate any reachable object; drop property narrowings. (The
                // call's own argument checking above still saw them.)
                Self::purge_property_narrowings(env);
                if builtin_name.eq_ignore_ascii_case("preg_match") {
                    if let Some(arg) = expanded_args.get(2) {
                        if let Some(name) = preg_match_output_var(arg) {
                            env.insert(name.clone(), PhpType::Array(Box::new(PhpType::Str)));
                        }
                    }
                }
                if builtin_name.eq_ignore_ascii_case("unset") {
                    for arg in &expanded_args {
                        promote_indexed_local_for_element_unset(arg, env);
                    }
                }
                if builtin_name.eq_ignore_ascii_case("eval") {
                    self.mark_eval_barrier(env);
                }
                Ok(ty)
            }
            ExprKind::NewObject { args, .. } | ExprKind::StaticMethodCall { args, .. } => {
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                for arg in &expanded_args {
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                let ty = self.infer_type(expr, env)?;
                Self::purge_property_narrowings(env);
                Ok(ty)
            }
            ExprKind::ClosureCall { var, args } => {
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                let skip_contextual_callback =
                    self.variable_targets_preg_replace_callback(var.as_str());
                for (idx, arg) in expanded_args.iter().enumerate() {
                    if skip_contextual_callback && idx == 1 {
                        continue;
                    }
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                let ty = self.infer_type(expr, env)?;
                Self::purge_property_narrowings(env);
                Ok(ty)
            }
            ExprKind::ExprCall { callee, args } => {
                self.infer_type_with_assignment_effects(callee, env)?;
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                let skip_contextual_callback = self
                    .expr_targets_preg_replace_callback(callee);
                for (idx, arg) in expanded_args.iter().enumerate() {
                    if skip_contextual_callback && idx == 1 {
                        continue;
                    }
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                let ty = self.infer_type(expr, env)?;
                Self::purge_property_narrowings(env);
                Ok(ty)
            }
            ExprKind::NamedArg { value, .. } => {
                self.infer_type_with_assignment_effects(value, env)?;
                self.infer_type(expr, env)
            }
            ExprKind::PropertyAccess { object, .. }
            | ExprKind::NullsafePropertyAccess { object, .. } => {
                self.infer_type_with_assignment_effects(object, env)?;
                self.infer_type(expr, env)
            }
            ExprKind::DynamicPropertyAccess { object, property }
            | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
                self.infer_type_with_assignment_effects(object, env)?;
                self.infer_type_with_assignment_effects(property, env)?;
                self.infer_type(expr, env)
            }
            ExprKind::MethodCall {
                object,
                method,
                args,
            }
            | ExprKind::NullsafeMethodCall {
                object,
                method,
                args,
            } => {
                self.infer_type_with_assignment_effects(object, env)?;
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                self.promote_pdo_binding_ref_storage(object, method, &expanded_args, env)?;
                for arg in &expanded_args {
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                let ty = self.infer_type(expr, env)?;
                Self::purge_property_narrowings(env);
                Ok(ty)
            }
            ExprKind::NullsafeDynamicMethodCall {
                object,
                method,
                args,
            } => {
                self.infer_type_with_assignment_effects(object, env)?;
                self.infer_type_with_assignment_effects(method, env)?;
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                for arg in &expanded_args {
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                self.infer_type(expr, env)
            }
            ExprKind::BufferNew { len, .. } => {
                self.infer_type_with_assignment_effects(len, env)?;
                self.infer_type(expr, env)
            }
            ExprKind::NewScopedObject { args, .. } => {
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                for arg in &expanded_args {
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                let ty = self.infer_type(expr, env)?;
                Self::purge_property_narrowings(env);
                Ok(ty)
            }
            _ => self.infer_type(expr, env),
        }
    }

    /// Infers effects for a language-construct operand without treating properties as reads.
    fn infer_non_reading_arg_assignment_effects(
        &mut self,
        arg: &Expr,
        env: &mut TypeEnv,
    ) -> Result<(), CompileError> {
        match &arg.kind {
            ExprKind::PropertyAccess { object, .. }
            | ExprKind::NullsafePropertyAccess { object, .. } => {
                self.infer_type_with_assignment_effects(object, env)?;
                Ok(())
            }
            ExprKind::DynamicPropertyAccess { object, property }
            | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
                self.infer_type_with_assignment_effects(object, env)?;
                self.infer_type_with_assignment_effects(property, env)?;
                Ok(())
            }
            ExprKind::ArrayAccess { array, index } => {
                self.infer_type_with_assignment_effects(array, env)?;
                self.infer_type_with_assignment_effects(index, env)?;
                Ok(())
            }
            ExprKind::NamedArg { value, .. } => {
                self.infer_non_reading_arg_assignment_effects(value, env)
            }
            _ => {
                self.infer_type_with_assignment_effects(arg, env)?;
                Ok(())
            }
        }
    }

    /// Returns true when an expression call target is first-class `preg_replace_callback`.
    fn expr_targets_preg_replace_callback(&self, callee: &Expr) -> bool {
        match &callee.kind {
            ExprKind::FirstClassCallable(target) => callable_target_is_preg_replace_callback(target),
            ExprKind::Variable(var_name) => {
                self.variable_targets_preg_replace_callback(var_name.as_str())
            }
            _ => false,
        }
    }

    /// Returns true when a variable stores first-class `preg_replace_callback`.
    fn variable_targets_preg_replace_callback(&self, var_name: &str) -> bool {
        self.first_class_callable_targets
            .get(var_name)
            .is_some_and(callable_target_is_preg_replace_callback)
    }

    /// Widens PDOStatement binding destinations before validation so their escaping
    /// references use the boxed storage that the EIR call lowering materializes.
    fn promote_pdo_binding_ref_storage(
        &mut self,
        object: &Expr,
        method: &str,
        args: &[Expr],
        env: &mut TypeEnv,
    ) -> Result<(), CompileError> {
        let object_ty = self.infer_type(object, env)?;
        if !type_may_be_pdo_statement(&object_ty) {
            return Ok(());
        }
        let method_key = crate::names::php_symbol_key(method);
        let parameter_name = match method_key.as_str() {
            "bindparam" => "variable",
            "bindcolumn" => "var",
            _ => return Ok(()),
        };
        let argument = args.iter().enumerate().find_map(|(index, arg)| match &arg.kind {
            ExprKind::NamedArg { name, value } if name == parameter_name => Some(value.as_ref()),
            ExprKind::NamedArg { .. } => None,
            _ if index == 1 => Some(arg),
            _ => None,
        });
        let Some(Expr {
            kind: ExprKind::Variable(name),
            ..
        }) = argument
        else {
            return Ok(());
        };
        env.insert(name.clone(), PhpType::Mixed);
        Ok(())
    }

    /// Marks the active statement stream as having crossed eval and widens local facts.
    fn mark_eval_barrier(&mut self, env: &mut TypeEnv) {
        self.eval_barrier_active = true;
        let local_names = env.keys().cloned().collect::<Vec<_>>();
        for ty in env.values_mut() {
            *ty = PhpType::Mixed;
        }
        for name in local_names {
            self.closure_return_types.remove(&name);
            self.callable_sigs.remove(&name);
            self.callable_captures.remove(&name);
            self.callable_array_targets.remove(&name);
            self.first_class_callable_targets.remove(&name);
        }
    }
}

/// Returns whether a receiver type contains PDOStatement, including the
/// `PDOStatement|false` contract returned by `PDO::prepare()` and `query()`.
fn type_may_be_pdo_statement(ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(class) => class.trim_start_matches('\\') == "PDOStatement",
        PhpType::Union(members) => members.iter().any(type_may_be_pdo_statement),
        _ => false,
    }
}

/// Merges the array storage-representation conversions performed inside a conditionally-evaluated
/// branch back into the environment the branch was cloned from.
///
/// Every OTHER binding a branch makes is conditional and correctly dropped with the clone. A storage
/// conversion is not: `Op::ArrayToMixed` and `Op::ArrayToHash` rewrite the array the local already
/// points at, and the lowering hoists both to the enclosing STATEMENT's entry — precisely so no op
/// inside can execute against a representation another op replaced — so by the time the statement
/// finishes, the conversion has run on every path through it, taken branch or not.
///
/// If the checker drops the fact, it keeps typing the local as a raw indexed array and specializes
/// any callee it is passed to for raw scalar slots, while the lowering hands that callee boxed cell
/// pointers. Only the conversions the lowering can actually perform are merged
/// (`array_storage_conversion`), so the two views cannot drift apart in either direction.
///
/// A name bound only inside the branch (not present in `env`) stays branch-local: it is not a
/// conversion of anything the outer scope can see.
fn merge_array_storage_effects(env: &mut TypeEnv, branch_env: &TypeEnv) {
    let converted = branch_env
        .iter()
        .filter_map(|(name, branch_ty)| {
            let converted = crate::types::array_storage_conversion(env.get(name), branch_ty)?;
            Some((name.clone(), converted))
        })
        .collect::<Vec<_>>();
    for (name, converted) in converted {
        env.insert(name, converted);
    }
}

/// Returns true when a first-class callable target is PHP `preg_replace_callback`.
fn callable_target_is_preg_replace_callback(target: &CallableTarget) -> bool {
    matches!(
        target,
        CallableTarget::Function(name) if php_symbol_key(name.as_str()) == "preg_replace_callback"
    )
}

/// Returns true when an assignment target can write through to an object property — directly
/// (`$obj->p = …`) or via an element of one (`$obj->p[0] = …`) — invalidating property
/// narrowings. Plain variables (and elements of plain variables) cannot.
fn assignment_may_write_property(target: &Expr) -> bool {
    match &target.kind {
        ExprKind::Variable(_) => false,
        ExprKind::ArrayAccess { array, .. } => assignment_may_write_property(array),
        _ => true,
    }
}

/// Returns the variable name used as `preg_match()`'s output `$matches` argument.
fn preg_match_output_var(arg: &Expr) -> Option<&String> {
    match &arg.kind {
        ExprKind::Variable(name) => Some(name),
        ExprKind::NamedArg { value, .. } => preg_match_output_var(value),
        _ => None,
    }
}

/// Promotes a packed indexed-array local to an associative array when one of its elements is
/// removed via `unset($arr[$key])`.
///
/// PHP's `unset()` removes a key without renumbering the remaining elements, so the array can no
/// longer be a contiguous packed list (e.g. `unset([1,2,3][1])` leaves keys `0` and `2`). Re-typing
/// the local as `AssocArray<Int, T>` makes its literal build as a hash, so the element removal
/// lowers through `HashUnset`. Only plain `$var[$key]` targets on a currently-packed array are
/// affected; associative arrays, objects, and non-variable receivers are left unchanged.
fn promote_indexed_local_for_element_unset(arg: &Expr, env: &mut TypeEnv) {
    let ExprKind::ArrayAccess { array, index, .. } = &arg.kind else {
        return;
    };
    let ExprKind::Variable(name) = &array.kind else {
        return;
    };
    let Some(PhpType::Array(elem_ty)) = env.get(name).cloned() else {
        return;
    };
    let idx_ty = crate::types::array_keys::normalized_array_key_type(
        index,
        super::super::syntactic::infer_expr_type_syntactic(index),
    );
    let key_ty = if idx_ty == PhpType::Int {
        PhpType::Int
    } else {
        PhpType::Mixed
    };
    let value_ty = if *elem_ty == PhpType::Never {
        PhpType::Mixed
    } else {
        *elem_ty
    };
    env.insert(
        name.clone(),
        PhpType::AssocArray {
            key: Box::new(key_ty),
            value: Box::new(value_ty),
        },
    );
}
