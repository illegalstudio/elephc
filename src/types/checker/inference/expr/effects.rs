//! Purpose:
//! Infers expression effects forms for the checker.
//! Handles type facts and diagnostics for expression shapes that need more than scalar/operator inference.
//!
//! Called from:
//! - `crate::types::checker::inference::expr`
//!
//! Key details:
//! - Expression inference shares environments with statement checking, so variable and effect updates must stay synchronized.
//! - Call surfaces that own runtime iteration visit spread-source effects without imposing the
//!   ordinary direct-call array-only constraint before their authoritative checker runs.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{BinOp, CallableTarget, Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::super::super::null_probe;
use super::super::super::Checker;
use super::buffer_merges::buffer_merge_result_type;
use super::merge_match_arm_result_type;
use super::null_coalesce::{
    null_coalesce_buffer_merge_type, null_coalesce_result_type, operand_is_never_null,
};

/// Returns what a local holds after `++`/`--`, or `None` when the update cannot retype it.
///
/// An `int` becomes `int|float`, because PHP promotes at the boundary — `PHP_INT_MAX++` is
/// `float(9.223372036854776E+18)` in php-src and in elephc — and NOT `mixed`, which is what this
/// used to say. The difference is not cosmetic: `mixed` is the top type, so every value derived
/// from a counter became `mixed` too, and `for ($i = 0; …) { $out[] = $i; }` could not even be
/// DESCRIBED as a list of numbers. `int|float` keeps the same boxed storage (`codegen_repr` maps
/// both to `Mixed`) while saying something true, which is what a diagnostic and a downstream
/// inference can both use.
///
/// A `string` stays `mixed`: `"9"++` is `int(10)`, `"a"++` is `"b"` and `""++` is `"1"`, so the
/// result genuinely spans three types and none of them is knowable here.
fn incremented_local_type(current: Option<&PhpType>) -> Option<PhpType> {
    match current {
        Some(PhpType::Int) => Some(PhpType::Union(vec![PhpType::Int, PhpType::Float])),
        Some(PhpType::Str) => Some(PhpType::Mixed),
        _ => None,
    }
}

impl Checker {
    /// Installs assignment effects that are guaranteed when a short-circuit condition is true.
    ///
    /// The ordinary expression pass keeps the right side of `&&` isolated because it may never
    /// execute. Inside the true branch, however, every right side in the successful `&&` chain
    /// has executed. This pass exposes only those branch-local effects and returns the previous
    /// bindings so the statement checker can restore the fallthrough environment afterwards.
    pub(crate) fn install_truthy_short_circuit_effects(
        &mut self,
        condition: &Expr,
        env: &mut TypeEnv,
    ) -> Result<Vec<(String, Option<PhpType>)>, CompileError> {
        self.install_short_circuit_outcome_effects(condition, true, env)
    }

    /// Installs effects and narrowing facts guaranteed when a short-circuit condition is false.
    ///
    /// Every operand of a false `||` chain has executed and evaluated false. This is used after a
    /// terminal `if` body, where reaching the following statement proves the condition was false.
    pub(crate) fn install_falsy_short_circuit_effects(
        &mut self,
        condition: &Expr,
        env: &mut TypeEnv,
    ) -> Result<Vec<(String, Option<PhpType>)>, CompileError> {
        self.install_short_circuit_outcome_effects(condition, false, env)
    }

    /// Installs effects and branch facts guaranteed by one short-circuit outcome.
    fn install_short_circuit_outcome_effects(
        &mut self,
        condition: &Expr,
        truthy: bool,
        env: &mut TypeEnv,
    ) -> Result<Vec<(String, Option<PhpType>)>, CompileError> {
        let before = env.clone();
        self.infer_short_circuit_outcome_effects(condition, truthy, false, env)?;
        let changed = env
            .iter()
            .filter_map(|(name, ty)| {
                let previous = before.get(name);
                (previous != Some(ty)).then(|| (name.clone(), previous.cloned()))
            })
            .collect();
        Ok(changed)
    }

    /// Infers operands and narrowing facts that must hold for one short-circuit outcome.
    fn infer_short_circuit_outcome_effects(
        &mut self,
        condition: &Expr,
        truthy: bool,
        nested: bool,
        env: &mut TypeEnv,
    ) -> Result<(), CompileError> {
        match &condition.kind {
            ExprKind::BinaryOp {
                left,
                op: BinOp::And,
                right,
            } if truthy => {
                self.infer_short_circuit_outcome_effects(left, true, true, env)?;
                let receiver_fact = zero_arg_method_receiver(right)
                    .and_then(|receiver| self.flow_guard_env_key(receiver))
                    .filter(|key| key.starts_with('\u{1}'))
                    .and_then(|key| env.get(&key).cloned().map(|ty| (key, ty)));
                self.infer_type_with_assignment_effects(right, env)?;
                // Calling a method through the guarded property does not change the class of the
                // receiver value that satisfied the preceding `instanceof`. Calls ordinarily
                // invalidate every property fact conservatively; retain this one narrow fact so
                // the next operand in the same successful `&&` chain sees the proven receiver.
                // Restrict this to zero-argument calls: argument evaluation can contain an
                // explicit write or by-reference mutation of the guarded storage.
                if let Some((key, ty)) = receiver_fact {
                    env.insert(key, ty);
                }
                self.infer_short_circuit_outcome_effects(right, true, true, env)
            }
            ExprKind::BinaryOp {
                left,
                op: BinOp::Or,
                right,
            } if !truthy => {
                self.infer_short_circuit_outcome_effects(left, false, true, env)?;
                self.infer_type_with_assignment_effects(right, env)?;
                self.infer_short_circuit_outcome_effects(right, false, true, env)
            }
            ExprKind::FunctionCall { name, args }
                if truthy && php_symbol_key(name.trim_start_matches('\\')) == "isset" =>
            {
                self.install_truthy_isset_root_facts(args, env);
                Ok(())
            }
            _ => {
                if nested {
                    let Some(narrowing) = self.guard_narrowing(condition, env)? else {
                        return Ok(());
                    };
                    env.insert(
                        narrowing.var,
                        if truthy {
                            narrowing.then_ty
                        } else {
                            narrowing.else_ty
                        },
                    );
                }
                Ok(())
            }
        }
    }

    /// Binds roots that must exist when a successful `isset()` condition is true.
    ///
    /// A chained probe such as `isset($value['key'])` may be the first syntactic read of a local
    /// that was created along only one earlier short-circuit path. The true branch proves that
    /// the root is initialized, while its exact runtime shape remains gradual, so `Mixed` is the
    /// sound branch-local storage fact. The caller restores the previous binding after the branch.
    fn install_truthy_isset_root_facts(&self, args: &[Expr], env: &mut TypeEnv) {
        let roots = args
            .iter()
            .filter_map(|arg| {
                null_probe::undefined_probe_root_variable(arg, env).map(str::to_string)
            })
            .collect::<Vec<_>>();
        for root in roots {
            env.insert(root, PhpType::Mixed);
        }
    }

    /// Ends the branch-local `isset()` root facts a truthy condition installed.
    ///
    /// `install_truthy_isset_root_facts` binds a never-declared probe root to `Mixed` for the
    /// guarded branch only. Leaving it there let the branch exit join (`if`, `?:`, `&&`) publish
    /// the name as a `Mixed` local AFTER the construct, which is not PHP: `if (isset($x)) {}` does
    /// not define `$x`. At top level the published binding also defeated the deferred null-probe
    /// check, so `unset($a); echo isset($a) ? 1 : 0;` was refused with `Undefined variable: $a`.
    /// A root the branch itself assigned no longer holds the installed `Mixed` and is kept.
    pub(crate) fn drop_truthy_isset_root_facts(
        condition: &Expr,
        before: &TypeEnv,
        branch_env: &mut TypeEnv,
    ) {
        for root in Self::truthy_isset_fact_roots(condition, before) {
            if branch_env.get(&root) == Some(&PhpType::Mixed) {
                branch_env.remove(&root);
            }
        }
    }

