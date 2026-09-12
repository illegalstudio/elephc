//! Purpose:
//! Type-checks callable first class behavior.
//! Infers callable signatures and validates invocation details that affect later lowering and optimizer effects.
//!
//! Called from:
//! - `crate::types::checker::callables`
//! - `crate::types::checker::inference`
//!
//! Key details:
//! - Closure captures, first-class callable syntax, and extern calls must agree with shared call argument planning.

use crate::errors::CompileError;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::Checker;

impl Checker {
    /// Resolves the canonical `FunctionSig` for a first-class callable expression.
    ///
    /// LFC prefers an elephc extension builtin over a same-named strict-PHP user
    /// declaration; other targets use user declarations, externs, then builtins.
    /// Returns a wrapped signature where all parameters are marked as declared (callable syntax has no
    /// type inference at the call site). Visibility checks are applied for static-method and instance-method targets.
    ///
    /// # Errors
    /// - `Undefined function for first-class callable` when the function is not registered.
    /// - `Undefined class` when the receiver class does not exist.
    /// - `Cannot access <visibility> method` when the method is not accessible.
    /// - `First-class callable syntax only supports static methods here` for non-static method names on static targets.
    pub(crate) fn resolve_first_class_callable_sig(
        &mut self,
        target: &CallableTarget,
        span: crate::span::Span,
        env: &TypeEnv,
    ) -> Result<FunctionSig, CompileError> {
        match target {
            CallableTarget::Function(name) => {
                let canonical_function_name = self
                    .canonical_function_name_folded(name.as_str())
                    .unwrap_or_else(|| name.as_str().to_string());
                let function_name = canonical_function_name.as_str();
                let function_key =
                    crate::names::php_symbol_key(function_name.trim_start_matches('\\'));
                let prefer_extension_builtin = !crate::strict_php::is_enabled()
                    && crate::types::checker::builtins::catalog::strict_php_hidden_builtin_for_profile(
                        &function_key,
                        true,
                );
                if prefer_extension_builtin {
                    self.require_first_class_callable_builtin_libraries(&function_key);
                    if let Some(message) =
                        crate::builtins::registry::first_class_callable_rejection(&function_key)
                    {
                        return Err(CompileError::new(span, message));
                    }
                    return crate::types::first_class_callable_builtin_sig(&function_key)
                        .ok_or_else(|| {
                            CompileError::new(
                                span,
                                &format!(
                                    "First-class callable syntax does not support builtin '{}' yet",
                                    function_name
                                ),
                            )
                        });
                }
                if self.function_variant_groups.contains_key(function_name)
                    && !self.functions.contains_key(function_name)
                {
                    self.ensure_function_variant_group_signature(function_name, span)?;
                }
                self.promote_descriptor_variadic_container(function_name)?;
                if let Some(sig) = self.functions.get(function_name) {
                    let effective_sig =
                        Self::callable_sig_for_declared_params(sig, &sig.declared_params);
                    return Ok(Self::callable_wrapper_sig(&effective_sig));
                }
                if let Some(decl) = self.fn_decls.get(function_name).cloned() {
                    let param_types = self.initial_function_param_types(function_name, &decl)?;
                    self.resolve_function_signature(function_name, &decl, param_types)?;
                    self.promote_descriptor_variadic_container(function_name)?;
                    if let Some(sig) = self.functions.get(function_name) {
                        let effective_sig =
                            Self::callable_sig_for_declared_params(sig, &sig.declared_params);
                        return Ok(Self::callable_wrapper_sig(&effective_sig));
                    }
                }
                if let Some(sig) = self.extern_functions.get(function_name) {
                    return Ok(FunctionSig {
                        params: sig.params.clone(),
                        param_type_exprs: vec![None; sig.params.len()],
                        param_attributes: Vec::new(),
                        defaults: vec![None; sig.params.len()],
                        return_type: sig.return_type.clone(),
                        declared_return: true,
                        by_ref_return: false,
                        ref_params: vec![false; sig.params.len()],
                        declared_params: vec![true; sig.params.len()],
                        variadic: None,
                        deprecation: None,
                    });
                }
                if crate::name_resolver::is_builtin_function(function_name) {
                    self.require_first_class_callable_builtin_libraries(&function_key);
                    if let Some(message) =
                        crate::builtins::registry::first_class_callable_rejection(&function_key)
                    {
                        return Err(CompileError::new(span, message));
                    }
                    return crate::types::first_class_callable_builtin_sig(function_name)
                        .ok_or_else(|| {
                            CompileError::new(
                                span,
                                &format!(
                                    "First-class callable syntax does not support builtin '{}' yet",
                                    function_name
                                ),
                            )
                        });
                }
                Err(CompileError::new(
                    span,
                    &format!(
                        "Undefined function for first-class callable: {}",
                        function_name
                    ),
                ))
            }
            CallableTarget::StaticMethod { receiver, method } => {
                let resolved_class_name = match receiver {
                    StaticReceiver::Named(class_name) => class_name.as_str().to_string(),
                    StaticReceiver::Self_ => {
                        self.current_class.as_ref().cloned().ok_or_else(|| {
                            CompileError::new(
                                span,
                                "Cannot use self:: in first-class callable outside class method scope",
                            )
                        })?
                    }
                    StaticReceiver::Static => {
                        self.current_class.as_ref().cloned().ok_or_else(|| {
                            CompileError::new(
                                span,
                                "Cannot use static:: in first-class callable outside class method scope",
                            )
                        })?
                    }
                    StaticReceiver::Parent => {
                        let current_class = self.current_class.as_ref().ok_or_else(|| {
                            CompileError::new(
                                span,
                                "Cannot use parent:: in first-class callable outside class method scope",
                            )
                        })?;
                        let current_info = self.classes.get(current_class).ok_or_else(|| {
                            CompileError::new(
                                span,
                                &format!("Undefined class: {}", current_class),
                            )
                        })?;
                        current_info.parent.as_ref().cloned().ok_or_else(|| {
                            CompileError::new(
                                span,
                                &format!("Class {} has no parent class", current_class),
                            )
                        })?
                    }
                };

                // Before the signature is read, not after: handing out the callable is what makes
                // the method descriptor-reachable, and the wrapper published below has to
                // describe the container the method's own frame will be compiled for.
                self.promote_descriptor_variadic_container_for_method(
                    &resolved_class_name,
                    method,
                    true,
                );
                let class_info = self.classes.get(&resolved_class_name).ok_or_else(|| {
                    CompileError::new(span, &format!("Undefined class: {}", resolved_class_name))
                })?;
                let sig = class_info.static_methods.get(method).ok_or_else(|| {
                    if class_info.methods.contains_key(method) {
                        CompileError::new(
                            span,
                            &format!(
                                "First-class callable syntax only supports static methods here: {}::{}",
                                resolved_class_name, method
                            ),
                        )
                    } else {
                        CompileError::new(
                            span,
                            &format!(
                                "Undefined static method for first-class callable: {}::{}",
                                resolved_class_name, method
                            ),
                        )
                    }
                })?;
                if let Some(visibility) = class_info.static_method_visibilities.get(method) {
                    let declaring_class = class_info
                        .static_method_declaring_classes
                        .get(method)
                        .map(String::as_str)
                        .unwrap_or(resolved_class_name.as_str());
                    if !self.can_access_member(declaring_class, visibility) {
                        return Err(CompileError::new(
                            span,
                            &format!(
                                "Cannot access {} method: {}::{}",
                                Self::visibility_label(visibility),
                                resolved_class_name,
                                method
                            ),
                        ));
                    }
                }
                let declared_flags = Self::declared_method_param_flags(class_info, method, true);
                let effective_sig = Self::callable_sig_for_declared_params(sig, &declared_flags);
                Ok(Self::callable_wrapper_sig(&effective_sig))
            }
            CallableTarget::Method { object, method } => {
                let object_ty = self.infer_type(object, env)?;
                match object_ty {
                    PhpType::Object(class_name) => {
                        // See the static-method arm: promote before publishing the wrapper, so
                        // the descriptor and the method frame agree on one collector container.
                        self.promote_descriptor_variadic_container_for_method(
                            &class_name,
                            method,
                            false,
                        );
                        let class_info = self.classes.get(&class_name).ok_or_else(|| {
                            CompileError::new(span, &format!("Undefined class: {}", class_name))
                        })?;
                        let sig = class_info.methods.get(method).ok_or_else(|| {
                            CompileError::new(
                                span,
                                &format!(
                                    "Undefined method for first-class callable: {}::{}",
                                    class_name, method
                                ),
                            )
                        })?;
                        if let Some(visibility) = class_info.method_visibilities.get(method) {
                            let declaring_class = class_info
                                .method_declaring_classes
                                .get(method)
                                .map(String::as_str)
                                .unwrap_or(class_name.as_str());
                            if !self.can_access_member(declaring_class, visibility) {
                                return Err(CompileError::new(
                                    span,
                                    &format!(
                                        "Cannot access {} method: {}::{}",
                                        Self::visibility_label(visibility),
                                        class_name,
                                        method
                                    ),
                                ));
                            }
                        }
                        let declared_flags =
                            Self::declared_method_param_flags(class_info, method, false);
                        let effective_sig =
                            Self::callable_sig_for_declared_params(sig, &declared_flags);
                        Ok(Self::callable_wrapper_sig(&effective_sig))
                    }
                    _ => Err(CompileError::new(
                        span,
                        "First-class method callable requires an object receiver",
                    )),
                }
            }
        }
    }

