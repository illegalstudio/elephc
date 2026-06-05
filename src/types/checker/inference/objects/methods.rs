//! Purpose:
//! Infers object methods expression types.
//! Validates class, method, constructor, property, and magic-access contracts against schema metadata.
//!
//! Called from:
//! - `crate::types::checker::inference::objects`
//!
//! Key details:
//! - Object inference depends on flattened class metadata, visibility, inheritance, and declared property types.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::super::Checker;
use super::super::syntactic::wider_type_syntactic;

impl Checker {
    /// Infers the type of a method call expression (`$obj->method(...)`).
    ///
    /// Dispatches to `infer_method_call_on_class_type` for `Object` types,
    /// `infer_method_call_on_interface_type` for interface types, and
    /// handles nullable union receivers. Returns `PhpType::Int` as a fallback
    /// for unhandled types (e.g. `Mixed` without specific handler).
    pub(crate) fn infer_method_call_type(
        &mut self,
        object: &Expr,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let obj_ty = self.infer_type(object, env)?;
        if let PhpType::Object(class_name) = &obj_ty {
            if self.interfaces.contains_key(class_name) {
                return self.infer_method_call_on_interface_type(
                    class_name, method, args, expr, env,
                );
            }
            return self.infer_method_call_on_class_type(class_name, method, args, expr, env);
        }
        // Method calls on a union object type are allowed when the union has a
        // single object class. `?Foo` / `Foo|null` faults on a null receiver as in
        // PHP; `Foo|false` (and other object-plus-scalar unions) dispatch on the
        // runtime class id and fault when the value is not an object. Either way
        // the checker surfaces the method's return type so callers can chain.
        if let PhpType::Union(_) = &obj_ty {
            let class_name = self.union_single_object_class(&obj_ty).or_else(|| {
                self.nullsafe_object_receiver(&obj_ty, expr, "method call")
                    .ok()
                    .flatten()
                    .map(|(name, _nullable)| name)
            });
            if let Some(class_name) = class_name {
                if self.interfaces.contains_key(&class_name) {
                    return self.infer_method_call_on_interface_type(
                        &class_name,
                        method,
                        args,
                        expr,
                        env,
                    );
                }
                return self.infer_method_call_on_class_type(&class_name, method, args, expr, env);
            }
            // No single object class: re-run the strict check to surface its
            // diagnostic (e.g. a union of two distinct object classes).
            self.nullsafe_object_receiver(&obj_ty, expr, "method call")?;
        }
        Ok(PhpType::Int)
    }

    /// Infers the type of a nullsafe method call expression (`$obj?->method(...)`).
    ///
    /// Returns `PhpType::Void` for invalid receivers. For valid nullable object
    /// unions, returns a union of the method's return type with `void`.
    pub(crate) fn infer_nullsafe_method_call_type(
        &mut self,
        object: &Expr,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let obj_ty = self.infer_type(object, env)?;
        let Some((class_name, nullable)) =
            self.nullsafe_object_receiver(&obj_ty, expr, "method call")?
        else {
            return Ok(PhpType::Void);
        };
        let return_ty = if self.interfaces.contains_key(&class_name) {
            self.infer_method_call_on_interface_type(&class_name, method, args, expr, env)?
        } else {
            self.infer_method_call_on_class_type(&class_name, method, args, expr, env)?
        };
        if nullable {
            Ok(self.normalize_union_type(vec![return_ty, PhpType::Void]))
        } else {
            Ok(return_ty)
        }
    }

    /// Infers the type of a method call on an interface type.
    ///
    /// Looks up the method in the interface schema, validates arguments via
    /// `normalize_named_call_args` and `check_known_callable_call`, and
    /// returns the declared return type.
    pub(crate) fn infer_method_call_on_interface_type(
        &mut self,
        interface_name: &str,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let method_key = php_symbol_key(method);
        let sig = self
            .interfaces
            .get(interface_name)
            .and_then(|interface_info| interface_info.methods.get(&method_key))
            .cloned()
            .ok_or_else(|| {
                CompileError::new(
                    expr.span,
                    &format!("Undefined method: {}::{}", interface_name, method),
                )
            })?;
        let normalized_args = self.normalize_named_call_args(
            &sig,
            args,
            expr.span,
            &format!("Method {}::{}", interface_name, method),
            env,
        )?;
        self.check_known_callable_call(
            &sig,
            &normalized_args,
            expr.span,
            env,
            &format!("Method {}::{}", interface_name, method),
        )?;
        Ok(sig.return_type)
    }