    /// The never-declared `isset()` roots a truthy `condition` binds for its branch only.
    pub(crate) fn truthy_isset_fact_roots(condition: &Expr, before: &TypeEnv) -> Vec<String> {
        let mut roots = Vec::new();
        collect_truthy_isset_roots(condition, before, &mut roots);
        roots
    }

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
    /// - Ternary, null coalesce, and match clone the environment per branch so assignments
    ///   do not leak across arms; match/ternary result types then reuse `infer_type`'s
    ///   Mixed-aware branch merge (not the Str-absorbing syntactic join).
    /// - `preg_replace_callback` argument at index 1 is skipped (special handling for capture groups).
    pub(crate) fn infer_type_with_assignment_effects(
        &mut self,
        expr: &Expr,
        env: &mut TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let result = match &expr.kind {
            ExprKind::Variable(name) if self.eval_barrier_active && !env.contains_key(name) => {
                env.insert(name.clone(), PhpType::Mixed);
                Ok(PhpType::Mixed)
            }
            // `yield from self::passthruStream($response = $r->response, ...)` (Symfony's
            // `AsyncResponse::stream()`) defines `$response` for the statements after it: the
            // operand's assignments take effect first, then the yield is typed as usual.
            ExprKind::YieldFrom(inner) => {
                self.infer_type_with_assignment_effects(inner, env)?;
                self.infer_type(expr, env)
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
            // `yield (($tmp = f()) && $tmp instanceof M ? ... : $tmp)` -- what every compiled Twig
            // template writes for a filtered value -- assigns inside the yielded operand and reads
            // the result in the same expression. The plain inference of the operand saw the read
            // before any assignment ("Undefined variable: $tmp"); the operand's effects belong to
            // the enclosing statement like any other expression's.
            ExprKind::Yield { key, value } => {
                if let Some(key) = key {
                    self.infer_type_with_assignment_effects(key, env)?;
                }
                if let Some(value) = value {
                    self.infer_type_with_assignment_effects(value, env)?;
                }
                Ok(PhpType::Mixed)
            }
            ExprKind::PreIncrement(name) | ExprKind::PreDecrement(name) => {
                let old_ty = env.get(name).cloned();
                let result_ty = self.infer_type(expr, env)?;
                if let Some(updated) = incremented_local_type(old_ty.as_ref()) {
                    env.insert(name.clone(), updated);
                }
                Ok(result_ty)
            }
            ExprKind::PostIncrement(name) | ExprKind::PostDecrement(name) => {
                let old_ty = env.get(name).cloned();
                let result_ty = self.infer_type(expr, env)?;
                // Same retype as the pre-form: only the RESULT differs, and it was already
                // computed above against the type the local held before the update.
                if let Some(updated) = incremented_local_type(old_ty.as_ref()) {
                    env.insert(name.clone(), updated);
                }
                Ok(result_ty)
            }
            ExprKind::BinaryOp { left, op, right } => {
                self.infer_type_with_assignment_effects(left, env)?;
                if matches!(op, BinOp::And | BinOp::Or) {
                    let mut right_env = env.clone();
                    if *op == BinOp::And {
                        self.install_truthy_short_circuit_effects(left, &mut right_env)?;
                    } else {
                        self.install_falsy_short_circuit_effects(left, &mut right_env)?;
                    }
                    if let Some(narrowing) = self.guard_narrowing(left, &right_env)? {
                        right_env.insert(
                            narrowing.var,
                            if *op == BinOp::And {
                                narrowing.then_ty
                            } else {
                                narrowing.else_ty
                            },
                        );
                    }
                    self.local_conditional_depth += 1;
                    let right_result = self.infer_type_with_assignment_effects(right, &mut right_env);
                    self.local_conditional_depth -= 1;
                    right_result?;
                    if *op == BinOp::And {
                        Self::drop_truthy_isset_root_facts(left, env, &mut right_env);
                    }
                    merge_conditional_storage_effects(env, &right_env);
                    Ok(PhpType::Bool)
                } else {
                    self.infer_type_with_assignment_effects(right, env)?;
                    self.infer_type(expr, env)
                }
            }
            ExprKind::NullCoalesce { value, default } => {
                // `$neverDefined ?? $d` is a null probe: PHP answers `$d` without an
                // undefined-variable warning, so the left chain root reads as `null`.
                let probe = null_probe::begin_null_probe_root(self, value, env);
                let value_ty = self.infer_null_probe_operand_with_effects(value, env);
                null_probe::end_null_probe_root(probe, env);
                let value_ty = value_ty?;
                let value_is_never_null = operand_is_never_null(self, value, &value_ty, env);
                let default_ty = if value_ty == PhpType::Void {
                    self.infer_type_with_assignment_effects(default, env)?
                } else {
                    let mut default_env = env.clone();
                    self.local_conditional_depth += 1;
                    let default_ty =
                        self.infer_type_with_assignment_effects(default, &mut default_env);
                    self.local_conditional_depth -= 1;
                    let default_ty = default_ty?;
                    merge_conditional_storage_effects(env, &default_env);
                    default_ty
                };
                match null_coalesce_buffer_merge_type(
                    self,
                    value,
                    &value_ty,
                    value_is_never_null,
                    default,
                    &default_ty,
                    expr.span,
                )? {
                    Some(buffer) => Ok(buffer),
                    None => Ok(null_coalesce_result_type(
                        self,
                        value_is_never_null,
                        value_ty,
                        default_ty,
                    )),
                }
            }
            ExprKind::ShortTernary { value, default } => {
                let value_ty = self.infer_type_with_assignment_effects(value, env)?;
                if value_ty == PhpType::Void {
                    self.infer_type_with_assignment_effects(default, env)?;
                } else {
                    let mut default_env = env.clone();
                    self.local_conditional_depth += 1;
                    let default_result =
                        self.infer_type_with_assignment_effects(default, &mut default_env);
                    self.local_conditional_depth -= 1;
                    default_result?;
                    merge_conditional_storage_effects(env, &default_env);
                }
                // Result type comes from the Mixed-aware short-ternary merge in `infer_type`.
                self.infer_type(expr, env)
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
                self.install_truthy_short_circuit_effects(condition, &mut then_env)?;
                self.install_falsy_short_circuit_effects(condition, &mut else_env)?;
                // A side the guard rules out cannot run; see the dead-branch note in
                // `infer_type`'s ternary arm.
                let (then_dead, else_dead) =
                    guard.as_ref().map_or((false, false), |guard| (guard.then_dead, guard.else_dead));
                if let Some(guard) = guard {
                    let then_ty = if guard.then_dead { PhpType::Mixed } else { guard.then_ty };
                    let else_ty = if guard.else_dead { PhpType::Mixed } else { guard.else_ty };
                    then_env.insert(guard.var.clone(), then_ty);
                    else_env.insert(guard.var, else_ty);
                }
                // Neither branch is certain to run, so a store in one is no straight-line retype
                // (`Checker::local_binding_is_killable`), exactly as for an `if` statement.
                self.local_conditional_depth += 1;
                let then_ty = self.infer_type_with_assignment_effects(then_expr, &mut then_env);
                self.local_conditional_depth -= 1;
                let then_ty = then_ty?;
                let then_ty = if matches!(then_expr.kind, ExprKind::Throw(_)) {
                    PhpType::Never
                } else {
                    then_ty
                };
                Self::drop_truthy_isset_root_facts(condition, env, &mut then_env);
                merge_conditional_storage_effects(env, &then_env);
                merge_conditional_storage_effects(&mut else_env, &then_env);
                self.local_conditional_depth += 1;
                let else_ty = self.infer_type_with_assignment_effects(else_expr, &mut else_env);
                self.local_conditional_depth -= 1;
                let else_ty = else_ty?;
                let else_ty = if matches!(else_expr.kind, ExprKind::Throw(_)) {
                    PhpType::Never
                } else {
                    else_ty
                };
                merge_conditional_storage_effects(env, &else_env);
                match (then_dead, else_dead) {
                    (false, true) => Ok(then_ty),
                    (true, false) => Ok(else_ty),
                    _ => {
                        // A `throw` arm types as `never` here, so the buffer merge skips it by its
                        // shape, as `infer_type` does through `match_arm_result_type`.
                        let arms = [(&**then_expr, then_ty), (&**else_expr, else_ty)];
                        match buffer_merge_result_type("a ternary", &arms, expr.span)? {
                            Some(buffer) => Ok(buffer),
                            None => {
                                let [(_, then_ty), (_, else_ty)] = arms;
                                Ok(merge_match_arm_result_type(self, then_ty, else_ty))
                            }
                        }
                    }
                }
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
            ExprKind::ArrayLiteralMixed(entries) => {
                for entry in entries {
                    for inner in entry.exprs() {
                        self.infer_type_with_assignment_effects(inner, env)?;
                    }
                }
                self.infer_type(expr, env)
            }
            ExprKind::Match {
                subject,
                arms,
                default,
            } => {
                self.infer_type_with_assignment_effects(subject, env)?;
                let mut fallthrough_env = env.clone();
                let mut result_ty: Option<PhpType> = None;
                for (conditions, result) in arms {
                    let mut arm_env = fallthrough_env.clone();
                    for condition in conditions {
                        if let Err(error) =
                            self.infer_type_with_assignment_effects(condition, &mut arm_env)
                        {
                            return Err(self.with_other_match_condition_errors(error, arms, &arm_env));
                        }
                    }
                    fallthrough_env = arm_env.clone();
                    let mut result_env =
                        self.match_arm_narrowed_env(subject, conditions, &arm_env)?;
                    let ty = self.infer_type_with_assignment_effects(result, &mut result_env)?;
                    let ty = if matches!(result.kind, ExprKind::Throw(_)) {
                        PhpType::Never
                    } else {
                        ty
                    };
                    result_ty = Some(match result_ty {
                        Some(acc) => merge_match_arm_result_type(self, acc, ty),
                        None => ty,
                    });
                    merge_conditional_storage_effects(env, &result_env);
                }
                if let Some(default) = default {
                    let mut default_env = fallthrough_env;
                    let ty = self.infer_type_with_assignment_effects(default, &mut default_env)?;
                    let ty = if matches!(default.kind, ExprKind::Throw(_)) {
                        PhpType::Never
                    } else {
                        ty
                    };
                    result_ty = Some(match result_ty {
                        Some(acc) => merge_match_arm_result_type(self, acc, ty),
                        None => ty,
                    });
                    merge_conditional_storage_effects(env, &default_env);
                }
                Ok(result_ty.unwrap_or(PhpType::Void))
            }
            ExprKind::ArrayAccess { array, index } => {
                self.infer_type_with_assignment_effects(array, env)?;
                self.infer_type_with_assignment_effects(index, env)?;
                self.infer_type(expr, env)
            }
            ExprKind::Not(inner) => {
                self.infer_type_with_assignment_effects(inner, env)?;
                Ok(PhpType::Bool)
            }
            ExprKind::Negate(inner)
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
            ExprKind::InstanceOf { value, target } => {
                self.infer_type_with_assignment_effects(value, env)?;
                if let crate::parser::ast::InstanceOfTarget::Expr(target) = target {
                    self.infer_type_with_assignment_effects(target, env)?;
                }
                self.infer_type(expr, env)
            }
            ExprKind::Closure { capture_refs, .. } => {
                // Capturing an undefined local by reference creates the caller-visible null cell
                // before the closure value is constructed. Use gradual storage because the
                // closure can replace that cell with any PHP value when it executes.
                for name in capture_refs {
                    // A local that only ever held null (`$found = null; function () use
                    // (&$found) { $found = $v; }`) is exactly as open: typed `Void`, a later
                    // `new ReflectionFunction($found)` was refused as taking null.
                    let entry = env.entry(name.clone()).or_insert(PhpType::Mixed);
                    if matches!(entry, PhpType::Void) {
                        *entry = PhpType::Mixed;
                    }
                }
                self.infer_type(expr, env)
            }
            ExprKind::FunctionCall { name, args } => {
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                let builtin_name = name.trim_start_matches('\\');
                if let Some((signature, internal_callable)) =
                    self.direct_function_call_signature(builtin_name)
                {
                    // A builtin has no user body to widen anything, so only a resolved user
                    // function names a scope here.
                    let callee_scope = (!internal_callable).then(|| {
                        self.canonical_function_name_folded(builtin_name)
                            .unwrap_or_else(|| builtin_name.to_string())
                    });
                    if internal_callable {
                        self.widen_hash_splice_receiver(builtin_name, args, env);
                        self.box_iterable_by_ref_builtin_args(&signature, args, env);
                        // `exec()` is lowered to a prelude helper with ordinary by-reference
                        // parameters, so a property passed as `$output` / `$result_code` must be
                        // promoted to a reference cell exactly as for a user callee.
                        if builtin_name.eq_ignore_ascii_case("exec") {
                            self.promote_by_ref_property_args(&signature, args, env);
                        }
                    }
                    self.prepare_by_ref_variable_storage(
                        &signature,
                        args,
                        expr.span,
                        env,
                        internal_callable,
                        callee_scope.as_deref(),
                    )?;
                } else if self.direct_call_is_late_bound_undefined(builtin_name) {
                    self.prepare_late_bound_call_argument_storage(args, env);
                }
                // `isset`/`unset` are lazy language constructs: an operand may be
                // an undeclared property routed to `__isset`/`__unset`, which must
                // not be inferred as a bare property access here. The call's own
                // inference handles the operands (with magic routing).
                // `empty()` too: its operand is a silent probe like `isset()`'s (an inaccessible
                // property answers through `__isset`/`__get` or `true`), and the eager read below
                // refused `empty($o->private)` from outside the class at compile time.
                if matches!(
                    php_symbol_key(builtin_name).as_str(),
                    "isset" | "unset" | "empty"
                ) {
                    // `isset($never)` / `unset($never)` are exactly the constructs PHP provides
                    // for probing storage that may never have been declared, so a never-declared
                    // chain root reads as `null` for the operand.
                    // The operand is a silent probe for this walk too: `isset(self::$p)` on a
                    // class without `$p` is php's `false`, not an undefined-property refusal.
                    let probing = php_symbol_key(builtin_name) != "unset";
                    for arg in &expanded_args {
                        let probe = null_probe::begin_null_probe_root(self, arg, env);
                        if probing {
                            self.null_probe_depth += 1;
                        }
                        let effects = self.infer_non_reading_arg_assignment_effects(arg, env);
                        if probing {
                            self.null_probe_depth -= 1;
                        }
                        null_probe::end_null_probe_root(probe, env);
                        effects?;
                    }
                } else if !crate::types::checker::is_unset_call(builtin_name) {
                    // `empty()` shares that tolerance, but its operand is still read (PHP
                    // consults `__isset` then `__get`), so it stays on the eager path with only
                    // the probe binding added.
                    let is_empty = php_symbol_key(builtin_name) == "empty";
                    // An array-callback builtin types its callback's unannotated parameters
                    // from the array element/key inside `check_builtin`. Skip that argument
                    // here: the eager pass would otherwise check the closure body against the
                    // unhinted parameter fallback and reject valid PHP such as
                    // `array_filter($strings, fn($v) => strlen($v) > 5)`.
                    let contextual_callbacks =
                        crate::types::checker::builtins::contextual_callback_arg_positions(
                            builtin_name,
                        );
                    // The table is in parameter positions; a named argument fills the
                    // parameter it names, wherever it sits in the call, so
                    // `xml_set_character_data_handler(handler: fn(...) => ..., parser: $p)`
                    // skips the closure at source index 0 too.
                    let contextual_sig = if contextual_callbacks.is_empty() {
                        None
                    } else {
                        crate::types::signatures::builtin_call_sig(builtin_name)
                    };
                    for (idx, arg) in expanded_args.iter().enumerate() {
                        let param_idx = match &arg.kind {
                            ExprKind::NamedArg { name, .. } => contextual_sig
                                .as_ref()
                                .and_then(|sig| {
                                    sig.params.iter().position(|(param, _)| {
                                        param.trim_start_matches('$') == name.trim_start_matches('$')
                                    })
                                })
                                .unwrap_or(idx),
                            _ => idx,
                        };
                        if contextual_callbacks.contains(&param_idx) {
                            continue;
                        }
                        if crate::builtins::mbstring::is_capture_output_argument(builtin_name, arg, idx) {
                            // Forming the output reference requires dynamic storage before later
                            // source arguments can pass that same variable to a mutating call.
                            if let Some(name) = output_variable(arg) {
                                env.insert(name.clone(), PhpType::Mixed);
                            }
                            continue;
                        }
                        if preg_output_type(builtin_name, arg, idx).is_some()
                            || pcntl_output_type(builtin_name, arg, idx).is_some()
                            || xml_struct_output_type(builtin_name, arg, idx).is_some()
                            || (builtin_name.eq_ignore_ascii_case("openssl_encrypt")
                                && is_openssl_encrypt_tag_arg(arg, idx))
                        {
                            continue;
                        }
                        if is_empty {
                            let probe = null_probe::begin_null_probe_root(self, arg, env);
                            let effects = self.infer_null_probe_operand_with_effects(arg, env);
                            null_probe::end_null_probe_root(probe, env);
                            effects?;
                            continue;
                        }
                        let runtime_iterated_spread = idx > 0
                            && matches!(arg.kind, ExprKind::Spread(_))
                            && matches!(
                                php_symbol_key(builtin_name).as_str(),
                                "array_push" | "call_user_func"
                            );
                        if runtime_iterated_spread {
                            self.infer_spread_source_assignment_effects(arg, env)?;
                        } else {
                            self.infer_call_argument_with_assignment_effects(arg, env)?;
                        }
                    }
                }
                let ty = self.infer_type(expr, env)?;
                // The callee may mutate any reachable object; drop property narrowings. (The
                // call's own argument checking above still saw them.) A `readonly` property's
                // narrowing survives: no call can reassign it.
                // A registry builtin with an empty semantic effect summary cannot mutate that
                // storage; retaining facts for it keeps compound guards precise without a PHP-name
                // allowlist.
                if !registry_builtin_call_preserves_property_narrowings(builtin_name) {
                    self.purge_property_narrowings_after_call(env);
                }
                for (idx, arg) in expanded_args.iter().enumerate() {
                    if preg_output_type(builtin_name, arg, idx).is_some() {
                        if let Some(name) = output_variable(arg) {
                            // ONE authority for the `$matches` shape, shared with EIR lowering:
                            // `preg_match_all` fills its outer container with BOXED cells, so the
                            // element type is Mixed whatever the pattern looks like, and a literal
                            // pattern with a named capture group makes the destination the ordered
                            // hash the runtime really builds (a list type would read it back
                            // renumbered). `preg_match_all` builds that hash only in PATTERN ORDER;
                            // under `PREG_SET_ORDER` only the rows are hashes.
                            let matches_ty = crate::types::checker::regex_matches_destination_type(
                                builtin_name,
                                &expanded_args,
                            );
                            env.insert(name.clone(), matches_ty);
                        }
                    }
                    if crate::builtins::mbstring::is_capture_output_argument(builtin_name, arg, idx) {
                        if let Some(name) = output_variable(arg) {
                            env.insert(name.clone(), PhpType::Mixed);
                        }
                    }
                    if let Some(output_ty) = pcntl_output_type(builtin_name, arg, idx) {
                        if let Some(name) = output_variable(arg) {
                            env.insert(name.clone(), output_ty);
                        }
                    }
                    if let Some(output_ty) = xml_struct_output_type(builtin_name, arg, idx) {
                        if let Some(name) = output_variable(arg) {
                            env.insert(name.clone(), output_ty);
                        }
                    }
                }
                if builtin_name.eq_ignore_ascii_case("openssl_encrypt") {
                    if let Some(arg) = openssl_encrypt_tag_arg(&expanded_args) {
                        if let Some(name) = output_variable(arg) {
                            env.insert(name.clone(), PhpType::Str);
                        }
                    }
                }
                if crate::types::checker::is_unset_call(builtin_name) {
                    for arg in &expanded_args {
                        promote_indexed_local_for_element_unset(arg, env);
                    }
                    // `unset($a)` on an eligible local KILLS the binding, which is what PHP
                    // does: a later read is "Undefined variable" and a later assignment binds
                    // fresh, at whatever type it likes. Ineligible names (conditional depth,
                    // reference-aliased, `global`, `static`, declared-typed) keep today's
                    // typing no-op. Iterating the SOURCE args keeps the recorded span on a node
                    // EIR lowering actually visits.
                    //
                    // ONLY from statement position. `unset` is a statement in PHP's grammar, but
                    // elephc's parser also accepts it as an expression, so this arm is reachable
                    // from a `?:`/`??`/`&&` operand — a position whose `TypeEnv` clone is
                    // DISCARDED when the branch loses. Everything the kill does below lives on
                    // the checker instead (the recorded kill site, the per-name metadata clear,
                    // the dropped binding depth), so it would escape that discard: measured as a
                    // kill recorded for a never-executed ternary arm (the program then printed
                    // nothing where the local was still live) and as a callable-argument
                    // diagnostic lost with the metadata. Outside statement position this whole
                    // block is a no-op — including the re-decision below, which must not disarm
                    // a kill a statement-position visit legitimately recorded.
                    if self.expr_is_in_statement_position(expr) {
                        for arg in args {
                            let ExprKind::Variable(var) = &arg.kind else {
                                continue;
                            };
                            // `unset($var)` ends this binding even when escaped-reference
                            // conservatism keeps the name in `ref_aliased_locals`. A later `=&`
                            // assignment must establish fresh cell provenance for the new binding.
                            self.invalidate_boxed_ref_alias_binding(var);
                            // Detaching a promoted reference is separate from killing its type
                            // binding: escaped aliases still prohibit incompatible reassignments.
                            if arg.span.identifies_a_node() {
                                if env.contains_key(var) && self.local_reference_is_detachable(var) {
                                    self.local_ref_detach_sites.entry(arg.span).or_default()
                                        .insert(var.clone());
                                    self.active_ref_params.remove(var);
                                    self.active_external_ref_bindings.remove(var);
                                    self.ref_aliased_locals.remove(var);
                                    self.non_builtin_ref_aliased_locals.remove(var);
                                    self.ref_assign_aliased_locals.remove(var);
                                } else if let Some(names) = self.local_ref_detach_sites.get_mut(&arg.span) {
                                    if names.remove(var.as_str()) {
                                        self.retired_ref_detach_sites.insert((arg.span, var.clone()));
                                    }
                                    if names.is_empty() {
                                        self.local_ref_detach_sites.remove(&arg.span);
                                    }
                                }
                            }
                            // A top-level name some other body declares `global` is NOT killable
                            // however eligible it otherwise looks: `global $a;` in a function binds
                            // the very cell this slot holds, so dropping the name here leaves the
                            // rest of the program reaching storage the environment no longer knows.
                            // Lowering refuses to abandon the same slots, from the same collected
                            // set — see `Checker::top_level_binding_is_program_global`.
                            if env.contains_key(var)
                                && self.local_binding_is_killable(var)
                                && !self.top_level_binding_is_program_global(var)
                            {
                                env.remove(var);
                                self.local_binding_depth.remove(var);
                                self.clear_local_binding_metadata(var);
                                // A span that names no node must never enter a map lowering keys
                                // BY span. `Span::dummy()` is shared by every compiler-generated
                                // node (`Span::identifies_a_node`), so one prelude `unset` filed
                                // under it would make lowering abandon the binding at EVERY other
                                // dummy-span `unset` argument in the program. The env kill above
                                // still happens — only the lowering instruction is withheld, which
                                // leaves such a site on the plain null-store path it had before
                                // kills were lowered at all.
                                //
                                // Inserted into the span's SET of killed names rather than over
                                // it: two DIFFERENT locals killed at one (line, column) in two
                                // files are two decisions, and one used to evict the other.
                                if arg.span.identifies_a_node() {
                                    self.local_bind_kill_sites
                                        .entry(arg.span)
                                        .or_default()
                                        .insert(var.clone());
                                }
                            } else {
                                // This visit RE-DECIDES the site. The checker walks a body more
                                // than once (top level twice, method bodies to stability, a
                                // function body once per call-site re-specialization), and only
                                // the LAST walk's decisions may reach EIR lowering — the same
                                // rule `loop_storage_types` follows by clearing its scope before
                                // each re-walk. A kill recorded by a superseded walk whose
                                // successor refuses (a callee's by-reference parameter that only
                                // became visible later, say) would otherwise make lowering
                                // abandon a slot the final check kept alive.
                                //
                                // Qualified by NAME for the same reason the insert above is: a
                                // `Span` has no file identity, so an `unset($other)` at the same
                                // (line, column) of another file must not disarm this one. Exactly
                                // ONE name leaves the span's set, and the key itself goes only
                                // when the set is empty.
                                if let Some(killed) = self.local_bind_kill_sites.get_mut(&arg.span)
                                {
                                    killed.remove(var.as_str());
                                    if killed.is_empty() {
                                        self.local_bind_kill_sites.remove(&arg.span);
                                    }
                                }
                            }
                        }
                    }
                }
                if builtin_name.eq_ignore_ascii_case("eval") {
                    self.mark_eval_barrier(env);
                }
                Ok(ty)
            }
            ExprKind::NewObject { class_name, args } => {
                let constructor = self
                    .classes
                    .get(class_name.as_str().trim_start_matches('\\'))
                    .and_then(|info| info.methods.get("__construct"))
                    .cloned();
                if let Some(constructor) = constructor {
                    if constructor.ref_params.iter().any(|by_ref| *by_ref) {
                        let callee_scope = self.method_body_scope(
                            class_name.as_str().trim_start_matches('\\'),
                            "__construct",
                        );
                        self.promote_by_ref_property_args(&constructor, args, env);
                        self.record_property_bound_promotion_sites(
                            &constructor,
                            args,
                            callee_scope.as_deref(),
                        );
                    }
                }
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                for arg in &expanded_args {
                    self.infer_call_argument_with_assignment_effects(arg, env)?;
                }
                let ty = self.infer_type(expr, env)?;
                Self::purge_property_narrowings(env);
                Ok(ty)
            }
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args,
            } => {
                let target = CallableTarget::StaticMethod {
                    receiver: receiver.clone(),
                    method: method.clone(),
                };
                let signature = self
                    .resolve_first_class_callable_sig(&target, expr.span, env)
                    .ok()
                    .or_else(|| self.static_call_effect_signature(receiver, method));
                if let Some(signature) = signature {
                    let callee_scope = self
                        .static_receiver_class_name(receiver)
                        .and_then(|class| self.method_body_scope(&class, method));
                    self.prepare_by_ref_variable_storage(
                        &signature,
                        args,
                        expr.span,
                        env,
                        false,
                        callee_scope.as_deref(),
                    )?;
                }
                let closure_bind = matches!(
                    receiver,
                    crate::parser::ast::StaticReceiver::Named(name)
                        if name.as_str().trim_start_matches('\\') == "Closure"
                ) && php_symbol_key(method) == "bind";
                if !closure_bind {
                    let expanded_args =
                        crate::types::call_args::expand_static_assoc_spread_args(args);
                    for arg in &expanded_args {
                        self.infer_call_argument_with_assignment_effects(arg, env)?;
                    }
                }
                // `Closure::bind` owns inference of its closure operand so it can install the
                // literal visibility scope before checking the body. Eager inference here would
                // check that body in its lexical scope first and reject valid private access.
                let ty = self.infer_type(expr, env)?;
                self.purge_property_narrowings_after_call(env);
                Ok(ty)
            }
            ExprKind::ClosureCall { var, args } => {
                if let Some(signature) = self.callable_sigs.get(var).cloned() {
                    self.prepare_by_ref_variable_storage(
                        &signature,
                        args,
                        expr.span,
                        env,
                        false,
                        None,
                    )?;
                }
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                let skip_contextual_callback =
                    self.variable_targets_preg_replace_callback(var.as_str());
                let descriptor_args = self.callable_param_names.contains(var)
                    || self.callable_array_targets.contains_key(var);
                for (idx, arg) in expanded_args.iter().enumerate() {
                    if skip_contextual_callback && idx == 1 {
                        continue;
                    }
                    if descriptor_args && matches!(arg.kind, ExprKind::Spread(_)) {
                        self.infer_spread_source_assignment_effects(arg, env)?;
                    } else {
                        self.infer_call_argument_with_assignment_effects(arg, env)?;
                    }
                }
                let ty = self.infer_type(expr, env)?;
                self.purge_property_narrowings_after_call(env);
                Ok(ty)
            }
            ExprKind::ExprCall { callee, args } => {
                self.infer_type_with_assignment_effects(callee, env)?;
                if let Some(signature) = self.resolve_expr_callable_sig(callee, env)? {
                    self.prepare_by_ref_variable_storage(
                        &signature,
                        args,
                        expr.span,
                        env,
                        false,
                        None,
                    )?;
                }
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                let skip_contextual_callback = self
                    .expr_targets_preg_replace_callback(callee);
                for (idx, arg) in expanded_args.iter().enumerate() {
                    if skip_contextual_callback && idx == 1 {
                        continue;
                    }
                    if matches!(arg.kind, ExprKind::Spread(_)) {
                        self.infer_spread_source_assignment_effects(arg, env)?;
                    } else {
                        self.infer_call_argument_with_assignment_effects(arg, env)?;
                    }
                }
                let ty = self.infer_type(expr, env)?;
                self.purge_property_narrowings_after_call(env);
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
                let object_type = self.infer_type_with_assignment_effects(object, env)?;
                let target = CallableTarget::Method {
                    object: object.clone(),
                    method: method.clone(),
                };
                // The class schema is the authority here. Only when it cannot answer at all does
                // the callable resolver get a say, and its gradual arm returns a placeholder that
                // describes a DESCRIPTOR invocation rather than the callee, so the `ref_params` it
                // carries are not evidence about this call.
                let schema_signature = self.instance_call_effect_signature(&object_type, method);
                // A receiver of a class the build does not declare (`new \Spoofchecker()` without
                // ext/intl) has no schema either: php raises at run time, and the argument shape
                // is just as unknown as a gradual receiver's.
                let receiver_class_outside_build = match object_type.codegen_repr() {
                    PhpType::Object(class_name) => {
                        let class_name = class_name.trim_start_matches('\\');
                        !class_name.is_empty()
                            && !self.classes.contains_key(class_name)
                            && !self.interfaces.contains_key(class_name)
                    }
                    _ => false,
                };
                let by_ref_shape_is_unknown = schema_signature.is_none()
                    && (matches!(object_type.codegen_repr(), PhpType::Mixed)
                        || receiver_class_outside_build);
                let signature = schema_signature
                    .or_else(|| self.resolve_first_class_callable_sig(&target, expr.span, env).ok());
                if let Some(signature) = signature {
                    // Same rule as `instance_call_effect_signature`, and asked of the declared
                    // type for the same reason: `codegen_repr()` turns a union into `Mixed`.
                    let callee_scope = match &object_type {
                        PhpType::Union(_) => self
                            .union_single_object_class(&object_type)
                            .and_then(|class_name| self.method_body_scope(&class_name, method)),
                        other => match other.codegen_repr() {
                            PhpType::Object(class_name) => {
                                self.method_body_scope(&class_name, method)
                            }
                            _ => None,
                        },
                    };
                    self.prepare_by_ref_variable_storage(
                        &signature,
                        args,
                        expr.span,
                        env,
                        false,
                        callee_scope.as_deref(),
                    )?;
                }
                if by_ref_shape_is_unknown {
                    self.define_unknown_callee_by_ref_arguments(args, env);
                }
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                for arg in &expanded_args {
                    self.infer_call_argument_with_assignment_effects(arg, env)?;
                }
                let ty = self.infer_type(expr, env)?;
                self.purge_property_narrowings_after_call(env);
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
                    self.infer_call_argument_with_assignment_effects(arg, env)?;
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
                    self.infer_call_argument_with_assignment_effects(arg, env)?;
                }
                let ty = self.infer_type(expr, env)?;
                self.purge_property_narrowings_after_call(env);
                Ok(ty)
            }
            _ => self.infer_type(expr, env),
        };
        if result.is_ok() {
            self.apply_boxed_reference_outputs(expr.span, env);
        }
        result
    }