    /// Specializes an untyped user-defined function for a first-class callable call site.
    ///
    /// After resolving the base signature, if any parameters are inferred (not declared),
    /// this method normalizes named arguments, performs full type-checking via
    /// `check_function_call_pre_normalized`, and then re-resolves the now-specialized signature.
    /// Builtin and extern functions are returned unchanged since they cannot be specialized.
    ///
    /// # Errors
    /// Propagates errors from `normalize_named_call_args`, `check_function_call_pre_normalized`,
    /// and `specialize_untyped_function_params`.
    pub(crate) fn specialize_first_class_callable_target(
        &mut self,
        target: &CallableTarget,
        args: &[Expr],
        span: crate::span::Span,
        env: &TypeEnv,
    ) -> Result<FunctionSig, CompileError> {
        let base_sig = self.resolve_first_class_callable_sig(target, span, env)?;
        if base_sig.declared_params.iter().all(|is_declared| *is_declared) {
            return Ok(base_sig);
        }
        match target {
            CallableTarget::Function(name) => {
                if crate::name_resolver::is_builtin_function(name.as_str()) {
                    return Ok(base_sig);
                }
                let function_name = self
                    .canonical_function_name_folded(name.as_str())
                    .unwrap_or_else(|| name.as_str().to_string());
                let plan = self.plan_named_call_args(&base_sig, args, span, "first-class callable", env)?;
                let defaults = plan.default_argument_mask();
                let descriptor_projections = plan.descriptor_projection_mask();
                let normalized_args = plan.normalized_args();
                self.check_function_call_pre_normalized(
                    &function_name,
                    &normalized_args,
                    &defaults,
                    &descriptor_projections,
                    span,
                    env,
                )?;
                self.specialize_untyped_function_params(&function_name, &normalized_args, env)?;
            }
            CallableTarget::StaticMethod { receiver, method } => {
                let call_expr = Expr::new(
                    ExprKind::StaticMethodCall {
                        receiver: receiver.clone(),
                        method: method.clone(),
                        args: args.to_vec(),
                    },
                    span,
                );
                self.infer_type(&call_expr, env)?;
            }
            CallableTarget::Method { object, method } => {
                let call_expr = Expr::new(
                    ExprKind::MethodCall {
                        object: object.clone(),
                        method: method.clone(),
                        args: args.to_vec(),
                    },
                    span,
                );
                self.infer_type(&call_expr, env)?;
            }
        }
        self.resolve_first_class_callable_sig(target, span, env)
    }