    /// Infers the type of a method call on a class type.
    ///
    /// Looks up the method in the class schema, checks deprecation warnings,
    /// validates visibility, normalizes named arguments, validates the
    /// callable signature, and updates the method's parameter types from
    /// argument types (for local type inference). Handles `__call` magic
    /// methods and falls back to `PhpType::Int`.
    pub(crate) fn infer_method_call_on_class_type(
        &mut self,
        class_name: &str,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        self.infer_method_call_on_class_type_with_options(
            class_name,
            method,
            args,
            expr,
            env,
            false,
        )
    }

    /// Infers a class method call for descriptor-backed callback paths that can
    /// preserve by-reference spread arguments through runtime invoker metadata.
    pub(crate) fn infer_method_call_on_class_type_allowing_by_ref_spread(
        &mut self,
        class_name: &str,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        self.infer_method_call_on_class_type_with_options(
            class_name,
            method,
            args,
            expr,
            env,
            true,
        )
    }

    /// Shared implementation for class method call inference.
    fn infer_method_call_on_class_type_with_options(
        &mut self,
        class_name: &str,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
        allow_by_ref_spread: bool,
    ) -> Result<PhpType, CompileError> {
        let method_key = php_symbol_key(method);
        let mut normalized_args = args.to_vec();
        let mut magic_return_ty = None;
        let mut magic_original_args = None;
        if let Some(class_info) = self.classes.get(class_name) {
            if let Some(sig) = class_info.methods.get(&method_key) {
                if let Some(reason) = sig.deprecation.clone() {
                    let message = if reason.is_empty() {
                        format!("Call to deprecated method: {}::{}()", class_name, method)
                    } else {
                        format!(
                            "Call to deprecated method: {}::{}() — {}",
                            class_name, method, reason
                        )
                    };
                    self.warnings
                        .push(crate::errors::CompileWarning::new(expr.span, &message));
                }
                if let Some(visibility) = class_info.method_visibilities.get(&method_key) {
                    let declaring_class = class_info
                        .method_declaring_classes
                        .get(&method_key)
                        .map(String::as_str)
                        .unwrap_or(class_name);
                    if !self.can_access_member(declaring_class, visibility) {
                        return Err(CompileError::new(
                            expr.span,
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
                    Self::declared_method_param_flags(class_info, &method_key, false);
                let mut effective_sig =
                    Self::callable_sig_for_declared_params(sig, &declared_flags);
                if method_key == "__call" {
                    Self::relax_magic_call_validation_sig(&mut effective_sig);
                }
                normalized_args = self.normalize_named_call_args(
                    &effective_sig,
                    args,
                    expr.span,
                    &format!("Method {}::{}", class_name, method),
                    env,
                )?;
                if allow_by_ref_spread {
                    self.check_known_callable_call_allowing_by_ref_spread(
                        &effective_sig,
                        &normalized_args,
                        expr.span,
                        env,
                        &format!("Method {}::{}", class_name, method),
                    )?;
                } else {
                    self.check_known_callable_call(
                        &effective_sig,
                        &normalized_args,
                        expr.span,
                        env,
                        &format!("Method {}::{}", class_name, method),
                    )?;
                }
            } else if let Some(sig) = class_info.methods.get("__call") {
                let magic_args = Self::magic_call_args(method, args, expr.span);
                let declared_flags =
                    Self::declared_method_param_flags(class_info, "__call", false);
                let mut effective_sig =
                    Self::callable_sig_for_declared_params(sig, &declared_flags);
                Self::relax_magic_call_validation_sig(&mut effective_sig);
                normalized_args = self.normalize_named_call_args(
                    &effective_sig,
                    &magic_args,
                    expr.span,
                    &format!("Method {}::__call", class_name),
                    env,
                )?;
                if allow_by_ref_spread {
                    self.check_known_callable_call_allowing_by_ref_spread(
                        &effective_sig,
                        &normalized_args,
                        expr.span,
                        env,
                        &format!("Method {}::__call", class_name),
                    )?;
                } else {
                    self.check_known_callable_call(
                        &effective_sig,
                        &normalized_args,
                        expr.span,
                        env,
                        &format!("Method {}::__call", class_name),
                    )?;
                }
                magic_return_ty = Some(effective_sig.return_type.clone());
                magic_original_args = Some(args.to_vec());
            } else {
                return Err(CompileError::new(
                    expr.span,
                    &format!("Undefined method: {}::{}", class_name, method),
                ));
            }
        }
        if let Some(return_ty) = magic_return_ty {
            if let Some(args) = magic_original_args {
                self.specialize_magic_call_signature(class_name, &args, env)?;
            }
            return Ok(return_ty);
        }
        let mut arg_types = Vec::new();
        for arg in &normalized_args {
            arg_types.push(self.infer_type(arg, env)?);
        }

        let impl_class_name = self
            .classes
            .get(class_name)
            .and_then(|class_info| class_info.method_impl_classes.get(&method_key))
            .cloned()
            .unwrap_or_else(|| class_name.to_string());
        let declared_flags = self
            .classes
            .get(&impl_class_name)
            .map(|class_info| Self::declared_method_param_flags(class_info, &method_key, false))
            .unwrap_or_default();
        if let Some(class_info) = self.classes.get_mut(&impl_class_name) {
            if let Some(sig) = class_info.methods.get_mut(&method_key) {
                let regular_param_count = if sig.variadic.is_some() {
                    sig.params.len().saturating_sub(1)
                } else {
                    sig.params.len()
                };
                for (i, arg_ty) in arg_types.iter().enumerate() {
                    if i < regular_param_count
                        && !declared_flags.get(i).copied().unwrap_or(false)
                        && !matches!(*arg_ty, PhpType::Void | PhpType::Never | PhpType::Callable)
                    {
                        let key = (format!("{}::{}", impl_class_name, method_key), i);
                        let seen = self.param_specialization_seen.contains(&key);
                        if sig.params[i].1 == PhpType::Int && !seen {
                            self.param_specialization_seen.insert(key);
                            sig.params[i].1 = arg_ty.clone();
                        } else {
                            sig.params[i].1 = Self::union_param_type(&sig.params[i].1, arg_ty);
                        }
                    }
                }
                if method_variadic_tail_needs_iterable(
                    &normalized_args,
                    sig,
                    regular_param_count,
                    env,
                ) {
                    if let Some((_, variadic_ty)) = sig.params.last_mut() {
                        *variadic_ty = PhpType::Iterable;
                    }
                } else if sig.variadic.is_some() && arg_types.len() > regular_param_count {
                    let mut elem_ty = arg_types[regular_param_count].clone();
                    for arg_ty in arg_types.iter().skip(regular_param_count + 1) {
                        elem_ty = wider_type_syntactic(&elem_ty, arg_ty);
                    }
                    if let Some((_, PhpType::Array(existing_elem_ty))) = sig.params.last_mut() {
                        **existing_elem_ty =
                            wider_type_syntactic(existing_elem_ty.as_ref(), &elem_ty);
                    }
                }
                return Ok(sig.return_type.clone());
            }
        }
        Ok(PhpType::Int)
    }

    /// Builds synthetic `__call` arguments: `[method_name, [args...]]`.
    ///
    /// Constructs a `StringLiteral` for the method name and an `ArrayLiteral`
    /// of the original arguments, used when forwarding to `__call`.
    fn magic_call_args(method: &str, args: &[Expr], span: crate::span::Span) -> Vec<Expr> {
        vec![
            Expr::new(ExprKind::StringLiteral(method.to_string()), span),
            Expr::new(ExprKind::ArrayLiteral(args.to_vec()), span),
        ]
    }

    /// Specializes `__call`'s second parameter (the args array) type based on
    /// the actual call arguments' inferred types.
    ///
    /// Merges all argument types into an element type, then updates the
    /// `__call` signature's params[1] (the array parameter) accordingly,
    /// respecting `declared_flags` and avoiding widening to `Mixed` when
    /// the declared type is already `Mixed`.
    fn specialize_magic_call_signature(
        &mut self,
        class_name: &str,
        args: &[Expr],
        env: &TypeEnv,
    ) -> Result<(), CompileError> {
        let mut elem_ty = PhpType::Never;
        for arg in args {
            let arg_ty = self.infer_type(arg, env)?;
            elem_ty = Self::merge_magic_call_arg_type(elem_ty, arg_ty);
        }
        let args_array_ty = PhpType::Array(Box::new(elem_ty.clone()));
        let impl_class_name = self
            .classes
            .get(class_name)
            .and_then(|class_info| class_info.method_impl_classes.get("__call"))
            .cloned()
            .unwrap_or_else(|| class_name.to_string());
        let declared_flags = self
            .classes
            .get(&impl_class_name)
            .map(|class_info| Self::declared_method_param_flags(class_info, "__call", false))
            .unwrap_or_default();
        if let Some(sig) = self
            .classes
            .get_mut(&impl_class_name)
            .and_then(|class_info| class_info.methods.get_mut("__call"))
        {
            if !sig.params.is_empty() {
                sig.params[0].1 = PhpType::Str;
            }
            if sig.params.len() > 1 {
                let declared_array_param = declared_flags.get(1).copied().unwrap_or(false);
                sig.params[1].1 = match &sig.params[1].1 {
                    PhpType::Array(existing)
                        if declared_array_param
                            && matches!(existing.as_ref(), PhpType::Mixed)
                            && !matches!(elem_ty, PhpType::Mixed) =>
                    {
                        args_array_ty
                    }
                    PhpType::Array(existing) => PhpType::Array(Box::new(
                        Self::merge_magic_call_arg_type(*existing.clone(), elem_ty.clone()),
                    )),
                    PhpType::Int => args_array_ty,
                    _ => sig.params[1].1.clone(),
                };
            }
        }
        Ok(())
    }

    /// Merges two types for `__call` argument type inference.
    ///
    /// Returns `right` when `left` is `Never`, `left` when `right` is `Never`,
    /// `left` when equal, and `PhpType::Mixed` otherwise. Used to compute the
    /// element type of the synthetic args array.
    fn merge_magic_call_arg_type(left: PhpType, right: PhpType) -> PhpType {
        if left == right {
            return left;
        }
        if matches!(left, PhpType::Never) {
            return right;
        }
        if matches!(right, PhpType::Never) {
            return left;
        }
        PhpType::Mixed
    }

    /// Relaxes a `__call` signature for validation-only use.
    ///
    /// Sets the first parameter to `PhpType::Str` and the second to
    /// `PhpType::Array(PhpType::Mixed)`, bypassing strict type checking so
    /// arbitrary arguments can be forwarded without false validation errors.
    fn relax_magic_call_validation_sig(sig: &mut crate::types::FunctionSig) {
        if let Some(param) = sig.params.get_mut(0) {
            param.1 = PhpType::Str;
        }
        if let Some(param) = sig.params.get_mut(1) {
            param.1 = PhpType::Array(Box::new(PhpType::Mixed));
        }
    }

    /// Infers the type of a static method call expression (`Foo::method()`, `self::`, `parent::`, `static::`).
    ///
    /// Resolves the receiver to a class name, checks deprecation and visibility,
    /// validates arguments via `normalize_named_call_args` and `check_known_callable_call`,
    /// and updates parameter types from argument types for local type inference.
    /// Handles enum static calls, `parent::`/`self::` forwarding to instance methods,
    /// and falls back to `PhpType::Int`.
    pub(crate) fn infer_static_method_call_type(
        &mut self,
        receiver: &StaticReceiver,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        self.infer_static_method_call_type_with_options(receiver, method, args, expr, env, false)
    }

    /// Infers a static method call for descriptor-backed callback paths that can
    /// preserve by-reference spread arguments through runtime invoker metadata.
    pub(crate) fn infer_static_method_call_type_allowing_by_ref_spread(
        &mut self,
        receiver: &StaticReceiver,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        self.infer_static_method_call_type_with_options(receiver, method, args, expr, env, true)
    }

    /// Shared implementation for static method call inference.
    fn infer_static_method_call_type_with_options(
        &mut self,
        receiver: &StaticReceiver,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
        allow_by_ref_spread: bool,
    ) -> Result<PhpType, CompileError> {
        let parent_call = matches!(receiver, StaticReceiver::Parent);
        let self_call = matches!(receiver, StaticReceiver::Self_);
        let resolved_class_name = match receiver {
            StaticReceiver::Named(class_name) => class_name.as_str().to_string(),
            StaticReceiver::Self_ => self.current_class.as_ref().cloned().ok_or_else(|| {
                CompileError::new(expr.span, "Cannot use self:: outside class method scope")
            })?,
            StaticReceiver::Static => self.current_class.as_ref().cloned().ok_or_else(|| {
                CompileError::new(expr.span, "Cannot use static:: outside class method scope")
            })?,
            StaticReceiver::Parent => {
                let current_class = self.current_class.as_ref().ok_or_else(|| {
                    CompileError::new(expr.span, "Cannot use parent:: outside class method scope")
                })?;
                let current_info = self.classes.get(current_class).ok_or_else(|| {
                    CompileError::new(expr.span, &format!("Undefined class: {}", current_class))
                })?;
                current_info.parent.as_ref().cloned().ok_or_else(|| {
                    CompileError::new(
                        expr.span,
                        &format!("Class {} has no parent class", current_class),
                    )
                })?
            }
        };
        let class_name = resolved_class_name.as_str();
        if let Some(enum_info) = self.enums.get(class_name).cloned() {
            return self
                .check_enum_static_call(&enum_info, class_name, method, args, env, expr.span);
        }
        let normalized_args: Vec<Expr>;
        if let Some(class_info) = self.classes.get(class_name) {
            if let Some(sig) = class_info.static_methods.get(method) {
                if let Some(reason) = sig.deprecation.clone() {
                    let message = if reason.is_empty() {
                        format!("Call to deprecated static method: {}::{}()", class_name, method)
                    } else {
                        format!(
                            "Call to deprecated static method: {}::{}() — {}",
                            class_name, method, reason
                        )
                    };
                    self.warnings
                        .push(crate::errors::CompileWarning::new(expr.span, &message));
                }
                if let Some(visibility) = class_info.static_method_visibilities.get(method) {
                    let declaring_class = class_info
                        .static_method_declaring_classes
                        .get(method)
                        .map(String::as_str)
                        .unwrap_or(class_name);
                    if !self.can_access_member(declaring_class, visibility) {
                        return Err(CompileError::new(
                            expr.span,
                            &format!(
                                "Cannot access {} method: {}::{}",
                                Self::visibility_label(visibility),
                                class_name,
                                method
                            ),
                        ));
                    }
                }
                let declared_flags = Self::declared_method_param_flags(class_info, method, true);
                let effective_sig = Self::callable_sig_for_declared_params(sig, &declared_flags);
                normalized_args = self.normalize_named_call_args(
                    &effective_sig,
                    args,
                    expr.span,
                    &format!("Static method {}::{}", class_name, method),
                    env,
                )?;
                if allow_by_ref_spread {
                    self.check_known_callable_call_allowing_by_ref_spread(
                        &effective_sig,
                        &normalized_args,
                        expr.span,
                        env,
                        &format!("Static method {}::{}", class_name, method),
                    )?;
                } else {
                    self.check_known_callable_call(
                        &effective_sig,
                        &normalized_args,
                        expr.span,
                        env,
                        &format!("Static method {}::{}", class_name, method),
                    )?;
                }
            } else if parent_call || self_call {
                if self.current_method_is_static {
                    return Err(CompileError::new(
                        expr.span,
                        if parent_call {
                            "Cannot call parent instance method from a static method"
                        } else {
                            "Cannot call self instance method from a static method"
                        },
                    ));
                }
                let sig = class_info.methods.get(method).ok_or_else(|| {
                    CompileError::new(
                        expr.span,
                        &format!("Undefined method: {}::{}", class_name, method),
                    )
                })?;
                if let Some(visibility) = class_info.method_visibilities.get(method) {
                    let declaring_class = class_info
                        .method_declaring_classes
                        .get(method)
                        .map(String::as_str)
                        .unwrap_or(class_name);
                    if !self.can_access_member(declaring_class, visibility) {
                        return Err(CompileError::new(
                            expr.span,
                            &format!(
                                "Cannot access {} method: {}::{}",
                                Self::visibility_label(visibility),
                                class_name,
                                method
                            ),
                        ));
                    }
                }
                let declared_flags = Self::declared_method_param_flags(class_info, method, false);
                let effective_sig = Self::callable_sig_for_declared_params(sig, &declared_flags);
                normalized_args = self.normalize_named_call_args(
                    &effective_sig,
                    args,
                    expr.span,
                    &format!(
                        "{} method {}::{}",
                        if parent_call { "Parent" } else { "Self" },
                        class_name,
                        method
                    ),
                    env,
                )?;
                if allow_by_ref_spread {
                    self.check_known_callable_call_allowing_by_ref_spread(
                        &effective_sig,
                        &normalized_args,
                        expr.span,
                        env,
                        &format!(
                            "{} method {}::{}",
                            if parent_call { "Parent" } else { "Self" },
                            class_name,
                            method
                        ),
                    )?;
                } else {
                    self.check_known_callable_call(
                        &effective_sig,
                        &normalized_args,
                        expr.span,
                        env,
                        &format!(
                            "{} method {}::{}",
                            if parent_call { "Parent" } else { "Self" },
                            class_name,
                            method
                        ),
                    )?;
                }
            } else if class_info.methods.contains_key(method) {
                return Err(CompileError::new(
                    expr.span,
                    &format!(
                        "Cannot call instance method statically: {}::{}",
                        class_name, method
                    ),
                ));
            } else {
                return Err(CompileError::new(
                    expr.span,
                    &format!("Undefined method: {}::{}", class_name, method),
                ));
            }
        } else {
            return Err(CompileError::new(
                expr.span,
                &format!("Undefined class: {}", class_name),
            ));
        }
        let mut arg_types = Vec::new();
        for arg in &normalized_args {
            arg_types.push(self.infer_type(arg, env)?);
        }

        let direct_impl_class_name = if parent_call || self_call {
            self.classes
                .get(class_name)
                .and_then(|class_info| class_info.method_impl_classes.get(method))
                .cloned()
                .unwrap_or_else(|| class_name.to_string())
        } else {
            String::new()
        };
        let static_declared_flags = self
            .classes
            .get(class_name)
            .map(|class_info| Self::declared_method_param_flags(class_info, method, true))
            .unwrap_or_default();
        if let Some(class_info) = self.classes.get_mut(class_name) {
            if let Some(sig) = class_info.static_methods.get_mut(method) {
                let regular_param_count = if sig.variadic.is_some() {
                    sig.params.len().saturating_sub(1)
                } else {
                    sig.params.len()
                };
                for (i, arg_ty) in arg_types.iter().enumerate() {
                    if i < regular_param_count
                        && !static_declared_flags.get(i).copied().unwrap_or(false)
                        && !matches!(*arg_ty, PhpType::Void | PhpType::Never | PhpType::Callable)
                    {
                        let key = (format!("static:{}::{}", class_name, method), i);
                        let seen = self.param_specialization_seen.contains(&key);
                        if sig.params[i].1 == PhpType::Int && !seen {
                            self.param_specialization_seen.insert(key);
                            sig.params[i].1 = arg_ty.clone();
                        } else {
                            sig.params[i].1 = Self::union_param_type(&sig.params[i].1, arg_ty);
                        }
                    }
                }
                if method_variadic_tail_needs_iterable(
                    &normalized_args,
                    sig,
                    regular_param_count,
                    env,
                ) {
                    if let Some((_, variadic_ty)) = sig.params.last_mut() {
                        *variadic_ty = PhpType::Iterable;
                    }
                } else if sig.variadic.is_some() && arg_types.len() > regular_param_count {
                    let mut elem_ty = arg_types[regular_param_count].clone();
                    for arg_ty in arg_types.iter().skip(regular_param_count + 1) {
                        elem_ty = wider_type_syntactic(&elem_ty, arg_ty);
                    }
                    if let Some((_, PhpType::Array(existing_elem_ty))) = sig.params.last_mut() {
                        **existing_elem_ty =
                            wider_type_syntactic(existing_elem_ty.as_ref(), &elem_ty);
                    }
                }
                return Ok(sig.return_type.clone());
            }
        }
        if parent_call || self_call {
            let instance_declared_flags = self
                .classes
                .get(&direct_impl_class_name)
                .map(|class_info| Self::declared_method_param_flags(class_info, method, false))
                .unwrap_or_default();
            if let Some(sig) = self
                .classes
                .get_mut(&direct_impl_class_name)
                .and_then(|class_info| class_info.methods.get_mut(method))
            {
                let regular_param_count = if sig.variadic.is_some() {
                    sig.params.len().saturating_sub(1)
                } else {
                    sig.params.len()
                };
                for (i, arg_ty) in arg_types.iter().enumerate() {
                    if i < regular_param_count
                        && !instance_declared_flags.get(i).copied().unwrap_or(false)
                        && !matches!(*arg_ty, PhpType::Void | PhpType::Never | PhpType::Callable)
                    {
                        let key = (format!("{}::{}", direct_impl_class_name, method), i);
                        let seen = self.param_specialization_seen.contains(&key);
                        if sig.params[i].1 == PhpType::Int && !seen {
                            self.param_specialization_seen.insert(key);
                            sig.params[i].1 = arg_ty.clone();
                        } else {
                            sig.params[i].1 = Self::union_param_type(&sig.params[i].1, arg_ty);
                        }
                    }
                }
                if sig.variadic.is_some() && arg_types.len() > regular_param_count {
                    let mut elem_ty = arg_types[regular_param_count].clone();
                    for arg_ty in arg_types.iter().skip(regular_param_count + 1) {
                        elem_ty = wider_type_syntactic(&elem_ty, arg_ty);
                    }
                    if let Some((_, PhpType::Array(existing_elem_ty))) = sig.params.last_mut() {
                        **existing_elem_ty =
                            wider_type_syntactic(existing_elem_ty.as_ref(), &elem_ty);
                    }
                }
                return Ok(sig.return_type.clone());
            }
        }
        Ok(PhpType::Int)
    }
}

/// Returns true when a method variadic parameter must keep runtime key information.
fn method_variadic_tail_needs_iterable(
    args: &[Expr],
    sig: &FunctionSig,
    regular_param_count: usize,
    env: &TypeEnv,
) -> bool {
    if sig.variadic.is_none() {
        return false;
    }

    if args.iter().any(|arg| {
        matches!(
            &arg.kind,
            ExprKind::Spread(inner) if spread_source_keeps_runtime_keys(inner, env)
        )
    }) {
        return true;
    }

    args.iter().any(|arg| {
        matches!(
            &arg.kind,
            ExprKind::NamedArg { name, .. }
                if !sig
                    .params
                    .iter()
                    .take(regular_param_count)
                    .any(|(param_name, _)| param_name == name)
        )
    })
}

/// Returns true when a spread source can carry string keys into a variadic method tail.
fn spread_source_keeps_runtime_keys(expr: &Expr, env: &TypeEnv) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => matches!(
            env.get(name),
            Some(PhpType::AssocArray { .. } | PhpType::Iterable)
        ),
        ExprKind::ArrayLiteralAssoc(_) => true,
        _ => matches!(
            crate::types::checker::infer_expr_type_syntactic(expr),
            PhpType::AssocArray { .. } | PhpType::Iterable
        ),
    }
}