    /// Visits a call-unpack source for assignment effects without choosing its container policy.
    ///
    /// The enclosing call checker decides whether this spread is an array-only direct call or a
    /// descriptor invocation that may walk Traversable. This preliminary effects walk must still
    /// observe assignments inside the source, but must not reject a source before that decision.
    fn infer_spread_source_assignment_effects(
        &mut self,
        arg: &Expr,
        env: &mut TypeEnv,
    ) -> Result<(), CompileError> {
        let ExprKind::Spread(source) = &arg.kind else {
            self.infer_type_with_assignment_effects(arg, env)?;
            return Ok(());
        };
        self.infer_type_with_assignment_effects(source, env)?;
        Ok(())
    }

    /// Infers one call argument while preserving assignment effects in a gradual spread source.
    fn infer_call_argument_with_assignment_effects(
        &mut self,
        arg: &Expr,
        env: &mut TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let ExprKind::Spread(inner) = &arg.kind else {
            return self.infer_type_with_assignment_effects(arg, env);
        };
        match self.infer_type_with_assignment_effects(inner, env)? {
            PhpType::Array(elem_ty) => Ok(*elem_ty),
            PhpType::AssocArray { value, .. } => Ok(*value),
            PhpType::Iterable | PhpType::Mixed | PhpType::Union(_) => Ok(PhpType::Mixed),
            // `callable` is a PREDICATE, not a storage shape -- see the sibling arms in
            // `infer_call_argument_type` and `calls_objects`.
            PhpType::Callable => Ok(PhpType::Mixed),
            PhpType::Object(name) if self.object_type_implements_iterable(&name) => {
                Ok(PhpType::Mixed)
            }
            _ => Err(CompileError::new(
                arg.span,
                "Spread operator requires an array",
            )),
        }
    }