    /// Specializes a first-class target for a descriptor invocation such as `call_user_func()`.
    ///
    /// A Traversable spread has no statically addressable elements, so ordinary direct-call
    /// specialization cannot synthesize array-index reads for its parameter slots. Preserve the
    /// already-checked fallback signature and let descriptor-aware validation accept the runtime
    /// iterator walk. Array spreads and calls without spreads keep the existing specialization.
    pub(crate) fn specialize_first_class_callable_target_for_descriptor_call(
        &mut self,
        target: &CallableTarget,
        args: &[Expr],
        span: crate::span::Span,
        env: &TypeEnv,
    ) -> Result<FunctionSig, CompileError> {
        if !self.descriptor_call_has_traversable_spread(args, env)? {
            return self.specialize_first_class_callable_target(target, args, span, env);
        }
        self.resolve_first_class_callable_sig(target, span, env)
    }

    /// Recompiles `name` for the descriptor container contract when it collects a variadic tail.
    ///
    /// Handing out a first-class callable makes the callable reachable through a descriptor
    /// invoker, whose container holds exactly the arguments PHP supplied, names included. The
    /// invoker's tail collector copies every unconsumed name into an associative hash, so the
    /// callee must read its variadic parameter through the runtime heap kind. Neither the
    /// untyped `array<int>` fallback nor the declared `array<T>` of an `int ...$xs` can: the body
    /// reads the tail hash's header as indexed storage (entry count as the length, the
    /// insertion-order head slot as element 0) and releases it as an indexed array, leaking the
    /// persisted string keys.
    ///
    /// `crate::types::signatures::descriptor_variadic_container` is the storage the invoker's own
    /// tail collector fills, so both agree on one callee shape. The signature is re-resolved
    /// through `resolve_function_signature` rather than patched in place, because the body's own
    /// checked metadata (foreach storage types above all) has to be recorded against the promoted
    /// parameter type. `param_type_exprs` and `declared_params` come back from the DECLARATION on
    /// every re-resolve, so a declared `int ...$xs` keeps the source element contract its direct
    /// call sites are checked against while only its storage moves. It is idempotent:
    /// `array<mixed>` is already dynamic, so a second descriptor for the same function changes
    /// nothing.
    pub(crate) fn promote_descriptor_variadic_container(
        &mut self,
        name: &str,
    ) -> Result<(), CompileError> {
        // A conditional function is a VARIANT GROUP: the group name carries the unified signature
        // every call site sees, and each variant carries the body that is actually compiled. Both
        // have to move, or the descriptor publishes one container while the selected variant's
        // frame reads the other. Promoting the variants first and re-unifying afterwards keeps
        // the group signature derived from them rather than patched independently.
        if let Some(variants) = self.function_variant_groups.get(name).cloned() {
            // All or nothing, checked BEFORE the first variant moves. A group is only legal while
            // every variant shares one signature, so promoting a subset would either be rejected
            // by the re-unification below as a variant mismatch, or leave the group publishing a
            // container that the one variant that could not move does not read. A group with any
            // immovable variant therefore keeps indexed storage throughout, and a named tail
            // aimed at it is refused by the invoker rather than silently reinterpreted.
            if !variants
                .iter()
                .all(|variant| self.declared_variadic_container_is_movable(variant))
            {
                return Ok(());
            }
            let mut promoted = false;
            for variant in &variants {
                promoted |= self.promote_declared_variadic_container(variant)?;
            }
            if promoted {
                self.functions.remove(name);
                self.ensure_function_variant_group_signature(name, crate::span::Span::dummy())?;
            }
            return Ok(());
        }
        self.promote_declared_variadic_container(name)?;
        Ok(())
    }

