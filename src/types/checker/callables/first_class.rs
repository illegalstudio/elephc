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
                        declared_return: true,
                        ref_params: vec![false; sig.params.len()],
                        declared_params: vec![true; sig.params.len()],
                        variadic: None,
                        deprecation: None,
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
                if !matches!(&object.kind, ExprKind::Variable(_) | ExprKind::This) {
                    return Err(CompileError::new(
                        span,
                        "First-class method callable requires a variable or $this receiver",
                    ));
                }
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
                self.check_function_call_pre_normalized(
                    name.as_str(),
                    &normalized_args,
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
