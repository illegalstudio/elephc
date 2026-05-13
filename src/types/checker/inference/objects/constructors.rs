//! Purpose:
//! Infers object constructors expression types.
//! Validates class, method, constructor, property, and magic-access contracts against schema metadata.
//!
//! Called from:
//! - `crate::types::checker::inference::objects`
//!
//! Key details:
//! - Object inference depends on flattened class metadata, visibility, inheritance, and declared property types.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{fibers, PhpType, TypeEnv};

use super::super::super::Checker;

impl Checker {
    pub(crate) fn infer_new_object_type(
        &mut self,
        class_name: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let class_name = class_name.to_string();
        if self.enums.contains_key(class_name.as_str()) {
            return Err(CompileError::new(
                expr.span,
                &format!("Cannot instantiate enum: {}", class_name),
            ));
        }
        if self.interfaces.contains_key(class_name.as_str()) {
            return Err(CompileError::new(
                expr.span,
                &format!("Cannot instantiate interface: {}", class_name),
            ));
        }
        if !self.classes.contains_key(class_name.as_str()) {
            return Err(CompileError::new(
                expr.span,
                &format!("Undefined class: {}", class_name),
            ));
        }
        if class_name == "Fiber" {
            self.validate_fiber_constructor_args(args, expr, env)?;
        }
        if let Some(class_info) = self.classes.get(class_name.as_str()) {
            if class_info.is_abstract {
                return Err(CompileError::new(
                    expr.span,
                    &format!("Cannot instantiate abstract class: {}", class_name),
                ));
            }
            if let Some(sig) = class_info.methods.get("__construct") {
                if let Some(visibility) = class_info.method_visibilities.get("__construct") {
                    let declaring_class = class_info
                        .method_declaring_classes
                        .get("__construct")
                        .map(String::as_str)
                        .unwrap_or(class_name.as_str());
                    if !self.can_access_member(declaring_class, visibility) {
                        return Err(CompileError::new(
                            expr.span,
                            &format!(
                                "Cannot access {} constructor: {}::__construct",
                                Self::visibility_label(visibility),
                                class_name
                            ),
                        ));
                    }
                }
                let declared_flags =
                    Self::declared_method_param_flags(class_info, "__construct", false);
                let effective_sig = Self::callable_sig_for_declared_params(sig, &declared_flags);
                let param_to_prop = class_info.constructor_param_to_prop.clone();
                let normalized_args = self.normalize_named_call_args(
                    &effective_sig,
                    args,
                    expr.span,
                    &format!("Constructor '{}::__construct'", class_name),
                )?;
                self.check_known_callable_call(
                    &effective_sig,
                    &normalized_args,
                    expr.span,
                    env,
                    &format!("Constructor '{}::__construct'", class_name),
                )?;
                for (i, arg) in normalized_args.iter().enumerate() {
                    let arg_ty = self.infer_type(arg, env)?;
                    if param_to_prop.get(i).is_some_and(|mapped| mapped.is_some()) {
                        self.propagate_constructor_arg_type(class_name.as_str(), i, &arg_ty);
                    }
                }
                return Ok(PhpType::Object(class_name));
            } else if !args.is_empty() {
                return Err(CompileError::new(
                    expr.span,
                    &format!(
                        "Constructor '{}::__construct' expects 0 arguments, got {}",
                        class_name,
                        args.len()
                    ),
                ));
            }
        }
        let param_to_prop = self
            .classes
            .get(class_name.as_str())
            .map(|c| c.constructor_param_to_prop.clone())
            .unwrap_or_default();
        for (i, arg) in args.iter().enumerate() {
            let arg_ty = self.infer_type(arg, env)?;
            if param_to_prop.get(i).is_some_and(|mapped| mapped.is_some()) {
                self.propagate_constructor_arg_type(class_name.as_str(), i, &arg_ty);
            }
        }
        Ok(PhpType::Object(class_name))
    }

    fn validate_fiber_constructor_args(
        &mut self,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<(), CompileError> {
        let Some(callback) = args.first() else {
            return Ok(());
        };
        let Some(sig) = self.resolve_expr_callable_sig(callback, env)? else {
            return Err(CompileError::new(
                callback.span,
                "Fiber callback must be a closure or known first-class callable",
            ));
        };

        let visible_param_count = match &callback.kind {
            ExprKind::Closure {
                params,
                variadic,
                captures,
                ..
            } => {
                let visible_param_count =
                    fibers::visible_param_count(params.len(), variadic.is_some());
                let capture_types = captures
                    .iter()
                    .map(|name| {
                        (
                            name.clone(),
                            env.get(name).cloned().unwrap_or(PhpType::Mixed),
                        )
                    })
                    .collect::<Vec<_>>();
                fibers::validate_capture_slots(
                    &sig,
                    visible_param_count,
                    &capture_types,
                    callback.span,
                )?;
                visible_param_count
            }
            ExprKind::Variable(name) => {
                let capture_types = self
                    .callable_captures
                    .get(name)
                    .cloned()
                    .unwrap_or_default();
                let visible_param_count = sig.params.len();
                fibers::validate_capture_slots(
                    &sig,
                    visible_param_count,
                    &capture_types,
                    callback.span,
                )?;
                visible_param_count
            }
            ExprKind::FirstClassCallable(_) => sig.params.len(),
            _ => {
                return Err(CompileError::new(
                    callback.span,
                    "Fiber callback must be a closure or known first-class callable",
                ));
            }
        };

        fibers::validate_callback_signature(&sig, visible_param_count, expr.span)
    }

    pub(crate) fn infer_enum_case_type(
        &mut self,
        enum_name: &str,
        case_name: &str,
        expr: &Expr,
    ) -> Result<PhpType, CompileError> {
        let enum_name = enum_name.to_string();
        let enum_info = self.enums.get(enum_name.as_str()).ok_or_else(|| {
            CompileError::new(expr.span, &format!("Undefined enum: {}", enum_name))
        })?;
        if !enum_info.cases.iter().any(|case| case.name == *case_name) {
            return Err(CompileError::new(
                expr.span,
                &format!("Undefined enum case: {}::{}", enum_name, case_name),
            ));
        }
        Ok(PhpType::Object(enum_name))
    }
}