    /// Promotes ONE declared function's variadic collector, reporting whether it moved.
    ///
    /// Returns `false` without touching anything when the function has no declaration to
    /// re-resolve, when its body is already being walked (the promotion would recurse into the
    /// resolution it is running inside), or when the collector is already the descriptor
    /// container. A function with no declaration cannot be recompiled for a different parameter
    /// type, so it keeps its current shape rather than receiving a signature its body was never
    /// checked against; the invoker refuses a named tail for such a callee rather than filling a
    /// container it cannot read (see
    /// `crate::codegen::runtime_callable_invoker::variadic_collector_accepts_named_entries`).
    fn promote_declared_variadic_container(
        &mut self,
        name: &str,
    ) -> Result<bool, CompileError> {
        if self.resolving_functions.contains(name) {
            return Ok(false);
        }
        let Some(sig) = self.functions.get(name) else {
            return Ok(false);
        };
        if !crate::types::signatures::variadic_needs_descriptor_container(sig) {
            return Ok(false);
        }
        let Some(index) = crate::types::signatures::variadic_param_index(sig) else {
            return Ok(false);
        };
        let mut param_types = sig.params.clone();
        param_types[index].1 = crate::types::signatures::descriptor_variadic_container();
        let Some(decl) = self.fn_decls.get(name).cloned() else {
            return Ok(false);
        };
        self.resolve_function_signature(name, &decl, param_types)?;
        Ok(true)
    }

