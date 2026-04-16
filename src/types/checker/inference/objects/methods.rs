use crate::errors::CompileError;
use crate::parser::ast::{Expr, StaticReceiver};
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;
use super::super::syntactic::wider_type_syntactic;

impl Checker {
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
            let mut normalized_args = args.to_vec();
            if let Some(class_info) = self.classes.get(class_name) {
                if let Some(sig) = class_info.methods.get(method) {
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
                    let declared_flags =
                        Self::declared_method_param_flags(class_info, method, false);
                    let effective_sig =
                        Self::callable_sig_for_declared_params(sig, &declared_flags);
                    normalized_args = self.normalize_named_call_args(
                        &effective_sig,
                        args,
                        expr.span,
                        &format!("Method {}::{}", class_name, method),
                    )?;
                    self.check_known_callable_call(
                        &effective_sig,
                        &normalized_args,
                        expr.span,
                        env,
                        &format!("Method {}::{}", class_name, method),
                    )?;
                } else {
                    return Err(CompileError::new(
                        expr.span,
                        &format!("Undefined method: {}::{}", class_name, method),
                    ));
                }
            }
            let mut arg_types = Vec::new();
            for arg in &normalized_args {
                arg_types.push(self.infer_type(arg, env)?);
            }

            let impl_class_name = self
                .classes
                .get(class_name)
                .and_then(|class_info| class_info.method_impl_classes.get(method))
                .cloned()
                .unwrap_or_else(|| class_name.clone());
            let declared_flags = self
                .classes
                .get(&impl_class_name)
                .map(|class_info| Self::declared_method_param_flags(class_info, method, false))
                .unwrap_or_default();
            if let Some(class_info) = self.classes.get_mut(&impl_class_name) {
                if let Some(sig) = class_info.methods.get_mut(method) {
                    let regular_param_count = if sig.variadic.is_some() {
                        sig.params.len().saturating_sub(1)
                    } else {
                        sig.params.len()
                    };
                    for (i, arg_ty) in arg_types.iter().enumerate() {
                        if i < regular_param_count
                            && !declared_flags.get(i).copied().unwrap_or(false)
                            && sig.params[i].1 == PhpType::Int
                            && *arg_ty != PhpType::Int
                        {
                            sig.params[i].1 = arg_ty.clone();
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
        }
        Ok(PhpType::Int)
    }

    pub(crate) fn infer_static_method_call_type(
        &mut self,
        receiver: &StaticReceiver,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
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
                )?;
                self.check_known_callable_call(
                    &effective_sig,
                    &normalized_args,
                    expr.span,
                    env,
                    &format!("Static method {}::{}", class_name, method),
                )?;
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
                )?;
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
                        && sig.params[i].1 == PhpType::Int
                        && *arg_ty != PhpType::Int
                    {
                        sig.params[i].1 = arg_ty.clone();
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
                        && sig.params[i].1 == PhpType::Int
                        && *arg_ty != PhpType::Int
                    {
                        sig.params[i].1 = arg_ty.clone();
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
