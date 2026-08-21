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
use crate::parser::ast::{BinOp, CallableTarget, Expr, ExprKind, StaticReceiver};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::super::builtins::out_params;
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
                // A write-only by-ref argument is a definition, not a use: PHP auto-vivifies it
                // to `null` before the call, so reading it below would reject the manual's own
                // `stream_socket_client($url, $errno, $errstr)` idiom. The same list is what
                // binds those variables once the call has been checked.
                let write_only = out_params::write_only_variable_args(builtin_name, &expanded_args);
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
                } else if !builtin_name.eq_ignore_ascii_case("unset") {
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
                            || (builtin_name.eq_ignore_ascii_case("openssl_encrypt")
                                && is_openssl_encrypt_tag_arg(arg, idx))
                        {
                            continue;
                        }
                        if write_only.iter().any(|out| out.index == idx) {
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
                self.widen_by_ref_array_arg_types(builtin_name, &expanded_args, env);
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
                // Bind each write-only by-ref argument to the type the builtin writes. Going
                // through the ordinary assignment merge means a variable already holding an
                // incompatible type reports elephc's own reassignment error rather than having
                // the runtime write an int into, say, a string slot.
                for out in &write_only {
                    crate::types::checker::stmt_check::assignments::locals::merge_local_assignment_type(
                        self, &out.variable, &out.written, out.span, env,
                    )?;
                }
                if builtin_name.eq_ignore_ascii_case("openssl_encrypt") {
                    if let Some(arg) = openssl_encrypt_tag_arg(&expanded_args) {
                        if let Some(name) = output_variable(arg) {
                            env.insert(name.clone(), PhpType::Str);
                        }
                    }
                }
                if builtin_name.eq_ignore_ascii_case("unset") {
                    for arg in &expanded_args {
                        promote_indexed_local_for_element_unset(arg, env);
                    }
                }
                promote_indexed_local_for_key_preserving_sort(
                    builtin_name,
                    &expanded_args,
                    &self.active_ref_params,
                    env,
                );
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
                if let ExprKind::StaticMethodCall { receiver, method, .. } = &expr.kind {
                    let owner = self.static_call_owner_class(receiver);
                    self.widen_by_ref_array_static_arg_types(
                        owner.as_deref(),
                        method,
                        &expanded_args,
                        env,
                    );
                }
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
                let receiver_ty = self.infer_type_with_assignment_effects(object, env)?;
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                self.promote_pdo_binding_ref_storage(object, method, &expanded_args, env)?;
                for arg in &expanded_args {
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                let ty = self.infer_type(expr, env)?;
                self.widen_by_ref_array_method_arg_types(&receiver_ty, method, &expanded_args, env);
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

    /// Returns the class a static call dispatches to, resolving `self`/`static`/`parent`.
    fn static_call_owner_class(&self, receiver: &StaticReceiver) -> Option<String> {
        match receiver {
            StaticReceiver::Named(name) => Some(name.to_string()),
            StaticReceiver::Self_ | StaticReceiver::Static => self.current_class.clone(),
            StaticReceiver::Parent => {
                let current = self.current_class.as_ref()?;
                self.classes.get(current).and_then(|info| info.parent.clone())
            }
        }
    }

    /// Re-types a by-reference ARRAY argument of a STATIC method call.
    ///
    /// Same rule as the free-function case: a callee that widened the element representation
    /// wrote boxed slots into the caller's storage, and the caller's own type has to follow.
    fn widen_by_ref_array_static_arg_types(
        &mut self,
        owner: Option<&str>,
        method: &str,
        args: &[Expr],
        env: &mut TypeEnv,
    ) {
        let Some(owner) = owner else {
            return;
        };
        let key = php_symbol_key(method);
        let Some(sig) = self
            .classes
            .get(owner)
            .and_then(|info| info.static_methods.get(&key))
            .cloned()
        else {
            return;
        };
        Self::apply_by_ref_array_arg_types(&sig, args, env);
    }

    /// Re-types a by-reference ARRAY argument of an INSTANCE method call.
    fn widen_by_ref_array_method_arg_types(
        &mut self,
        receiver_ty: &PhpType,
        method: &str,
        args: &[Expr],
        env: &mut TypeEnv,
    ) {
        let PhpType::Object(class_name) = receiver_ty else {
            return;
        };
        let key = php_symbol_key(method);
        let Some(sig) = self
            .classes
            .get(class_name)
            .and_then(|info| info.methods.get(&key))
            .cloned()
        else {
            return;
        };
        Self::apply_by_ref_array_arg_types(&sig, args, env);
    }

    /// Writes each by-reference array parameter's type back onto the argument variable.
    ///
    /// Only a WIDENING is written. Assigning the parameter type unconditionally would let a
    /// callee compiled for `array<int>` NARROW a caller that holds `array<mixed>` — the same
    /// defect in reverse, and one no call site can justify: a callee cannot unbox storage it
    /// does not own.
    fn apply_by_ref_array_arg_types(sig: &FunctionSig, args: &[Expr], env: &mut TypeEnv) {
        for (index, arg) in args.iter().enumerate() {
            if !sig.ref_params.get(index).copied().unwrap_or(false) {
                continue;
            }
            let ExprKind::Variable(variable) = &arg.kind else {
                continue;
            };
            let Some((_, param_ty)) = sig.params.get(index) else {
                continue;
            };
            let Some(current) = env.get(variable).cloned() else {
                continue;
            };
            // A variable that held NULL and reached a by-reference parameter the callee WRITES
            // comes back holding whatever was written. The signature already says so — the
            // parameter was widened to `mixed` when the body was checked — so adopting it here
            // is what finally makes the write visible: leaving the caller typed `null`
            // constant-folded every later read to NULL, so `var_dump($x)` printed NULL after
            // `f($x)` had put an int there and `if ($x === null)` took the wrong branch.
            if matches!(current, PhpType::Void) && matches!(param_ty, PhpType::Mixed) {
                env.insert(variable.clone(), param_ty.clone());
                continue;
            }
            if !matches!(param_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                continue;
            }
            if crate::types::checker::array_element_representation_widens(&current, param_ty) {
                env.insert(variable.clone(), param_ty.clone());
            }
        }
    }

    /// Re-types a by-reference ARRAY argument to the parameter type the callee was compiled for.
    ///
    /// A by-reference parameter keeps its declared `array` — that is, `array<mixed>` — instead of
    /// adopting the call site's element type, because the callee writes into the CALLER's storage
    /// (see `functions::resolution::specialization`). The caller therefore holds boxed slots once
    /// the call returns, and its own type has to say so: leaving it `array<int>` made a LATER
    /// by-value call specialize its callee for raw slots and read the boxes back as ADDRESSES —
    /// `bump($t); total($t);` printed one.
    ///
    /// Only a plain variable is re-typed. Any other argument shape is not a by-reference target
    /// this backend resolves to a slot, and the checker rejects those separately.
    fn widen_by_ref_array_arg_types(
        &mut self,
        name: &str,
        args: &[Expr],
        env: &mut TypeEnv,
    ) {
        let canonical = self
            .canonical_function_name_folded(name)
            .unwrap_or_else(|| name.to_string());
        let Some(sig) = self.functions.get(&canonical).cloned() else {
            return;
        };
        Self::apply_by_ref_array_arg_types(&sig, args, env);
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

/// Promotes a packed indexed-array local to an int-keyed hash when a KEY-PRESERVING sort mutates
/// it through its by-reference receiver.
///
/// php sorts with `zend_array_sort(Z_ARRVAL_P(array), <comparator>, renumber)`
/// (ext/standard/array.c). `sort()` and `usort()` pass `renumber = 1`; `natsort()` and
/// `natcasesort()` pass `0`, so only the iteration order moves and every key stays attached to its
/// own value — measured, `php -n -r '$n=["img12","img2"]; natsort($n); echo json_encode($n);'`
/// prints `{"1":"img2","0":"img12"}`. A packed array stores its keys implicitly as slot positions
/// `0..n-1` and has nowhere to put that permutation, so the receiver's STORAGE has to become a
/// hash; re-typing the local as `AssocArray<Int, T>` is what makes the lowering emit
/// `Op::ArrayToHash` for it and what makes every later read of the local — `foreach`,
/// `json_encode`, `$n[0]`, `count` — go through the hash view the runtime now holds. Which
/// receivers move is `crate::types::key_preserving_sort_promotes`, the SAME predicate the lowering
/// applies; splitting that decision in two is how a local ends up with the checker and the backend
/// compiling its reads against different storage.
///
/// This is the ARGUMENT-DEPENDENT counterpart of `ParamSpec::writes`: the written type is not a
/// fixed `TypeSpec` but a function of the argument's own type (the element type carries over), and
/// the parameter is in-out rather than write-only. `promote_indexed_local_for_element_unset` is the
/// same shape one construct over — `unset($a[$k])` also re-types a local from `Array(T)` to
/// `AssocArray` because php's own semantics leave the array unable to stay packed — and this
/// re-uses that shape rather than adding a second retyping mechanism to the registry.
///
/// A local that ALIASES someone else's storage — `$s = &$r`, a `&$a` parameter — is left alone.
/// Converting one rewrites the storage the alias points at while every OTHER name for it stays
/// compiled against the packed layout, which reads the hash header as elements: measured,
/// `$r = ["img12","img2"]; $s = &$r; natsort($s); json_encode($r)` printed `[5,0]`. The lowering
/// skips the same locals through `is_ref_bound_local`, so neither side promotes storage the other
/// would not.
fn promote_indexed_local_for_key_preserving_sort(
    builtin_name: &str,
    args: &[Expr],
    ref_bound: &std::collections::HashSet<String>,
    env: &mut TypeEnv,
) {
    let [arg] = args else {
        return;
    };
    let Some(name) = output_variable(arg) else {
        return;
    };
    if ref_bound.contains(name) {
        return;
    }
    let Some(PhpType::Array(elem_ty)) = env.get(name).cloned() else {
        return;
    };
    if !crate::types::key_preserving_sort_promotes(builtin_name, &elem_ty) {
        return;
    }
    env.insert(
        name.clone(),
        PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: elem_ty,
        },
    );
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