    /// Returns whether a variant can move to descriptor-safe variadic storage atomically.
    fn declared_variadic_container_is_movable(&self, name: &str) -> bool {
        if self.resolving_functions.contains(name) {
            return false;
        }
        let Some(sig) = self.functions.get(name) else {
            return false;
        };
        !crate::types::signatures::variadic_needs_descriptor_container(sig)
            || self.fn_decls.contains_key(name)
    }

    /// Promotes a class method's variadic collector for the descriptor container contract.
    ///
    /// A method reached through `$o->m(...)`, `C::m(...)`, or a `[$o, 'm']` callable is invoked
    /// through the same descriptor invoker as a free function, so its collector needs the same
    /// storage. Unlike a free function there is nothing to re-resolve here: the stored class
    /// signature IS what seeds the body's parameter environment
    /// (`crate::types::checker::method_pass`, which reads the collector out of `sig_params`), and
    /// `type_check_methods_until_stable` runs method bodies to a fixed point over `self.classes`.
    /// Mutating the stored signature therefore makes that loop observe a change and re-check the
    /// body against the promoted container by itself. Returning early when nothing moves is what
    /// keeps the fixed point reachable.
    pub(crate) fn promote_descriptor_variadic_container_for_method(
        &mut self,
        class_name: &str,
        method: &str,
        is_static: bool,
    ) {
        let Some(class_info) = self.classes.get_mut(class_name) else {
            return;
        };
        let table = if is_static {
            &mut class_info.static_methods
        } else {
            &mut class_info.methods
        };
        // Method tables are keyed by the folded PHP symbol key, but the callers reach this with
        // the spelling their own syntax carried: first-class callable syntax hands over the
        // source name, and a `[$o, 'Add']` callable array hands over a string literal PHP
        // matches case-insensitively. Trying the given spelling first keeps the exact-match
        // path allocation-free and makes the fold a fallback rather than a reformatting step.
        let key = if table.contains_key(method) {
            method.to_string()
        } else {
            crate::names::php_symbol_key(method)
        };
        let Some(sig) = table.get_mut(&key) else {
            return;
        };
        crate::types::signatures::promote_variadic_to_descriptor_container(sig);
    }

