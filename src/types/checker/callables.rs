use crate::errors::CompileError;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::Checker;

impl Checker {
    pub(crate) fn resolve_first_class_callable_sig(
        &mut self,
        target: &CallableTarget,
        span: crate::span::Span,
        env: &TypeEnv,
    ) -> Result<FunctionSig, CompileError> {
        match target {
            CallableTarget::Function(name) => {
                let function_name = name.as_str();
                if let Some(sig) = self.functions.get(function_name) {
                    let effective_sig =
                        Self::callable_sig_for_declared_params(sig, &sig.declared_params);
                    return Ok(Self::callable_wrapper_sig(&effective_sig));
                }
                if let Some(decl) = self.fn_decls.get(function_name).cloned() {
                    let param_types = self.initial_function_param_types(function_name, &decl)?;
                    self.resolve_function_signature(function_name, &decl, param_types)?;
                    if let Some(sig) = self.functions.get(function_name) {
                        let effective_sig =
                            Self::callable_sig_for_declared_params(sig, &sig.declared_params);
                        return Ok(Self::callable_wrapper_sig(&effective_sig));
                    }
                }
                if let Some(sig) = self.extern_functions.get(function_name) {
                    return Ok(FunctionSig {
                        params: sig.params.clone(),
                        defaults: vec![None; sig.params.len()],
                        return_type: sig.return_type.clone(),
                        ref_params: vec![false; sig.params.len()],
                        declared_params: vec![true; sig.params.len()],
                        variadic: None,
                    });
                }
                if crate::name_resolver::is_builtin_function(function_name) {
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
                        return Err(CompileError::new(
                            span,
                            "First-class callable syntax does not support static:: targets yet",
                        ));
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
                    PhpType::Object(class_name) => Err(CompileError::new(
                        span,
                        &format!(
                            "First-class instance method callables are not supported yet: {}->{}(...)",
                            class_name, method
                        ),
                    )),
                    _ => Err(CompileError::new(
                        span,
                        "First-class method callable requires an object receiver",
                    )),
                }
            }
        }
    }

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
                let normalized_args =
                    self.normalize_named_call_args(&base_sig, args, span, "first-class callable")?;
                self.check_function_call(name.as_str(), &normalized_args, span, env)?;
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
            CallableTarget::Method { .. } => {}
        }
        self.resolve_first_class_callable_sig(target, span, env)
    }

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

    pub(crate) fn resolve_expr_callable_sig(
        &mut self,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<Option<FunctionSig>, CompileError> {
        match &expr.kind {
            ExprKind::Closure {
                params,
                variadic,
                body,
                ..
            } => {
                let return_type = self.infer_closure_return_type(body, env);
                Ok(Some(FunctionSig {
                    params: params
                        .iter()
                        .map(|(name, type_ann, _, _)| {
                            let ty = match type_ann {
                                Some(type_ann) => self.resolve_declared_param_type_hint(
                                    type_ann,
                                    expr.span,
                                    &format!("Closure parameter ${}", name),
                                )?,
                                None => PhpType::Mixed,
                            };
                            Ok((name.clone(), ty))
                        })
                        .chain(
                            variadic
                                .iter()
                                .cloned()
                                .map(|name| Ok((name, PhpType::Array(Box::new(PhpType::Mixed))))),
                        )
                        .collect::<Result<Vec<_>, CompileError>>()?,
                    defaults: params
                        .iter()
                        .map(|(_, _, default, _)| default.clone())
                        .collect(),
                    return_type,
                    ref_params: params.iter().map(|(_, _, _, is_ref)| *is_ref).collect(),
                    declared_params: params
                        .iter()
                        .map(|(_, type_ann, _, _)| type_ann.is_some())
                        .chain(variadic.iter().map(|_| false))
                        .collect(),
                    variadic: variadic.clone(),
                }))
            }
            ExprKind::FirstClassCallable(target) => self
                .resolve_first_class_callable_sig(target, expr.span, env)
                .map(Some),
            ExprKind::Variable(var_name) => Ok(self.callable_sigs.get(var_name).cloned()),
            _ => Ok(None),
        }
    }

    pub(crate) fn check_extern_function_call(
        &mut self,
        name: &str,
        args: &[Expr],
        span: crate::span::Span,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let extern_sig = self.extern_functions.get(name).cloned().ok_or_else(|| {
            CompileError::new(span, &format!("Undefined extern function: {}", name))
        })?;

        let sig = self
            .functions
            .get(name)
            .cloned()
            .ok_or_else(|| CompileError::new(span, &format!("Undefined function: {}", name)))?;

        self.check_call_arity("Extern function", name, &sig, args, span)?;

        for (idx, arg) in args.iter().enumerate() {
            let Some((param_name, expected_ty)) = extern_sig.params.get(idx) else {
                break;
            };

            if *expected_ty == PhpType::Callable {
                match &arg.kind {
                    ExprKind::StringLiteral(callback_name) => {
                        self.register_callback_function(callback_name, span)?;
                    }
                    _ => {
                        return Err(CompileError::new(
                            arg.span,
                            &format!(
                                "Extern function '{}' parameter ${} expects a string literal naming a user function",
                                name, param_name
                            ),
                        ));
                    }
                }
                continue;
            }

            let actual_ty = self.infer_type(arg, env)?;
            self.require_compatible_arg_type(
                expected_ty,
                &actual_ty,
                arg.span,
                &format!("Extern function '{}' parameter ${}", name, param_name),
            )?;
        }

        Ok(extern_sig.return_type)
    }

    pub(crate) fn check_call_arity(
        &self,
        kind: &str,
        name: &str,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        let effective_arg_count = args
            .iter()
            .filter(|a| !matches!(a.kind, ExprKind::Spread(_)))
            .count();
        let has_spread = args.iter().any(|a| matches!(a.kind, ExprKind::Spread(_)));
        if has_spread {
            return Ok(());
        }

        let required = sig.defaults.iter().filter(|d| d.is_none()).count();
        if sig.variadic.is_some() {
            if effective_arg_count < required {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "{} '{}' expects at least {} arguments, got {}",
                        kind, name, required, effective_arg_count
                    ),
                ));
            }
        } else if effective_arg_count < required || effective_arg_count > sig.params.len() {
            let expected = if required == sig.params.len() {
                format!("{}", required)
            } else {
                format!("{} to {}", required, sig.params.len())
            };
            return Err(CompileError::new(
                span,
                &format!(
                    "{} '{}' expects {} arguments, got {}",
                    kind, name, expected, effective_arg_count
                ),
            ));
        }

        Ok(())
    }
}
