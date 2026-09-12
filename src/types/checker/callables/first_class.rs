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
                let function_name = name.as_str();
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
                let plan = self.plan_named_call_args(&base_sig, args, span, "first-class callable", env)?;
                let defaults = plan.default_argument_mask();
                let normalized_args = plan.normalized_args();
                self.check_function_call_pre_normalized(
                    name.as_str(),
                    &normalized_args,
                    &defaults,
                    span,
                    env,
                )?;
                self.specialize_untyped_function_params(name.as_str(), &normalized_args, env)?;
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
        if !self.call_has_traversable_spread(args, env)? {
            return self.specialize_first_class_callable_target(target, args, span, env);
        }
        self.resolve_first_class_callable_sig(target, span, env)
    }

    /// Returns whether a descriptor call contains a spread backed by Traversable storage.
    pub(crate) fn call_has_traversable_spread(
        &mut self,
        args: &[Expr],
        env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        for arg in args {
            let ExprKind::Spread(inner) = &arg.kind else {
                continue;
            };
            let ty = self.infer_type(inner, env)?;
            if matches!(ty, PhpType::Iterable)
                || matches!(&ty, PhpType::Object(class_name) if self.object_type_implements_iterable(class_name))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Recompiles `name` for the descriptor container contract when it collects a variadic tail.
    ///
    /// Handing out a first-class callable makes the function reachable through a descriptor
    /// invoker, whose container holds exactly the arguments PHP supplied, names included. The
    /// invoker's tail collector copies every unconsumed name into an associative hash, so the
    /// callee must read its variadic parameter through the runtime heap kind. An UNDECLARED
    /// variadic does not: it carries the untyped `array<int>` fallback, and nothing narrows it
    /// here because a descriptor call site has no named arguments the checker can see. The body
    /// then reads the tail hash's header as indexed storage (entry count as the length, the
    /// insertion-order head slot as element 0) and releases it as an indexed array, leaking the
    /// persisted string keys.
    ///
    /// `crate::types::signatures::descriptor_variadic_container` is the storage the invoker's own
    /// tail collector fills and the one `callable_wrapper_sig` already publishes to callers, so
    /// all three agree on one callee shape. The signature is re-resolved through
    /// `resolve_function_signature` rather than patched in place, because the body's own checked
    /// metadata (foreach storage types above all) has to be recorded against the promoted
    /// parameter type. It is idempotent: `array<mixed>` is already dynamic, so a second
    /// descriptor for the same function changes nothing.
    fn promote_descriptor_variadic_container(&mut self, name: &str) -> Result<(), CompileError> {
        if self.resolving_functions.contains(name) {
            return Ok(());
        }
        let Some(sig) = self.functions.get(name) else {
            return Ok(());
        };
        if !crate::types::signatures::variadic_needs_descriptor_container(sig) {
            return Ok(());
        }
        let variadic_name = sig.variadic.clone();
        let mut param_types = sig.params.clone();
        let Some((_, variadic_ty)) = param_types
            .iter_mut()
            .find(|(param_name, _)| Some(param_name.as_str()) == variadic_name.as_deref())
        else {
            return Ok(());
        };
        *variadic_ty = crate::types::signatures::descriptor_variadic_container();
        // Only a declaration can be re-resolved. A variant group keeps its current shape rather
        // than receiving a signature its per-variant bodies were not checked against.
        let Some(decl) = self.fn_decls.get(name).cloned() else {
            return Ok(());
        };
        self.resolve_function_signature(name, &decl, param_types)?;
        Ok(())
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
}