    /// Promotes whatever callable a resolved [`CallableTarget`] names, for any of its three kinds.
    ///
    /// `resolve_first_class_callable_sig` covers the targets written as first-class callable
    /// syntax, but a `[$object, 'method']` or `[Klass::class, 'method']` callable array reaches
    /// the SAME descriptor invoker without ever passing through it: the pair is recorded as a
    /// target when it is assigned (`crate::types::checker::stmt_check::assignments::locals`) and
    /// invoked through that record (`Checker::infer_callable_array_target_call`). Both of those
    /// are descriptor materialization points, so both promote through here.
    ///
    /// Receiver resolution is deliberately INFERENCE-FREE. The two call sites either already
    /// inferred the receiver or are recording an assignment whose environment already holds its
    /// type, and re-running `infer_type` for a promotion decision would re-record argument
    /// aliases and re-emit narrowing as a side effect of asking a storage question. An
    /// unresolvable receiver simply promotes nothing: the callee then keeps indexed storage and
    /// the invoker refuses a named tail rather than corrupting it (see
    /// `crate::codegen::runtime_callable_invoker::variadic_collector_accepts_named_entries`).
    pub(crate) fn promote_descriptor_variadic_container_for_callable_target(
        &mut self,
        target: &CallableTarget,
        env: &TypeEnv,
    ) -> Result<(), CompileError> {
        match target {
            CallableTarget::Function(name) => {
                self.promote_descriptor_variadic_container(name.as_str())
            }
            CallableTarget::StaticMethod { receiver, method } => {
                if let Some(class_name) = self.static_receiver_class_without_inference(receiver) {
                    self.promote_descriptor_variadic_container_for_method(
                        &class_name,
                        method,
                        true,
                    );
                }
                Ok(())
            }
            CallableTarget::Method { object, method } => {
                let receiver_ty = match &object.kind {
                    ExprKind::Variable(name) => env.get(name).cloned(),
                    _ => None,
                }
                .unwrap_or_else(|| {
                    crate::types::checker::infer_expr_type_syntactic(object)
                });
                if let Some(class_name) = self.invokable_class_for_type(&receiver_ty) {
                    self.promote_descriptor_variadic_container_for_method(
                        &class_name,
                        method,
                        false,
                    );
                }
                Ok(())
            }
        }
    }

    /// Resolves a static receiver to a class name without inferring anything, or gives up.
    ///
    /// `self::`, `static::` and `parent::` are answered from the checker's current class context,
    /// which is the same context `resolve_first_class_callable_sig` reads. Unlike that path this
    /// one reports `None` instead of a diagnostic: it serves a storage decision, and a receiver
    /// that cannot be resolved here is either reported by the surrounding call check or is not a
    /// call at all.
    fn static_receiver_class_without_inference(
        &self,
        receiver: &StaticReceiver,
    ) -> Option<String> {
        match receiver {
            StaticReceiver::Named(class_name) => Some(class_name.as_str().to_string()),
            StaticReceiver::Self_ | StaticReceiver::Static => self.current_class.clone(),
            StaticReceiver::Parent => self
                .classes
                .get(self.current_class.as_deref()?)
                .and_then(|class_info| class_info.parent.clone()),
        }
    }