    /// Returns the statically known signature for a direct named call and whether it uses
    /// internal-function named-argument rules.
    pub(crate) fn direct_function_call_signature(
        &self,
        name: &str,
    ) -> Option<(crate::types::FunctionSig, bool)> {
        let key = php_symbol_key(name);
        if !crate::types::checker::builtins::strict_php_hidden_builtin(&key) {
            if let Some(signature) = crate::types::builtin_call_sig(&key) {
                return Some((signature, true));
            }
        }
        let canonical = self
            .canonical_function_name_folded(name)
            .unwrap_or_else(|| name.to_string());
        self.functions
            .get(&canonical)
            .cloned()
            .or_else(|| self.unresolved_direct_function_effect_signature(&canonical))
            .map(|signature| (signature, false))
    }

    /// Builds the caller-visible reference contract for a declared function not yet resolved.
    ///
    /// Call-effect inference runs before ordinary call inference. A forward call can therefore
    /// encounter only its parsed declaration, but its by-reference arguments still need their
    /// post-call storage before later statements are checked. This lightweight signature keeps
    /// declared parameter types authoritative without resolving or checking the function body.
    fn unresolved_direct_function_effect_signature(
        &self,
        canonical: &str,
    ) -> Option<crate::types::FunctionSig> {
        let declaration = self.fn_decls.get(canonical)?;
        let params = declaration
            .params
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let ty = declaration
                    .param_types
                    .get(index)
                    .and_then(|type_expr| type_expr.as_ref())
                    .and_then(|type_expr| {
                        self.resolve_declared_param_type_hint(
                            type_expr,
                            declaration.span,
                            "Function parameter",
                        )
                        .ok()
                    })
                    .unwrap_or(PhpType::Int);
                (name.clone(), ty)
            })
            .chain(declaration.variadic.iter().map(|name| {
                let element = declaration
                    .variadic_type
                    .as_ref()
                    .and_then(|type_expr| {
                        self.resolve_declared_param_type_hint(
                            type_expr,
                            declaration.span,
                            "Variadic function parameter",
                        )
                        .ok()
                    })
                    .unwrap_or(if declaration.variadic_by_ref {
                        PhpType::Mixed
                    } else {
                        PhpType::Int
                    });
                (name.clone(), PhpType::Array(Box::new(element)))
            }))
            .collect();
        Some(crate::types::FunctionSig {
            params,
            param_type_exprs: declaration
                .param_types
                .iter()
                .cloned()
                .chain(
                    declaration
                        .variadic
                        .iter()
                        .map(|_| declaration.variadic_type.clone()),
                )
                .collect(),
            param_attributes: declaration.param_attributes.clone(),
            defaults: declaration.defaults.clone(),
            return_type: PhpType::Int,
            declared_return: declaration.return_type.is_some(),
            by_ref_return: declaration.by_ref_return,
            ref_params: declaration.ref_params.clone(),
            declared_params: declaration
                .param_types
                .iter()
                .map(|type_expr| type_expr.is_some())
                .chain(
                    declaration
                        .variadic
                        .iter()
                        .map(|_| declaration.variadic_type.is_some()),
                )
                .collect(),
            variadic: declaration.variadic.clone(),
            deprecation: None,
            is_generator: false,
        })
    }

    /// Returns whether a direct call has no compile-time declaration and will use runtime lookup.
    fn direct_call_is_late_bound_undefined(&self, name: &str) -> bool {
        let canonical = self
            .canonical_function_name_folded(name)
            .unwrap_or_else(|| name.to_string());
        !self.functions.contains_key(&canonical)
            && !self.fn_decls.contains_key(&canonical)
            && !self.function_variant_groups.contains_key(&canonical)
            && self.canonical_extern_function_name_folded(name).is_none()
            && crate::types::checker::builtins::is_late_bound_undefined_function(&canonical)
    }

    /// Provides gradual recovery storage for undefined variables passed to an unresolved call.
    ///
    /// PHP resolves a direct function before evaluating its arguments, so the compiler's current
    /// late-bound path throws before any such variable is read. Binding the names as `Mixed` keeps
    /// later unreachable statements typeable without pretending the unknown runtime signature is
    /// by-value or by-reference. Calls with any known signature stay on the precise path above.
    fn prepare_late_bound_call_argument_storage(&self, args: &[Expr], env: &mut TypeEnv) {
        let names = args
            .iter()
            .filter_map(by_ref_output_variable)
            .filter(|name| !env.contains_key(name.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        for name in names {
            env.insert(name, PhpType::Mixed);
        }
    }

    /// Resolves an instance method signature for call effects without callable-syntax policy.
    fn instance_call_effect_signature(
        &self,
        object_type: &PhpType,
        method: &str,
    ) -> Option<crate::types::FunctionSig> {
        // Asked of the DECLARED type, not `codegen_repr()`: that collapses every union to `Mixed`,
        // so a union arm placed after it can never match. `X|false` still dispatches on the one
        // object it names — `prepare()` answers `PDOStatement|false` and `$stmt->bindParam(1, $id)`
        // defines `$id` in PHP. Refusing to resolve the signature left the variable undefined.
        let class_name = match object_type {
            PhpType::Union(_) => match self.union_single_object_class(object_type) {
                Some(class_name) => class_name,
                // A union naming no single class is as gradual as `Mixed` is.
                None => return self.mixed_receiver_by_ref_signature(method),
            },
            other => match other.codegen_repr() {
                PhpType::Object(class_name) => class_name,
                PhpType::Mixed => return self.mixed_receiver_by_ref_signature(method),
                _ => return None,
            },
        };
        let key = php_symbol_key(method);
        self.classes
            .get(class_name.trim_start_matches('\\'))?
            .methods
            .get(&key)
            .cloned()
    }

    /// Binds bare-variable arguments of a call whose by-reference shape cannot be known.
    ///
    /// PHP decides this at run time: a by-reference parameter creates the variable silently, and a
    /// by-value one warns and passes null. Both run. Reporting `Undefined variable` at compile time
    /// is the one answer PHP never gives, and it was reached from
    /// `dynamic_first_class_callable_sig()` — a descriptor placeholder whose `ref_params: [false]`
    /// describes no callee at all.
    ///
    /// Only reached when the receiver is gradual AND no class in the program declares the method,
    /// so a typo passed to a method a known class declares by value keeps its diagnostic. An
    /// argument the caller already bound keeps its type; only an unbound name is created, as
    /// `mixed`, which is what a by-reference binding would leave there.
    fn define_unknown_callee_by_ref_arguments(&mut self, args: &[Expr], env: &mut TypeEnv) {
        for argument in args {
            let ExprKind::Variable(name) = &argument.kind else {
                continue;
            };
            if env.get(name).is_none() {
                env.insert(name.clone(), PhpType::Mixed);
            }
        }
    }

    /// Resolves the by-reference shape of a method on a GRADUAL receiver, from its candidates.
    ///
    /// A `Mixed` receiver dispatches on the runtime class id over exactly the classes that declare
    /// the method — `mixed_receiver_method_return_type` types the RESULT that way. Which arguments
    /// bind by reference is the same question about the same candidates.
    ///
    /// Symfony's `PdoAdapter::doSave` needs it: `$conn = $this->getConnection()` is declared `\PDO`,
    /// `PDO` is absent from that build's closed world, so `$stmt` degrades to `Mixed` and
    /// `$stmt->bindParam(1, $id)` no longer named a class to ask. Without an answer the lookup fell
    /// through to the callable resolver, which reported a one-parameter non-ref signature and left
    /// `$id` undefined.
    ///
    /// Candidates must AGREE. Disagreement is not a fact about this call — it is methods that share
    /// a name — and with no candidate at all this answers `None`, so a genuine typo passed to a
    /// method nothing declares keeps its diagnostic.
    fn mixed_receiver_by_ref_signature(
        &self,
        method: &str,
    ) -> Option<crate::types::FunctionSig> {
        let key = php_symbol_key(method);
        let mut found: Option<&crate::types::FunctionSig> = None;
        // Only the classes declaring the method can contribute; the owner index names them in
        // class-table order, the order this used to scan every class in per mixed call.
        let owners = self.method_owners_of(&key)?;
        for class_name in owners.iter() {
            let Some(sig) = self.classes.get(class_name).and_then(|info| info.methods.get(&key))
            else {
                continue;
            };
            if !sig.ref_params.iter().any(|by_ref| *by_ref) {
                continue;
            }
            match found {
                Some(previous) if previous.ref_params != sig.ref_params => return None,
                Some(_) => {}
                None => found = Some(sig),
            }
        }
        found.cloned()
    }

    /// Resolves the class a `Foo::m()` / `self::m()` / `parent::m()` receiver names.
    pub(crate) fn static_receiver_class_name(
        &self,
        receiver: &crate::parser::ast::StaticReceiver,
    ) -> Option<String> {
        match receiver {
            // A generic receiver is instantiated into an ordinary named one before type checking;
            // a template has no class to reach through.
            crate::parser::ast::StaticReceiver::Generic(_) => unreachable!(
                "StaticReceiver::Generic must be instantiated by generics::classes"
            ),
            crate::parser::ast::StaticReceiver::Named(name) => Some(name.as_str().to_string()),
            crate::parser::ast::StaticReceiver::Self_
            | crate::parser::ast::StaticReceiver::Static => self.current_class.clone(),
            crate::parser::ast::StaticReceiver::Parent => self
                .classes
                .get(self.current_class.as_ref()?)?
                .parent
                .clone(),
        }
    }

    /// Resolves a static method signature for call effects without callable-syntax policy.
    fn static_call_effect_signature(
        &self,
        receiver: &crate::parser::ast::StaticReceiver,
        method: &str,
    ) -> Option<crate::types::FunctionSig> {
        let class_name = self.static_receiver_class_name(receiver)?;
        let info = self.classes.get(class_name.trim_start_matches('\\'))?;
        let key = php_symbol_key(method);
        info.static_methods
            .get(&key)
            .or_else(|| info.methods.get(&key))
            .cloned()
    }

    /// Boxes an `iterable` local handed to a builtin's by-reference parameter
    /// (`array_unshift($providers, ...)`, `array_push`, `sort`).
    ///
    /// `iterable` storage is a bare heap pointer the mutator would write an array back through as
    /// if it were one, and the array checks refused it outright -- which, in an untyped function's
    /// first pass, left its return type at the `int` placeholder (`return implode(...)` came back
    /// `0`). Boxed, the builtin takes its run-time-checked `Mixed` path: an array is mutated, a
    /// `Traversable` raises php's `TypeError`.
    fn box_iterable_by_ref_builtin_args(
        &mut self,
        signature: &crate::types::FunctionSig,
        args: &[Expr],
        env: &mut TypeEnv,
    ) {
        for (index, arg) in args.iter().enumerate() {
            if !signature.ref_params.get(index).copied().unwrap_or(false) {
                continue;
            }
            let ExprKind::Variable(name) = &arg.kind else {
                continue;
            };
            if env.get(name).map(PhpType::codegen_repr) != Some(PhpType::Iterable) {
                continue;
            }
            self.by_ref_local_storage_types.insert(
                (self.current_loop_storage_scope.clone(), name.clone()),
                PhpType::Mixed,
            );
            env.insert(name.clone(), PhpType::Mixed);
        }
    }

    /// Gives a hash local spliced by `array_splice()` the boxed gradual storage the splice needs.
    ///
    /// The native splice rewrites a dense slot array, so a hash receiver is spliced by the prelude
    /// walk `array_splice_prelude` builds, which binds the receiver as `mixed &$array` and answers
    /// an array of EITHER kind: `['a' => 1]` spliced at offset 0 becomes `[0 => 'x', 'a' => 1]`,
    /// and a hash whose keys are all integers comes back as a list. A slot still typed as the
    /// original hash could not hold that, so the local is boxed from here on -- the same decision
    /// `prepare_by_ref_variable_storage` makes for a `mixed &$x` parameter.
    fn widen_hash_splice_receiver(&mut self, name: &str, args: &[Expr], env: &mut TypeEnv) {
        if php_symbol_key(name) != "array_splice" {
            return;
        }
        let Some(ExprKind::Variable(receiver)) = args.first().map(|arg| &arg.kind) else {
            return;
        };
        if !matches!(env.get(receiver), Some(PhpType::AssocArray { .. })) {
            return;
        }
        self.by_ref_local_storage_types.insert(
            (self.current_loop_storage_scope.clone(), receiver.clone()),
            PhpType::Mixed,
        );
        env.insert(receiver.clone(), PhpType::Mixed);
    }

    /// Prepares caller storage for variables supplied to by-reference parameters.
    ///
    /// PHP treats an undefined variable at an untyped reference boundary as a writable null
    /// cell. The compiler uses boxed `Mixed` storage for that cell so the callee can replace it
    /// with any PHP value. Declared reference parameters instead see a null value and retain
    /// ordinary parameter validation, which rejects null when the declaration does not accept it.
    ///
    /// `callee_scope` names the callee's BODY when the call resolves to one, so a parameter whose
    /// body widens it (`Checker::scan_widened_ref_params`) hands the caller the boxed storage its
    /// cell really has. `None` — a closure, a runtime callable, a builtin — leaves the decision
    /// exactly where it was.
    /// Promotes every declared property a USER callee binds to a by-reference parameter.
    ///
    /// php binds the parameter to the property's own slot: `new Svc($this->p)` with
    /// `__construct(array &$p) { $this->p = &$p; }` leaves both objects sharing one array. The
    /// lowering hands the callee the property's reference cell, which exists only for a promoted
    /// property, exactly as `$x = &$obj->p` promotes it. Without it the argument was a copy:
    /// Symfony's `ServicesConfigurator` kept a reference to a temporary, read back `null`, and
    /// every container-building console command died on "Cannot assign null to property
    /// ServiceConfigurator::$instanceof".
    ///
    /// Builtins keep their copy-in/write-back lowering (their mutators take the array value), so
    /// only user callees reach this. A receiver whose class is not statically one class is left
    /// alone, as `$x = &$obj->p` leaves it.
    fn promote_by_ref_property_args(
        &mut self,
        signature: &crate::types::FunctionSig,
        args: &[Expr],
        env: &mut TypeEnv,
    ) {
        let regular_param_count = crate::types::call_args::regular_param_count(signature);
        for (index, arg) in args.iter().enumerate() {
            let (param_index, place) = match &arg.kind {
                ExprKind::Spread(_) => return,
                ExprKind::NamedArg { name, value } => {
                    match signature.params.iter().position(|(param, _)| param == name) {
                        Some(position) => (position, value.as_ref()),
                        None => continue,
                    }
                }
                _ => (index, arg),
            };
            if param_index >= regular_param_count
                || !signature.ref_params.get(param_index).copied().unwrap_or(false)
            {
                continue;
            }
            // An ELEMENT of the property binds the element's cell, which lives in the property's
            // storage, so the property needs its cell too: `MockArraySessionStorage::loadSession()`
            // passes `$this->data[$key]` to `AttributeBag::initialize(array &$a)`, which keeps it.
            let mut place = place;
            while let ExprKind::ArrayAccess { array, .. } = &place.kind {
                place = array.as_ref();
            }
            let ExprKind::PropertyAccess { object, property } = &place.kind else {
                continue;
            };
            // A closure's `$this` may be rebound to any receiver; see the `return &$this->p` rule.
            if self.closure_depth > 0 && matches!(object.kind, ExprKind::This) {
                continue;
            }
            let Ok(object_ty) = self.infer_type(object, env) else {
                continue;
            };
            let Some(class) = crate::types::checker::single_object_class_name(&object_ty) else {
                continue;
            };
            let declares = self
                .classes
                .get(&class)
                .is_some_and(|info| info.properties.iter().any(|(name, _)| name == property));
            if declares && !self.is_native_throwable_payload_property(&class, property) {
                self.reference_property_promotions
                    .insert((class, property.clone()));
            }
        }
    }

    /// Records each local passed to a by-reference parameter its user callee binds a property to
    /// (`Checker::property_bound_promotion_sites`): lowering gives that local a managed cell the
    /// property can keep (`bind_property_to_param_ref_cell`). An unresolved callee answers for
    /// every user callee with such a parameter of that name; promoting a local only changes
    /// where its value lives, never what it means.
    fn record_property_bound_promotion_sites(
        &mut self,
        signature: &crate::types::FunctionSig,
        args: &[Expr],
        callee_scope: Option<&str>,
    ) {
        let regular_param_count = crate::types::call_args::regular_param_count(signature);
        for (index, arg) in args.iter().enumerate() {
            let (param_index, place) = match &arg.kind {
                ExprKind::Spread(_) => return,
                ExprKind::NamedArg { name, value } => {
                    match signature.params.iter().position(|(param, _)| param == name) {
                        Some(position) => (position, value.as_ref()),
                        None => continue,
                    }
                }
                _ => (index, arg),
            };
            if param_index >= regular_param_count
                || !signature.ref_params.get(param_index).copied().unwrap_or(false)
                || !matches!(place.kind, ExprKind::Variable(_))
                || !place.span.identifies_a_node()
            {
                continue;
            }
            let Some((param, _)) = signature.params.get(param_index) else {
                continue;
            };
            if self.ref_param_is_property_bound(callee_scope, param) {
                self.property_bound_promotion_sites
                    .insert((self.current_loop_storage_scope.clone(), place.span));
            }
        }
    }

    fn prepare_by_ref_variable_storage(
        &mut self,
        signature: &crate::types::FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        env: &mut TypeEnv,
        internal_callable: bool,
        callee_scope: Option<&str>,
    ) -> Result<(), CompileError> {
        // Nothing to prepare without a by-reference parameter, and planning the arguments here
        // can only fail: `call_user_func($callback, b: 3)` forwards `b` to the CALLBACK, which
        // the builtin's own signature does not name, and the call was refused with "call has no
        // parameter $b". The argument checks proper run on the call itself.
        if !signature.ref_params.iter().any(|by_ref| *by_ref) {
            return Ok(());
        }
        if !internal_callable {
            self.promote_by_ref_property_args(signature, args, env);
            self.record_property_bound_promotion_sites(signature, args, callee_scope);
        }
        let normalized = if internal_callable {
            self.plan_builtin_call_args(signature, args, span, "call", env)?
                .normalized_args()
        } else {
            self.normalize_named_call_args(signature, args, span, "call", env)?
        };
        let regular_param_count = crate::types::call_args::regular_param_count(signature);
        let variadic_param_index = signature.variadic.as_ref().and_then(|name| {
            signature
                .params
                .iter()
                .position(|(param_name, _)| param_name == name)
        });
        let mut param_index = 0usize;
        for argument in &normalized {
            if matches!(argument.kind, ExprKind::Spread(_)) {
                continue;
            }
            let is_variadic_binding = param_index >= regular_param_count;
            let Some(binding_index) = (if is_variadic_binding {
                variadic_param_index
            } else {
                Some(param_index)
            }) else {
                break;
            };
            if signature
                .ref_params
                .get(binding_index)
                .copied()
                .unwrap_or(false)
            {
                if let Some(name) = by_ref_output_variable(argument) {
                    let declared = signature
                        .declared_params
                        .get(binding_index)
                        .copied()
                        .unwrap_or(false);
                    let declared_expected = signature
                        .params
                        .get(binding_index)
                        .map(|(_, ty)| ty)
                        .unwrap_or(&PhpType::Mixed);
                    let current = env.get(name).cloned();
                    // The cell's REPRESENTATION, which is the declared type unless the callee's
                    // own body stores something that type cannot hold. The callee's cell IS this
                    // slot, so a widening there has to reach the caller before the slot is typed.
                    let widened = self.effective_ref_param_type(
                        callee_scope,
                        signature
                            .params
                            .get(binding_index)
                            .map(|(param_name, _)| param_name.as_str())
                            .unwrap_or(""),
                        declared_expected,
                    );
                    // …but ONLY for an argument the declaration would accept. Binding a rejected
                    // one to the widened `mixed` here hands `require_bound_param_arg_type` a
                    // gradual `actual` a moment later, and `mixed` satisfies every scalar — so
                    // `function adv(string $s, int &$i) {…}; $z = "str"; adv("a", $z);` compiled
                    // silently where `php -n` throws `Argument #2 ($i) must be of type int, string
                    // given` and exits 255. The declared type is left in place for exactly that
                    // argument, which is the only path on which this feature could turn a program
                    // PHP refuses into one elephc runs
                    // (`error_tests::type_system::test_widened_by_ref_param_still_validates_its_argument`).
                    let expected = &if widened != *declared_expected
                        && current.as_ref().is_some_and(|current| {
                            !Self::types_compatible(declared_expected, current)
                                && !self.type_accepts(declared_expected, current)
                        })
                    {
                        declared_expected.clone()
                    } else {
                        widened
                    };
                    let storage = if is_variadic_binding && *expected == PhpType::Mixed {
                        Some(PhpType::Mixed)
                    } else if internal_callable {
                        if *expected == PhpType::Mixed {
                            current.clone().or(Some(PhpType::Mixed))
                        } else {
                            Some(expected.clone())
                        }
                    } else if declared
                        && matches!(
                            expected,
                            PhpType::Array(_) | PhpType::AssocArray { .. }
                        )
                        && current.as_ref().is_some_and(|current| {
                            matches!(
                                current,
                                PhpType::Array(_) | PhpType::AssocArray { .. }
                            )
                        })
                    {
                        // A declared `array` reference may replace elements, keys, or the whole
                        // array while retaining the same PHP-visible contract. The caller's
                        // pre-call shape is therefore no longer sound after the call; expose the
                        // generic declared shape while preserving the shared array ABI storage.
                        Some(expected.clone())
                    } else if declared
                        && current.as_ref().is_some_and(|current| {
                            current.codegen_repr() != expected.codegen_repr()
                                && (matches!(
                                    current.codegen_repr(),
                                    PhpType::Mixed | PhpType::Union(_)
                                ) || matches!(
                                    expected.codegen_repr(),
                                    PhpType::Mixed | PhpType::Union(_) | PhpType::TaggedScalar
                                ))
                        })
                    {
                        // A declared reference parameter validates and updates the variable in
                        // place. Keep a gradual caller boxed for a checked concrete adapter;
                        // otherwise adopt the parameter's nullable/gradual representation so
                        // every value the callee may write fits the caller's frame slot.
                        let required_storage = if current.as_ref().is_some_and(|current| {
                            matches!(current.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
                        }) {
                            PhpType::Mixed
                        } else {
                            expected.clone()
                        };
                        // A by-reference PARAMETER of this body is not this call site's to
                        // re-store: its cell is the CALLER's slot, whose representation the
                        // declaration (or `scan_widened_ref_params`) fixed for every caller.
                        // `function g(int &$c) { $c = $c + 1; h($c); }` saw `$c` as gradual after
                        // the addition and boxed the parameter's slot, so the body read the
                        // caller's raw int as a Mixed pointer and crashed (Symfony's
                        // `FlattenException::flattenArgs(…, int &$count)` on every error page).
                        if !self.active_ref_params.contains(name) {
                            self.by_ref_local_storage_types.insert(
                                (self.current_loop_storage_scope.clone(), name.clone()),
                                required_storage,
                            );
                        }
                        Some(expected.clone())
                    } else if !declared || *expected == PhpType::Mixed {
                        Some(PhpType::Mixed)
                    } else if let Some(current) = current.clone() {
                        if current != PhpType::Mixed
                            && expected.codegen_repr() == PhpType::Mixed
                            && (Self::types_compatible(expected, &current)
                                || self.type_accepts(expected, &current))
                        {
                            Some(expected.clone())
                        } else {
                            Some(current)
                        }
                    } else if Self::types_compatible(expected, &PhpType::Void)
                        || self.type_accepts(expected, &PhpType::Void)
                    {
                        Some(expected.clone())
                    } else {
                        Some(PhpType::Void)
                    };
                    if let Some(storage) = storage {
                        if (!internal_callable || is_variadic_binding)
                            && !self.active_ref_params.contains(name)
                            && storage.codegen_repr() == PhpType::Mixed
                            && current
                                .as_ref()
                                .is_some_and(|current| current.codegen_repr() != PhpType::Mixed)
                        {
                            self.by_ref_local_storage_types.insert(
                                (self.current_loop_storage_scope.clone(), name.clone()),
                                storage.clone(),
                            );
                        }
                        // Lending a by-reference PARAMETER of the current body on to another
                        // by-reference parameter must not re-bind it below its own cell's
                        // representation. The arms above end by binding the name to the CALLEE's
                        // declared type, which is right for an ordinary local — the adapter
                        // writes a concrete value back into it — and wrong for a name whose cell
                        // is boxed and stays boxed: every read below the call would then be typed
                        // against a representation the slot does not have, and the next store
                        // through the reference is refused for a type the cell can hold perfectly
                        // well. `Symfony\Component\Yaml\Inline::parseMapping` is the shape:
                        // `self::parseScalar(…, $i, false)` re-bound `$i` to `parseScalar`'s
                        // declared `int`, and `$i = \strpos(…)` eleven lines below it was
                        // `cannot reassign $i from int to int|false`. Measured on a hand-written
                        // `mixed &$i` too, which is the same program with the decision spelled
                        // out, so this is not the widening's own defect.
                        let storage = match &current {
                            Some(current)
                                if self.active_ref_params.contains(name)
                                    && current.codegen_repr() == PhpType::Mixed
                                    && storage.codegen_repr() != PhpType::Mixed =>
                            {
                                current.clone()
                            }
                            _ => storage,
                        };
                        env.insert(name.clone(), storage);
                    }
                } else if let ExprKind::PropertyAccess { object, property } = &argument.kind {
                    // A property lent to an untyped (or `mixed`) reference parameter: the callee
                    // may store anything through it.
                    let declared = signature
                        .declared_params
                        .get(binding_index)
                        .copied()
                        .unwrap_or(false);
                    let gradual = !declared
                        || signature
                            .params
                            .get(binding_index)
                            .is_some_and(|(_, ty)| *ty == PhpType::Mixed);
                    if gradual {
                        self.widen_null_property_for_ref_argument(object, property, env);
                    }
                }
            }
            param_index += 1;
        }
        Ok(())
    }

    /// Retypes a null-initialised untyped property passed to a gradual by-reference parameter.
    ///
    /// Resolves the receiver's single class, then defers to
    /// `Checker::widen_null_property_bound_to_untyped_ref`. A receiver whose class is not
    /// statically one class is left alone, as `promote_by_ref_property_args` leaves it.
    fn widen_null_property_for_ref_argument(
        &mut self,
        object: &Expr,
        property: &str,
        env: &mut TypeEnv,
    ) {
        let Ok(object_ty) = self.infer_type(object, env) else {
            return;
        };
        let Some(class) = crate::types::checker::single_object_class_name(&object_ty) else {
            return;
        };
        self.widen_null_property_bound_to_untyped_ref(&class, property);
    }

    /// Infers effects for a language-construct operand without treating properties as reads.
    fn infer_non_reading_arg_assignment_effects(
        &mut self,
        arg: &Expr,
        env: &mut TypeEnv,
    ) -> Result<(), CompileError> {
        // The receiver spine is itself probed: `isset($null->a->b)` and `isset($null['k']->p)`
        // reach through a null link silently in PHP, so an inner read on a `null` base answers
        // `null` here exactly as it does for the operand's outermost access.
        match &arg.kind {
            ExprKind::PropertyAccess { object, .. }
            | ExprKind::NullsafePropertyAccess { object, .. } => {
                self.infer_null_probe_operand_with_effects(object, env)?;
                Ok(())
            }
            ExprKind::DynamicPropertyAccess { object, property }
            | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
                self.infer_null_probe_operand_with_effects(object, env)?;
                self.infer_type_with_assignment_effects(property, env)?;
                Ok(())
            }
            ExprKind::ArrayAccess { array, index } => {
                self.infer_null_probe_operand_with_effects(array, env)?;
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

/// Returns the receiver of a zero-argument method call used directly as an expression result.
///
/// A local assignment wrapper is transparent because it only captures the call result; calls
/// with arguments remain excluded since their evaluation may mutate the guarded property.
fn zero_arg_method_receiver(expr: &Expr) -> Option<&Expr> {
    match &expr.kind {
        ExprKind::MethodCall { object, args, .. } if args.is_empty() => Some(object),
        ExprKind::Assignment { value, .. } => zero_arg_method_receiver(value),
        _ => None,
    }
}

/// Returns whether a registry builtin's shared contract cannot invalidate property type facts.
///
/// Shared/value-dependent effect resolvers remain conservative here because this flow pass does
/// not own authoritative argument types. Read-only internal-array-pointer operations are also
/// safe: they may allocate a result cell but do not move the cursor or mutate PHP storage.
/// Unknown and user-defined functions return false.
fn registry_builtin_call_preserves_property_narrowings(name: &str) -> bool {
    let Some(def) = crate::builtins::registry::lookup(name) else {
        return false;
    };
    if matches!(
        def.spec.semantics.effects,
        crate::builtins::semantics::BuiltinEffects::Static(effects) if effects.is_pure()
    ) {
        return true;
    }
    matches!(
        def.spec.semantics.argument_lowering,
        crate::builtins::semantics::BuiltinArgumentLowering::ArrayInternalPointer(op)
            if op.seek_mode().is_none()
    )
}

/// Collects the never-declared `isset()` roots a truthy `condition` binds for its branch.
///
/// Mirrors `infer_short_circuit_outcome_effects` on its truthy side: an `isset()` call, possibly
/// inside a successful `&&` chain, whose chain root is absent from `env`.
fn collect_truthy_isset_roots(condition: &Expr, env: &TypeEnv, roots: &mut Vec<String>) {
    match &condition.kind {
        ExprKind::BinaryOp {
            left,
            op: BinOp::And,
            right,
        } => {
            collect_truthy_isset_roots(left, env, roots);
            collect_truthy_isset_roots(right, env, roots);
        }
        ExprKind::FunctionCall { name, args }
            if php_symbol_key(name.trim_start_matches('\\')) == "isset" =>
        {
            for arg in args {
                if let Some(root) = null_probe::undefined_probe_root_variable(arg, env) {
                    roots.push(root.to_string());
                }
            }
        }
        _ => {}
    }
}

/// Merges storage effects from an expression arm that may not execute into its outer environment.
///
/// Array layout changes retain their common converted representation. Any other conditional
/// creation, removal, or retyping loses its precise fact and becomes `Mixed`; synthetic property
/// narrowing keys are omitted because they describe flow facts rather than runtime bindings.
fn merge_conditional_storage_effects(env: &mut TypeEnv, branch_env: &TypeEnv) {
    let mut names = env
        .keys()
        .chain(branch_env.keys())
        .cloned()
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();

    for name in names {
        let outer_ty = env.get(&name).cloned();
        let branch_ty = branch_env.get(&name).cloned();
        if outer_ty == branch_ty {
            continue;
        }
        if name.starts_with('\u{1}') {
            env.remove(&name);
            continue;
        }
        if let (Some(outer_ty), Some(branch_ty)) = (&outer_ty, &branch_ty) {
            if let Some(converted) =
                crate::types::array_storage_conversion(Some(outer_ty), branch_ty)
            {
                env.insert(name, converted);
                continue;
            }
        }
        env.insert(name, PhpType::Mixed);
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

/// Returns the plain caller variable targeted by a positional or named by-reference output.
fn by_ref_output_variable(arg: &Expr) -> Option<&String> {
    match &arg.kind {
        ExprKind::Variable(name) => Some(name),
        ExprKind::NamedArg { value, .. } => by_ref_output_variable(value),
        _ => None,
    }
}

/// Reports whether a LITERAL preg pattern declares a named capture group.
///
/// PCRE spells the same construct three ways -- `(?<name>`, `(?P<name>` and `(?'name'` -- and any
/// one of them makes PHP hand back an ordered hash for `$matches`, keyed by each name immediately
/// before its numeric twin. `(?<=` and `(?<!` are lookbehind, not a name, so they do not count.
///
/// Only a literal is inspected: a pattern built at runtime cannot be classified here, and the
/// destination keeps the type it had.
pub(crate) fn pattern_declares_named_capture_group(pattern: Option<&Expr>) -> bool {
    let Some(Expr {
        kind: ExprKind::StringLiteral(pattern),
        ..
    }) = pattern
    else {
        return false;
    };
    let bytes = pattern.as_bytes();
    let mut index = 0usize;
    while index + 2 < bytes.len() {
        if bytes[index] == b'\\' {
            index += 2;                                                 // an escaped byte cannot open a group
            continue;
        }
        if bytes[index] != b'(' || bytes[index + 1] != b'?' {
            index += 1;
            continue;
        }
        let rest = &bytes[index + 2..];
        let named = match rest.first() {
            Some(b'<') => !matches!(rest.get(1), Some(b'=' | b'!')),     // `(?<=` and `(?<!` are lookbehind
            Some(b'\'') => true,
            Some(b'P') => matches!(rest.get(1), Some(b'<')),
            _ => false,
        };
        if named {
            return true;
        }
        index += 2;
    }
    false
}

/// Reports which `$matches` shape a `preg_match_all()` call selects, when that is knowable.
///
/// `Some(true)` is PATTERN ORDER — one column per capture group, and the OUTER container becomes
/// a hash when the pattern declares a group name. `Some(false)` is `PREG_SET_ORDER`, where the
/// outer container is a plain list of rows and only the ROWS are hashes. `None` means the flags
/// expression is not readable here and either shape could arrive, so the destination has to stay
/// gradual rather than be typed as one of them.
pub(crate) fn preg_match_all_uses_pattern_order(flags: Option<&Expr>) -> Option<bool> {
    const PREG_SET_ORDER: i64 = 2;
    let Some(flags) = flags else {
        return Some(true);                                              // PHP defaults to PREG_PATTERN_ORDER
    };
    let flags = match &flags.kind {
        ExprKind::NamedArg { value, .. } => value.as_ref(),
        _ => flags,
    };
    match &flags.kind {
        ExprKind::IntLiteral(value) => Some(value & PREG_SET_ORDER == 0),
        // The two order constants are the overwhelmingly common spelling, and they name their
        // own shape; any other constant or computed expression stays unknown.
        ExprKind::ConstRef(name) => match name.to_string().trim_start_matches('\\') {
            "PREG_PATTERN_ORDER" => Some(true),
            "PREG_SET_ORDER" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// Returns the variable name used by a builtin output argument.
fn output_variable(arg: &Expr) -> Option<&String> {
    match &arg.kind {
        ExprKind::Variable(name) => Some(name),
        ExprKind::NamedArg { value, .. } => output_variable(value),
        _ => None,
    }
}

/// Identifies regex output parameters independently of named-argument source order.
fn preg_output_type(builtin: &str, arg: &Expr, index: usize) -> Option<PhpType> {
    let element = match php_symbol_key(builtin).as_str() {
        "preg_match" => PhpType::Str,
        "preg_match_all" => PhpType::Mixed,
        _ => return None,
    };
    let matches = match &arg.kind {
        ExprKind::NamedArg { name, .. } => name == "matches",
        _ => index == 2,
    };
    matches.then(|| PhpType::Array(Box::new(element)))
}

/// Returns the post-call type of a write-only PCNTL output argument, when applicable.
fn pcntl_output_type(builtin: &str, arg: &Expr, index: usize) -> Option<PhpType> {
    let builtin = php_symbol_key(builtin);
    let positional_parameter = match builtin.as_str() {
        "pcntl_wait" => ["status", "flags", "resource_usage"].get(index).copied(),
        "pcntl_waitpid" => ["process_id", "status", "flags", "resource_usage"]
            .get(index)
            .copied(),
        "pcntl_waitid" => ["idtype", "id", "info", "flags", "resource_usage"]
            .get(index)
            .copied(),
        "pcntl_sigprocmask" => ["mode", "signals", "old_signals"].get(index).copied(),
        "pcntl_sigwaitinfo" => ["signals", "info"].get(index).copied(),
        "pcntl_sigtimedwait" => ["signals", "info", "seconds", "nanoseconds"]
            .get(index)
            .copied(),
        _ => return None,
    };
    let parameter = match &arg.kind {
        ExprKind::NamedArg { name, .. } => php_symbol_key(name),
        _ => positional_parameter?.to_string(),
    };
    match parameter.as_str() {
        "status" => Some(PhpType::Int),
        "resource_usage" => Some(PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Int),
        }),
        "info" => Some(PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Mixed),
        }),
        "old_signals" => Some(PhpType::Array(Box::new(PhpType::Int))),
        _ => None,
    }
}

/// Returns the post-call type of an `xml_parse_into_struct()` write-only output argument:
/// `$values` (the tag structures) and `$index` (positions by tag name) are boxed arrays
/// handed over by the xml prelude, so both are `mixed` after the call.
fn xml_struct_output_type(builtin: &str, arg: &Expr, index: usize) -> Option<PhpType> {
    if php_symbol_key(builtin) != "xml_parse_into_struct" {
        return None;
    }
    let parameter = match &arg.kind {
        ExprKind::NamedArg { name, .. } => php_symbol_key(name),
        _ => ["parser", "data", "values", "index"]
            .get(index)
            .copied()?
            .to_string(),
    };
    match parameter.as_str() {
        "values" | "index" => Some(PhpType::Mixed),
        _ => None,
    }
}

/// Returns true when one source-order argument is OpenSSL encrypt's by-reference tag.
fn is_openssl_encrypt_tag_arg(arg: &Expr, index: usize) -> bool {
    match &arg.kind {
        ExprKind::NamedArg { name, .. } => php_symbol_key(name) == "tag",
        _ => index == 5,
    }
}

/// Finds OpenSSL encrypt's tag argument across positional and reordered named calls.
fn openssl_encrypt_tag_arg(args: &[Expr]) -> Option<&Expr> {
    args.iter()
        .find(|arg| {
            matches!(
                &arg.kind,
                ExprKind::NamedArg { name, .. } if php_symbol_key(name) == "tag"
            )
        })
        .or_else(|| {
            args.get(5)
                .filter(|arg| !matches!(arg.kind, ExprKind::NamedArg { .. }))
        })
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
