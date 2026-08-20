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

use super::super::super::null_probe;
use super::super::super::Checker;
use super::{merge_match_arm_result_type, merge_null_coalesce_result_type};

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
    /// - Ternary, null coalesce, and match clone the environment per branch so assignments
    ///   do not leak across arms; match/ternary result types then reuse `infer_type`'s
    ///   Mixed-aware branch merge (not the Str-absorbing syntactic join).
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
                // `int` can overflow to float and `string` can become int/float
                // (`"9"++` is `int(10)`), so the local is dynamically typed afterwards.
                if matches!(old_ty, Some(PhpType::Int) | Some(PhpType::Str)) {
                    env.insert(name.clone(), PhpType::Mixed);
                }
                Ok(result_ty)
            }
            ExprKind::PostIncrement(name) | ExprKind::PostDecrement(name) => {
                let old_ty = env.get(name).cloned();
                let result_ty = self.infer_type(expr, env)?;
                // Same retype as the pre-form: only the RESULT differs, and it was already
                // computed above against the type the local held before the update.
                if matches!(old_ty, Some(PhpType::Int) | Some(PhpType::Str)) {
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
                // `$neverDefined ?? $d` is a null probe: PHP answers `$d` without an
                // undefined-variable warning, so the left chain root reads as `null`.
                let probe = null_probe::begin_null_probe_root(self, value, env);
                let value_ty = self.infer_null_probe_operand_with_effects(value, env);
                null_probe::end_null_probe_root(probe, env);
                let value_ty = value_ty?;
                let default_ty = if value_ty == PhpType::Void {
                    self.infer_type_with_assignment_effects(default, env)?
                } else {
                    let mut default_env = env.clone();
                    let default_ty =
                        self.infer_type_with_assignment_effects(default, &mut default_env)?;
                    merge_array_storage_effects(env, &default_env);
                    default_ty
                };
                let non_null_value = if Self::union_contains_void(&value_ty) {
                    self.strip_void_from_union(&value_ty)
                } else {
                    value_ty
                };
                Ok(merge_null_coalesce_result_type(
                    non_null_value,
                    default_ty,
                ))
            }
            ExprKind::ShortTernary { value, default } => {
                let value_ty = self.infer_type_with_assignment_effects(value, env)?;
                if value_ty == PhpType::Void {
                    self.infer_type_with_assignment_effects(default, env)?;
                } else {
                    let mut default_env = env.clone();
                    self.infer_type_with_assignment_effects(default, &mut default_env)?;
                    merge_array_storage_effects(env, &default_env);
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
                if let Some(guard) = guard {
                    then_env.insert(guard.var.clone(), guard.then_ty);
                    else_env.insert(guard.var, guard.else_ty);
                }
                let then_ty = self.infer_type_with_assignment_effects(then_expr, &mut then_env)?;
                merge_array_storage_effects(env, &then_env);
                merge_array_storage_effects(&mut else_env, &then_env);
                let else_ty = self.infer_type_with_assignment_effects(else_expr, &mut else_env)?;
                merge_array_storage_effects(env, &else_env);
                Ok(merge_match_arm_result_type(self, then_ty, else_ty))
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
                for (conditions, result) in arms {
                    let mut arm_env = env.clone();
                    for condition in conditions {
                        self.infer_type_with_assignment_effects(condition, &mut arm_env)?;
                    }
                    self.infer_type_with_assignment_effects(result, &mut arm_env)?;
                    merge_array_storage_effects(env, &arm_env);
                }
                if let Some(default) = default {
                    let mut default_env = env.clone();
                    self.infer_type_with_assignment_effects(default, &mut default_env)?;
                    merge_array_storage_effects(env, &default_env);
                }
                // Result type comes from the Mixed-aware match merge in `infer_type`
                // (assignment effects must not reintroduce the Str-absorbing syntactic join).
                self.infer_type(expr, env)
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
                    // `isset($never)` / `unset($never)` are exactly the constructs PHP provides
                    // for probing storage that may never have been declared, so a never-declared
                    // chain root reads as `null` for the operand.
                    for arg in &expanded_args {
                        let probe = null_probe::begin_null_probe_root(self, arg, env);
                        let effects = self.infer_non_reading_arg_assignment_effects(arg, env);
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
                    for (idx, arg) in expanded_args.iter().enumerate() {
                        if contextual_callbacks.contains(&idx) {
                            continue;
                        }
                        if (builtin_name.eq_ignore_ascii_case("preg_match") && idx == 2)
                            || pcntl_output_type(builtin_name, arg, idx).is_some()
                            || (builtin_name.eq_ignore_ascii_case("openssl_encrypt")
                                && is_openssl_encrypt_tag_arg(arg, idx))
                        {
                            continue;
                        }
                        if is_empty {
                            let probe = null_probe::begin_null_probe_root(self, arg, env);
                            let effects = self.infer_type_with_assignment_effects(arg, env);
                            null_probe::end_null_probe_root(probe, env);
                            effects?;
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
                        if let Some(name) = output_variable(arg) {
                            env.insert(name.clone(), PhpType::Array(Box::new(PhpType::Str)));
                        }
                    }
                }
                for (idx, arg) in expanded_args.iter().enumerate() {
                    if let Some(output_ty) = pcntl_output_type(builtin_name, arg, idx) {
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

    /// Widens PDO binding destinations before by-reference signature validation.
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
        let parameter_name = match crate::names::php_symbol_key(method).as_str() {
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

/// Returns whether a receiver type may contain a PDOStatement instance.
fn type_may_be_pdo_statement(ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(class) => class.trim_start_matches('\\') == "PDOStatement",
        PhpType::Union(members) => members.iter().any(type_may_be_pdo_statement),
        _ => false,
    }
}

/// Merges array layout conversions from a conditional expression arm into its outer environment.
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

/// Returns the variable name used by a builtin output argument.
fn output_variable(arg: &Expr) -> Option<&String> {
    match &arg.kind {
        ExprKind::Variable(name) => Some(name),
        ExprKind::NamedArg { value, .. } => output_variable(value),
        _ => None,
    }
}

/// Returns the post-call type of a write-only PCNTL output argument, when applicable.
fn pcntl_output_type(builtin: &str, arg: &Expr, index: usize) -> Option<PhpType> {
    let builtin = php_symbol_key(builtin);
    let positional_parameter = match builtin.as_str() {
        "pcntl_wait" => ["status", "flags", "resource_usage"].get(index).copied(),
        "pcntl_waitpid" => ["process_id", "status", "flags", "resource_usage"]
            .get(index)
            .copied(),
        "pcntl_waitid" => ["idtype", "id", "info", "flags"].get(index).copied(),
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