    /// Infers the return type of a first-class callable target without performing specialization.
    ///
    /// Delegates to `resolve_first_class_callable_sig` and extracts the `return_type` field.
    /// Used when the callable is used in a non-invocation context (e.g., `typeof($fn)`).
    pub(crate) fn infer_first_class_callable_target(
        &mut self,
        target: &CallableTarget,
        span: crate::span::Span,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        Ok(self
            .resolve_first_class_callable_sig(target, span, env)?
            .return_type)
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::ast::{Stmt, StmtKind};
    use crate::codegen_support::platform::Target;
    use crate::types::PhpType;

    /// A variadic reached only through a callable descriptor compiles for the dynamic container.
    ///
    /// `keepsSpareName` has no direct call site, so nothing else can tell the checker that its
    /// `...$rest` may receive a NAME: the descriptor invoker's tail collector copies every
    /// unconsumed name into an associative hash, and a body compiled for the untyped `array<int>`
    /// fallback reads that hash's header as indexed storage. That is exactly how `['z' => 9]`
    /// printed as `i0=13`: entry count 1 as the length, the insertion-order head slot (FNV-1a of
    /// `"z"` modulo the 16-slot capacity) as element 0, while the hash's persisted string key
    /// leaked, because an indexed release never frees hash keys.
    ///
    /// The second function is the scope control: a variadic with only direct positional call sites
    /// must keep its specialized `array<int>` storage, so the promotion cannot quietly box every
    /// variadic in the program.
    #[test]
    fn a_descriptor_variadic_is_dynamic_on_every_target() {
        let source = r#"<?php
function keepsSpareName(int $a, ...$rest): string {
    $out = (string) $a;
    foreach ($rest as $key => $value) {
        $out .= '|' . (is_string($key) ? 's' : 'i') . $key . '=' . $value;
    }
    return $out;
}
function positionalOnlyTail(...$rest): int { return count($rest); }
function spareNamedArray(): array { return [0 => 1, 'z' => 9]; }
function unpackSpareName(callable $callback): mixed { return $callback(...spareNamedArray()); }
echo unpackSpareName(keepsSpareName(...)), ':', positionalOnlyTail(1, 2);
"#;
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let target = Target::parse(name).expect("supported target");
            let checked =
                crate::types::checker::check_types(&program, target).expect("check");

            let descriptor_tail = checked
                .functions
                .get("keepsSpareName")
                .expect("the first-class callable target keeps a signature")
                .params
                .last()
                .expect("the variadic occupies the last parameter slot")
                .clone();
            assert_eq!(
                descriptor_tail.1,
                crate::types::signatures::descriptor_variadic_container(),
                "{name}: a descriptor-reachable variadic must read its tail through the \
                 runtime heap kind, not as indexed array<int> storage",
            );

            let direct_tail = checked
                .functions
                .get("positionalOnlyTail")
                .expect("the directly called variadic keeps a signature")
                .params
                .last()
                .expect("the variadic occupies the last parameter slot")
                .clone();
            assert_eq!(
                direct_tail.1,
                PhpType::Array(Box::new(PhpType::Int)),
                "{name}: a variadic with only positional direct call sites must keep its \
                 specialized element storage",
            );
        }
    }

    /// A first-class callable may name an include function variant before the final checker pass.
    ///
    /// The resolver normally synthesizes the group statement from mutually exclusive includes.
    /// Building that exact checker input here pins both early group materialization and PHP's
    /// case-insensitive lookup without requiring an executable multi-file fixture.
    #[test]
    fn a_first_class_callable_materializes_a_case_folded_function_variant_group() {
        let source = r#"<?php
function variant_tail_left(string $head, ...$rest): string { return $head; }
function variant_tail_right(string $head, ...$rest): string { return $head; }
$callback = vArIaNt_TaIl(...);
"#;
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        let mut program = crate::parser::parse(&tokens).expect("parse");
        program.push(Stmt::new(
            StmtKind::FunctionVariantGroup {
                name: "variant_tail".to_string(),
                variants: vec![
                    "variant_tail_left".to_string(),
                    "variant_tail_right".to_string(),
                ],
            },
            crate::span::Span::dummy(),
        ));

        let checked = crate::types::checker::check_types(
            &program,
            Target::parse("linux-x86_64").expect("supported target"),
        )
        .expect("a statically inventoried variant group is a valid first-class callable");
        let tail = checked
            .functions
            .get("variant_tail")
            .expect("the group signature is materialized during callable resolution")
            .params
            .last()
            .expect("the variadic occupies the last parameter slot");
        assert_eq!(
            tail.1,
            crate::types::signatures::descriptor_variadic_container(),
            "the group and its variants must use the descriptor-safe tail container",
        );
    }
}
