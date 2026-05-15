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
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
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
        if is_reflection_owner_class(&class_name) {
            self.validate_reflection_owner_constructor(&class_name, args, expr, env)?;
            return Ok(PhpType::Object(class_name));
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

    fn validate_reflection_owner_constructor(
        &mut self,
        class_name: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<(), CompileError> {
        let sig = self
            .classes
            .get(class_name)
            .and_then(|class_info| class_info.methods.get("__construct"))
            .cloned()
            .expect("builtin reflection class is missing its constructor signature");
        let normalized_args = self.normalize_named_call_args(
            &sig,
            args,
            expr.span,
            &format!("Constructor '{}::__construct'", class_name),
        )?;
        self.check_known_callable_call(
            &sig,
            &normalized_args,
            expr.span,
            env,
            &format!("Constructor '{}::__construct'", class_name),
        )?;

        let reflected_class =
            self.reflection_class_literal_arg(class_name, &normalized_args[0], env)?;
        match class_name {
            "ReflectionClass" => self.validate_reflection_class_attrs(&reflected_class, expr),
            "ReflectionMethod" => {
                let method_name = self.reflection_string_literal_arg(
                    class_name,
                    "method name",
                    normalized_args.get(1),
                    env,
                )?;
                self.validate_reflection_method_attrs(&reflected_class, &method_name, expr)
            }
            "ReflectionProperty" => {
                let property_name = self.reflection_string_literal_arg(
                    class_name,
                    "property name",
                    normalized_args.get(1),
                    env,
                )?;
                self.validate_reflection_property_attrs(&reflected_class, &property_name, expr)
            }
            _ => Ok(()),
        }
    }

    fn reflection_class_literal_arg(
        &mut self,
        reflection_type: &str,
        arg: &Expr,
        env: &TypeEnv,
    ) -> Result<String, CompileError> {
        let arg_ty = self.infer_type(arg, env)?;
        if !matches!(arg_ty, PhpType::Str) {
            return Err(CompileError::new(
                arg.span,
                &format!(
                    "{}::__construct() first argument must be a string class name",
                    reflection_type
                ),
            ));
        }
        let raw_class_name = match &arg.kind {
            ExprKind::StringLiteral(class_name) => class_name.clone(),
            ExprKind::ClassConstant { receiver } => {
                self.resolve_reflection_class_constant(receiver, arg.span)?
            }
            _ => {
                return Err(CompileError::new(
                    arg.span,
                    &format!(
                        "{}::__construct() requires a string literal class name (dynamic lookup is not yet supported)",
                        reflection_type
                    ),
                ));
            }
        };
        self.resolve_reflection_class_name(&raw_class_name)
            .map(str::to_string)
            .ok_or_else(|| {
                CompileError::new(
                    arg.span,
                    &format!(
                        "{}::__construct(): undefined class '{}'",
                        reflection_type, raw_class_name
                    ),
                )
            })
    }

    fn reflection_string_literal_arg(
        &mut self,
        reflection_type: &str,
        label: &str,
        arg: Option<&Expr>,
        env: &TypeEnv,
    ) -> Result<String, CompileError> {
        let arg = arg.expect("reflection constructor arity was validated");
        let arg_ty = self.infer_type(arg, env)?;
        if !matches!(arg_ty, PhpType::Str) {
            return Err(CompileError::new(
                arg.span,
                &format!(
                    "{}::__construct() {} argument must be a string",
                    reflection_type, label
                ),
            ));
        }
        match &arg.kind {
            ExprKind::StringLiteral(value) => Ok(value.clone()),
            _ => Err(CompileError::new(
                arg.span,
                &format!(
                    "{}::__construct() requires a string literal {} (dynamic lookup is not yet supported)",
                    reflection_type, label
                ),
            )),
        }
    }

    fn validate_reflection_class_attrs(
        &self,
        class_name: &str,
        expr: &Expr,
    ) -> Result<(), CompileError> {
        let Some(class_info) = self.classes.get(class_name) else {
            return Err(CompileError::new(
                expr.span,
                &format!("ReflectionClass::__construct(): undefined class '{}'", class_name),
            ));
        };
        if attributes_have_unsupported_args(&class_info.attribute_names, &class_info.attribute_args)
        {
            return Err(CompileError::new(
                expr.span,
                "ReflectionClass::getAttributes(): class has attribute argument metadata that is not supported yet",
            ));
        }
        Ok(())
    }

    fn validate_reflection_method_attrs(
        &self,
        class_name: &str,
        method_name: &str,
        expr: &Expr,
    ) -> Result<(), CompileError> {
        let Some(class_info) = self.classes.get(class_name) else {
            return Err(CompileError::new(
                expr.span,
                &format!("ReflectionMethod::__construct(): undefined class '{}'", class_name),
            ));
        };
        let method_key = php_symbol_key(method_name);
        if !class_info.methods.contains_key(&method_key)
            && !class_info.static_methods.contains_key(&method_key)
        {
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "ReflectionMethod::__construct(): undefined method '{}::{}'",
                    class_name, method_name
                ),
            ));
        }
        let empty_names = Vec::new();
        let empty_args = Vec::new();
        let names = class_info
            .method_attribute_names
            .get(&method_key)
            .unwrap_or(&empty_names);
        let args = class_info
            .method_attribute_args
            .get(&method_key)
            .unwrap_or(&empty_args);
        if attributes_have_unsupported_args(names, args) {
            return Err(CompileError::new(
                expr.span,
                "ReflectionMethod::getAttributes(): method has attribute argument metadata that is not supported yet",
            ));
        }
        Ok(())
    }

    fn validate_reflection_property_attrs(
        &self,
        class_name: &str,
        property_name: &str,
        expr: &Expr,
    ) -> Result<(), CompileError> {
        let Some(class_info) = self.classes.get(class_name) else {
            return Err(CompileError::new(
                expr.span,
                &format!("ReflectionProperty::__construct(): undefined class '{}'", class_name),
            ));
        };
        if !class_info.properties.iter().any(|(name, _)| name == property_name)
            && !class_info
                .static_properties
                .iter()
                .any(|(name, _)| name == property_name)
        {
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "ReflectionProperty::__construct(): undefined property '{}::${}'",
                    class_name, property_name
                ),
            ));
        }
        let empty_names = Vec::new();
        let empty_args = Vec::new();
        let names = class_info
            .property_attribute_names
            .get(property_name)
            .unwrap_or(&empty_names);
        let args = class_info
            .property_attribute_args
            .get(property_name)
            .unwrap_or(&empty_args);
        if attributes_have_unsupported_args(names, args) {
            return Err(CompileError::new(
                expr.span,
                "ReflectionProperty::getAttributes(): property has attribute argument metadata that is not supported yet",
            ));
        }
        Ok(())
    }

    fn resolve_reflection_class_constant(
        &self,
        receiver: &StaticReceiver,
        span: crate::span::Span,
    ) -> Result<String, CompileError> {
        match receiver {
            StaticReceiver::Named(name) => Ok(name.as_canonical()),
            StaticReceiver::Self_ | StaticReceiver::Static => self
                .current_class
                .clone()
                .ok_or_else(|| CompileError::new(span, "Cannot use self::class outside a class context")),
            StaticReceiver::Parent => {
                let current = self.current_class.as_ref().ok_or_else(|| {
                    CompileError::new(span, "Cannot use parent::class outside a class context")
                })?;
                self.classes
                    .get(current)
                    .and_then(|info| info.parent.clone())
                    .ok_or_else(|| {
                        CompileError::new(
                            span,
                            &format!("Class '{}' has no parent class", current),
                        )
                    })
            }
        }
    }

    fn resolve_reflection_class_name<'a>(&'a self, class_name: &str) -> Option<&'a str> {
        let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
        self.classes
            .keys()
            .find(|existing| php_symbol_key(existing) == class_key)
            .map(String::as_str)
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

fn is_reflection_owner_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "ReflectionClass" | "ReflectionMethod" | "ReflectionProperty"
    )
}

fn attributes_have_unsupported_args(
    names: &[String],
    args: &[Option<Vec<crate::types::AttrArgValue>>],
) -> bool {
    names.len() != args.len() || args.iter().any(Option::is_none)
}
