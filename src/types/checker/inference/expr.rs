use crate::errors::CompileError;
use crate::parser::ast::{
    BinOp, CallableTarget, Expr, ExprKind, StaticReceiver, Stmt, StmtKind,
};
use crate::span::Span;
use crate::types::{
    merge_array_key_types, normalized_array_key_type, packed_type_size, PhpType, TypeEnv,
};

use super::super::Checker;
use super::syntactic::wider_type_syntactic;

impl Checker {
    pub(crate) fn infer_type_with_assignment_effects(
        &mut self,
        expr: &Expr,
        env: &mut TypeEnv,
    ) -> Result<PhpType, CompileError> {
        match &expr.kind {
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                ..
            } => {
                self.check_assignment_expression(
                    target,
                    value,
                    result_target.as_deref(),
                    prelude,
                    expr.span,
                    env,
                )
            }
            ExprKind::BinaryOp { left, op, right } => {
                self.infer_type_with_assignment_effects(left, env)?;
                if matches!(op, BinOp::And | BinOp::Or) {
                    let mut right_env = env.clone();
                    self.infer_type_with_assignment_effects(right, &mut right_env)?;
                    Ok(PhpType::Bool)
                } else {
                    self.infer_type_with_assignment_effects(right, env)?;
                    self.infer_type(expr, env)
                }
            }
            ExprKind::NullCoalesce { value, default } => {
                let value_ty = self.infer_type_with_assignment_effects(value, env)?;
                let default_ty = if value_ty == PhpType::Void {
                    self.infer_type_with_assignment_effects(default, env)?
                } else {
                    let mut default_env = env.clone();
                    self.infer_type_with_assignment_effects(default, &mut default_env)?
                };
                if Self::union_contains_void(&value_ty) {
                    Ok(wider_type_syntactic(
                        &self.strip_void_from_union(&value_ty),
                        &default_ty,
                    ))
                } else {
                    Ok(wider_type_syntactic(&value_ty, &default_ty))
                }
            }
            ExprKind::ShortTernary { value, default } => {
                let value_ty = self.infer_type_with_assignment_effects(value, env)?;
                let default_ty = if value_ty == PhpType::Void {
                    self.infer_type_with_assignment_effects(default, env)?
                } else {
                    let mut default_env = env.clone();
                    self.infer_type_with_assignment_effects(default, &mut default_env)?
                };
                Ok(wider_type_syntactic(&value_ty, &default_ty))
            }
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                self.infer_type_with_assignment_effects(condition, env)?;
                let mut then_env = env.clone();
                let then_ty = self.infer_type_with_assignment_effects(then_expr, &mut then_env)?;
                let mut else_env = env.clone();
                let else_ty = self.infer_type_with_assignment_effects(else_expr, &mut else_env)?;
                Ok(wider_type_syntactic(&then_ty, &else_ty))
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
                let mut result_ty = None;
                for (conditions, result) in arms {
                    let mut arm_env = env.clone();
                    for condition in conditions {
                        self.infer_type_with_assignment_effects(condition, &mut arm_env)?;
                    }
                    let arm_ty = self.infer_type_with_assignment_effects(result, &mut arm_env)?;
                    result_ty = Some(match result_ty {
                        Some(current) => wider_type_syntactic(&current, &arm_ty),
                        None => arm_ty,
                    });
                }
                if let Some(default) = default {
                    let mut default_env = env.clone();
                    let default_ty =
                        self.infer_type_with_assignment_effects(default, &mut default_env)?;
                    result_ty = Some(match result_ty {
                        Some(current) => wider_type_syntactic(&current, &default_ty),
                        None => default_ty,
                    });
                }
                Ok(result_ty.unwrap_or(PhpType::Void))
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
            ExprKind::FunctionCall { args, .. }
            | ExprKind::NewObject { args, .. }
            | ExprKind::StaticMethodCall { args, .. } => {
                for arg in args {
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                self.infer_type(expr, env)
            }
            ExprKind::ClosureCall { args, .. } => {
                for arg in args {
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                self.infer_type(expr, env)
            }
            ExprKind::ExprCall { callee, args } => {
                self.infer_type_with_assignment_effects(callee, env)?;
                for arg in args {
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                self.infer_type(expr, env)
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
            ExprKind::MethodCall { object, args, .. }
            | ExprKind::NullsafeMethodCall { object, args, .. } => {
                self.infer_type_with_assignment_effects(object, env)?;
                for arg in args {
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                self.infer_type(expr, env)
            }
            ExprKind::BufferNew { len, .. } => {
                self.infer_type_with_assignment_effects(len, env)?;
                self.infer_type(expr, env)
            }
            ExprKind::NewScopedObject { args, .. } => {
                for arg in args {
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                self.infer_type(expr, env)
            }
            _ => self.infer_type(expr, env),
        }
    }

    pub fn infer_type(&mut self, expr: &Expr, env: &TypeEnv) -> Result<PhpType, CompileError> {
        match &expr.kind {
            ExprKind::BoolLiteral(_) => Ok(PhpType::Bool),
            ExprKind::Null => Ok(PhpType::Void),
            ExprKind::StringLiteral(_) => Ok(PhpType::Str),
            ExprKind::IntLiteral(_) => Ok(PhpType::Int),
            ExprKind::FloatLiteral(_) => Ok(PhpType::Float),
            ExprKind::Variable(name) => env.get(name).cloned().ok_or_else(|| {
                CompileError::new(expr.span, &format!("Undefined variable: ${}", name))
            }),
            ExprKind::Negate(inner) => {
                let ty = self.infer_type(inner, env)?;
                match ty {
                    PhpType::Int => Ok(PhpType::Int),
                    PhpType::Float => Ok(PhpType::Float),
                    _ => Err(CompileError::new(
                        expr.span,
                        "Cannot negate a non-numeric value",
                    )),
                }
            }
            ExprKind::Not(inner) => {
                self.infer_type(inner, env)?;
                Ok(PhpType::Bool)
            }
            ExprKind::ErrorSuppress(inner) => self.infer_type(inner, env),
            ExprKind::Print(inner) => {
                self.infer_type(inner, env)?;
                Ok(PhpType::Int)
            }
            ExprKind::PreIncrement(name)
            | ExprKind::PostIncrement(name)
            | ExprKind::PreDecrement(name)
            | ExprKind::PostDecrement(name) => match env.get(name) {
                Some(PhpType::Int) | Some(PhpType::Bool) | Some(PhpType::Void) => Ok(PhpType::Int),
                Some(other) => Err(CompileError::new(
                    expr.span,
                    &format!("Cannot increment/decrement ${} of type {:?}", name, other),
                )),
                None => Err(CompileError::new(
                    expr.span,
                    &format!("Undefined variable: ${}", name),
                )),
            },
            ExprKind::ArrayLiteralAssoc(pairs) => {
                if pairs.is_empty() {
                    return Err(CompileError::new(
                        expr.span,
                        "Cannot infer type of empty associative array literal",
                    ));
                }
                let mut key_ty = normalized_array_key_type(
                    &pairs[0].0,
                    self.infer_type(&pairs[0].0, env)?,
                );
                let mut val_ty = self.infer_type(&pairs[0].1, env)?;
                for (k, v) in &pairs[1..] {
                    let kt = normalized_array_key_type(k, self.infer_type(k, env)?);
                    let vt = self.infer_type(v, env)?;
                    if kt != key_ty {
                        key_ty = merge_array_key_types(key_ty, kt);
                    }
                    if vt != val_ty {
                        val_ty = PhpType::Mixed;
                    }
                }
                Ok(PhpType::AssocArray {
                    key: Box::new(key_ty),
                    value: Box::new(val_ty),
                })
            }
            ExprKind::Match {
                subject,
                arms,
                default,
            } => {
                self.infer_type(subject, env)?;
                let mut result_ty = None;
                for (conditions, result) in arms {
                    for c in conditions {
                        self.infer_type(c, env)?;
                    }
                    let ty = self.infer_type(result, env)?;
                    if result_ty.is_none() {
                        result_ty = Some(ty);
                    }
                }
                if let Some(d) = default {
                    let ty = self.infer_type(d, env)?;
                    if result_ty.is_none() {
                        result_ty = Some(ty);
                    }
                }
                Ok(result_ty.unwrap_or(PhpType::Void))
            }
            ExprKind::ArrayLiteral(elems) => {
                if elems.is_empty() {
                    return Ok(PhpType::Array(Box::new(PhpType::Int)));
                }
                let mut elem_ty = self.infer_type(&elems[0], env)?;
                for elem in &elems[1..] {
                    let ty = self.infer_type(elem, env)?;
                    if ty != elem_ty {
                        if let Some(merged_ty) = self.merge_array_element_type(&elem_ty, &ty) {
                            elem_ty = merged_ty;
                            continue;
                        }
                        return Err(CompileError::new(
                            elem.span,
                            &format!(
                                "Array element type mismatch: expected {:?}, got {:?}",
                                elem_ty, ty
                            ),
                        ));
                    }
                }
                Ok(PhpType::Array(Box::new(elem_ty)))
            }
            ExprKind::ArrayAccess { array, index } => {
                let arr_ty = self.infer_type(array, env)?;
                let idx_ty = self.infer_type(index, env)?;
                match &arr_ty {
                    PhpType::Str => {
                        if idx_ty != PhpType::Int {
                            return Err(CompileError::new(
                                expr.span,
                                "String index must be integer",
                            ));
                        }
                        Ok(PhpType::Str)
                    }
                    PhpType::Array(elem_ty) => {
                        if idx_ty != PhpType::Int {
                            return Err(CompileError::new(
                                expr.span,
                                "Array index must be integer",
                            ));
                        }
                        Ok(*elem_ty.clone())
                    }
                    PhpType::AssocArray { value, .. } => {
                        // Assoc arrays accept string or int keys
                        Ok(*value.clone())
                    }
                    PhpType::Buffer(elem_ty) => {
                        if idx_ty != PhpType::Int {
                            return Err(CompileError::new(
                                expr.span,
                                "Buffer index must be integer",
                            ));
                        }
                        match elem_ty.as_ref() {
                            PhpType::Packed(name) => Ok(PhpType::Pointer(Some(name.clone()))),
                            _ => Ok(*elem_ty.clone()),
                        }
                    }
                    _ => Err(CompileError::new(expr.span, "Cannot index non-array")),
                }
            }
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                self.infer_type(condition, env)?;
                let then_ty = self.infer_type(then_expr, env)?;
                let else_ty = self.infer_type(else_expr, env)?;
                let result_ty = if then_ty == else_ty {
                    then_ty
                } else if then_ty == PhpType::Str || else_ty == PhpType::Str {
                    PhpType::Str
                } else if then_ty == PhpType::Float || else_ty == PhpType::Float {
                    PhpType::Float
                } else {
                    then_ty
                };
                Ok(result_ty)
            }
            ExprKind::ShortTernary { value, default } => {
                let value_ty = self.infer_type(value, env)?;
                let default_ty = self.infer_type(default, env)?;
                let result_ty = if value_ty == default_ty {
                    value_ty
                } else if value_ty == PhpType::Str || default_ty == PhpType::Str {
                    PhpType::Str
                } else if value_ty == PhpType::Float || default_ty == PhpType::Float {
                    PhpType::Float
                } else {
                    value_ty
                };
                Ok(result_ty)
            }
            ExprKind::Throw(inner) => {
                let thrown_ty = self.infer_type(inner, env)?;
                match thrown_ty {
                    PhpType::Object(type_name)
                        if self.object_type_implements_throwable(&type_name) =>
                    {
                        Ok(PhpType::Void)
                    }
                    PhpType::Object(_) => Err(CompileError::new(
                        expr.span,
                        "Type error: throw requires an object implementing Throwable",
                    )),
                    _ => Err(CompileError::new(
                        expr.span,
                        "Type error: throw requires an object value",
                    )),
                }
            }
            ExprKind::Cast { target, expr } => {
                self.infer_type(expr, env)?;
                use crate::parser::ast::CastType;
                Ok(match target {
                    CastType::Int => PhpType::Int,
                    CastType::Float => PhpType::Float,
                    CastType::String => PhpType::Str,
                    CastType::Bool => PhpType::Bool,
                    CastType::Array => PhpType::Array(Box::new(PhpType::Int)),
                })
            }
            ExprKind::FunctionCall { name, args } => {
                let name = name.as_str().to_string();
                let args = args.clone();
                if Self::has_named_args(&args) {
                    if self.extern_functions.contains_key(name.as_str()) {
                        return Err(CompileError::new(
                            expr.span,
                            &format!(
                                "Extern function '{}' does not support named arguments yet",
                                name
                            ),
                        ));
                    }
                    if crate::name_resolver::is_builtin_function(name.as_str()) {
                        return Err(CompileError::new(
                            expr.span,
                            &format!("Builtin '{}' does not support named arguments yet", name),
                        ));
                    }
                }
                if self.extern_functions.contains_key(name.as_str()) {
                    return self.check_extern_function_call(name.as_str(), &args, expr.span, env);
                }
                if let Some(ty) = self.check_builtin(name.as_str(), &args, expr.span, env)? {
                    return Ok(ty);
                }
                self.check_function_call(name.as_str(), &args, expr.span, env)
            }
            ExprKind::BufferNew { element_type, len } => {
                let len_ty = self.infer_type(len, env)?;
                if len_ty != PhpType::Int {
                    return Err(CompileError::new(
                        expr.span,
                        "buffer_new<T>() length must be integer",
                    ));
                }
                let elem_ty = self.resolve_type_expr(element_type, expr.span)?;
                if packed_type_size(&elem_ty, &self.packed_classes).is_none() {
                    return Err(CompileError::new(
                        expr.span,
                        "buffer_new<T>() requires a POD scalar, pointer, or packed class element type",
                    ));
                }
                Ok(PhpType::Buffer(Box::new(elem_ty)))
            }
            ExprKind::BitNot(inner) => {
                let ty = self.infer_type(inner, env)?;
                if !matches!(ty, PhpType::Int | PhpType::Bool | PhpType::Void) {
                    return Err(CompileError::new(
                        expr.span,
                        "Bitwise NOT requires integer operand",
                    ));
                }
                Ok(PhpType::Int)
            }
            ExprKind::NullCoalesce { value, default } => {
                let vt = self.infer_type(value, env)?;
                let dt = self.infer_type(default, env)?;
                if Self::union_contains_void(&vt) {
                    Ok(wider_type_syntactic(&self.strip_void_from_union(&vt), &dt))
                } else {
                    Ok(wider_type_syntactic(&vt, &dt))
                }
            }
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                ..
            } => {
                let mut scoped_env = env.clone();
                self.check_assignment_expression(
                    target,
                    value,
                    result_target.as_deref(),
                    prelude,
                    expr.span,
                    &mut scoped_env,
                )
            }
            ExprKind::ConstRef(name) => {
                self.constants.get(name.as_str()).cloned().ok_or_else(|| {
                    CompileError::new(expr.span, &format!("Undefined constant: {}", name))
                })
            }
            ExprKind::FirstClassCallable(target) => {
                self.infer_first_class_callable_target(target, expr.span, env)?;
                Ok(PhpType::Callable)
            }
            ExprKind::Closure {
                params,
                variadic,
                return_type,
                body,
                is_arrow: _,
                is_static,
                captures,
            } => {
                if *is_static {
                    body_must_not_use_this(body, expr.span)?;
                }
                self.infer_closure_type(params, variadic, return_type, body, captures, expr, env)
            }
            ExprKind::Spread(inner) => {
                let ty = self.infer_type(inner, env)?;
                match ty {
                    PhpType::Array(elem_ty) => Ok(*elem_ty),
                    _ => Err(CompileError::new(
                        expr.span,
                        "Spread operator requires an array",
                    )),
                }
            }
            ExprKind::NamedArg { value, .. } => self.infer_type(value, env),
            ExprKind::ClosureCall { var, args } => {
                self.infer_closure_call_type(var, args, expr, env)
            }
            ExprKind::ExprCall { callee, args } => {
                self.infer_expr_call_type(callee, args, expr, env)
            }
            ExprKind::BinaryOp { left, op, right } => {
                self.infer_binary_op_type(left, op, right, expr, env)
            }
            ExprKind::InstanceOf { value, target } => {
                self.infer_instanceof_type(value, target, expr, env)
            }
            ExprKind::NewObject { class_name, args } => {
                self.infer_new_object_type(class_name.as_str(), args, expr, env)
            }
            ExprKind::EnumCase {
                enum_name,
                case_name,
            } => self.infer_enum_case_type(enum_name.as_str(), case_name, expr),
            ExprKind::PropertyAccess { object, property } => {
                self.infer_property_access_type(object, property, expr, env)
            }
            ExprKind::NullsafePropertyAccess { object, property } => {
                self.infer_nullsafe_property_access_type(object, property, expr, env)
            }
            ExprKind::StaticPropertyAccess { receiver, property } => {
                self.infer_static_property_access_type(receiver, property, expr)
            }
            ExprKind::MethodCall {
                object,
                method,
                args,
            } => self.infer_method_call_type(object, method, args, expr, env),
            ExprKind::NullsafeMethodCall {
                object,
                method,
                args,
            } => self.infer_nullsafe_method_call_type(object, method, args, expr, env),
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args,
            } => self.infer_static_method_call_type(receiver, method, args, expr, env),
            ExprKind::This => self.infer_this_type(expr),
            ExprKind::PtrCast {
                target_type,
                expr: inner,
            } => self.infer_ptr_cast_type(target_type, inner, expr, env),
            ExprKind::ClassConstant { receiver } => {
                self.validate_class_constant_receiver(receiver, expr.span)?;
                Ok(PhpType::Str)
            }
            ExprKind::NewScopedObject { receiver, args } => {
                let class_name = match receiver {
                    crate::parser::ast::StaticReceiver::Self_ => {
                        self.current_class.clone().ok_or_else(|| {
                            CompileError::new(
                                expr.span,
                                "Cannot use 'new self()' outside a class context",
                            )
                        })?
                    }
                    crate::parser::ast::StaticReceiver::Static => {
                        let class_name = self.current_class.clone().ok_or_else(|| {
                            CompileError::new(
                                expr.span,
                                "Cannot use 'new static()' outside a class context",
                            )
                        })?;
                        self.validate_late_bound_constructor_targets(&class_name, args, expr, env)?;
                        return Ok(PhpType::Object(class_name));
                    }
                    crate::parser::ast::StaticReceiver::Parent => {
                        let current = self.current_class.as_ref().ok_or_else(|| {
                            CompileError::new(
                                expr.span,
                                "Cannot use 'new parent()' outside a class context",
                            )
                        })?;
                        self.classes
                            .get(current)
                            .and_then(|info| info.parent.clone())
                            .ok_or_else(|| {
                                CompileError::new(
                                    expr.span,
                                    &format!("Class '{}' has no parent class", current),
                                )
                            })?
                    }
                    crate::parser::ast::StaticReceiver::Named(name) => name.as_canonical(),
                };
                self.infer_new_object_type(&class_name, args, expr, env)
            }
            ExprKind::MagicConstant(_) => {
                unreachable!("MagicConstant must be lowered before type inference")
            }
        }
    }

    fn check_assignment_expression(
        &mut self,
        target: &Expr,
        value: &Expr,
        result_target: Option<&Expr>,
        prelude: &[Stmt],
        span: Span,
        env: &mut TypeEnv,
    ) -> Result<PhpType, CompileError> {
        for stmt in prelude {
            self.check_assignment_like_stmt(stmt, env)?;
        }

        if let ExprKind::Variable(name) = &target.kind {
            return self.check_local_assignment_expression(name, value, span, env);
        }

        let stmt_kind = match &target.kind {
            ExprKind::ArrayAccess { array, index } => match &array.kind {
                ExprKind::Variable(array) => StmtKind::ArrayAssign {
                    array: array.clone(),
                    index: *index.clone(),
                    value: value.clone(),
                },
                ExprKind::PropertyAccess { object, property } => StmtKind::PropertyArrayAssign {
                    object: object.clone(),
                    property: property.clone(),
                    index: *index.clone(),
                    value: value.clone(),
                },
                ExprKind::StaticPropertyAccess { receiver, property } => {
                    StmtKind::StaticPropertyArrayAssign {
                        receiver: receiver.clone(),
                        property: property.clone(),
                        index: *index.clone(),
                        value: value.clone(),
                    }
                }
                _ => return Err(CompileError::new(span, "Invalid assignment target")),
            },
            ExprKind::PropertyAccess { object, property } => StmtKind::PropertyAssign {
                object: object.clone(),
                property: property.clone(),
                value: value.clone(),
            },
            ExprKind::StaticPropertyAccess { receiver, property } => {
                StmtKind::StaticPropertyAssign {
                    receiver: receiver.clone(),
                    property: property.clone(),
                    value: value.clone(),
                }
            }
            _ => return Err(CompileError::new(span, "Invalid assignment target")),
        };

        let stmt = Stmt::new(stmt_kind, span);
        self.check_assignment_like_stmt(&stmt, env)?;
        self.infer_type(result_target.unwrap_or(target), env)
    }

}

impl Checker {
    fn validate_late_bound_constructor_targets(
        &mut self,
        base_class: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<(), CompileError> {
        let mut class_names: Vec<String> = self
            .classes
            .keys()
            .filter(|name| self.class_is_same_or_descends_from(name, base_class))
            .cloned()
            .collect();
        class_names.sort();

        for class_name in class_names {
            self.infer_new_object_type(&class_name, args, expr, env)?;
        }

        Ok(())
    }

    fn class_is_same_or_descends_from(&self, class_name: &str, base_class: &str) -> bool {
        let mut current = Some(class_name);
        while let Some(name) = current {
            if name == base_class {
                return true;
            }
            current = self
                .classes
                .get(name)
                .and_then(|info| info.parent.as_deref());
        }
        false
    }

    fn validate_class_constant_receiver(
        &self,
        receiver: &StaticReceiver,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        match receiver {
            StaticReceiver::Named(_) => Ok(()),
            StaticReceiver::Self_ | StaticReceiver::Static => {
                if self.current_class.is_some() {
                    Ok(())
                } else {
                    Err(CompileError::new(
                        span,
                        "Cannot use self::class or static::class outside a class context",
                    ))
                }
            }
            StaticReceiver::Parent => {
                let current = self.current_class.as_ref().ok_or_else(|| {
                    CompileError::new(
                        span,
                        "Cannot use parent::class outside a class context",
                    )
                })?;
                if self
                    .classes
                    .get(current)
                    .and_then(|info| info.parent.as_ref())
                    .is_some()
                {
                    Ok(())
                } else {
                    Err(CompileError::new(
                        span,
                        &format!("Class '{}' has no parent class", current),
                    ))
                }
            }
        }
    }
}

/// Walk a static closure body and reject any reference to `$this`. PHP forbids
/// `$this` inside `static function() {}` and `static fn() => ...` because the
/// closure isn't bound to an object instance.
fn body_must_not_use_this(body: &[Stmt], span: crate::span::Span) -> Result<(), CompileError> {
    for stmt in body {
        stmt_must_not_use_this(stmt, span)?;
    }
    Ok(())
}

fn stmt_must_not_use_this(stmt: &Stmt, span: crate::span::Span) -> Result<(), CompileError> {
    match &stmt.kind {
        StmtKind::Echo(e)
        | StmtKind::Throw(e)
        | StmtKind::ExprStmt(e)
        | StmtKind::Include { path: e, .. }
        | StmtKind::ConstDecl { value: e, .. }
        | StmtKind::StaticVar { init: e, .. }
        | StmtKind::ListUnpack { value: e, .. }
        | StmtKind::Return(Some(e))
        | StmtKind::Assign { value: e, .. }
        | StmtKind::TypedAssign { value: e, .. }
        | StmtKind::ArrayPush { value: e, .. } => expr_must_not_use_this(e, span),
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_must_not_use_this(index, span)?;
            expr_must_not_use_this(value, span)
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_must_not_use_this(object, span)?;
            expr_must_not_use_this(value, span)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            expr_must_not_use_this(object, span)?;
            expr_must_not_use_this(index, span)?;
            expr_must_not_use_this(value, span)
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => expr_must_not_use_this(value, span),
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_must_not_use_this(index, span)?;
            expr_must_not_use_this(value, span)
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_must_not_use_this(condition, span)?;
            body_must_not_use_this(then_body, span)?;
            for (cond, body) in elseif_clauses {
                expr_must_not_use_this(cond, span)?;
                body_must_not_use_this(body, span)?;
            }
            if let Some(body) = else_body {
                body_must_not_use_this(body, span)?;
            }
            Ok(())
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_must_not_use_this(condition, span)?;
            body_must_not_use_this(body, span)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(s) = init {
                stmt_must_not_use_this(s, span)?;
            }
            if let Some(c) = condition {
                expr_must_not_use_this(c, span)?;
            }
            if let Some(s) = update {
                stmt_must_not_use_this(s, span)?;
            }
            body_must_not_use_this(body, span)
        }
        StmtKind::Foreach { array, body, .. } => {
            expr_must_not_use_this(array, span)?;
            body_must_not_use_this(body, span)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_must_not_use_this(subject, span)?;
            for (patterns, body) in cases {
                for pattern in patterns {
                    expr_must_not_use_this(pattern, span)?;
                }
                body_must_not_use_this(body, span)?;
            }
            if let Some(body) = default {
                body_must_not_use_this(body, span)?;
            }
            Ok(())
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            body_must_not_use_this(try_body, span)?;
            for catch in catches {
                body_must_not_use_this(&catch.body, span)?;
            }
            if let Some(body) = finally_body {
                body_must_not_use_this(body, span)?;
            }
            Ok(())
        }
        StmtKind::NamespaceBlock { body, .. } => body_must_not_use_this(body, span),
        StmtKind::FunctionDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::InterfaceDecl { .. } => Ok(()),
        _ => Ok(()),
    }
}

fn expr_must_not_use_this(expr: &Expr, span: crate::span::Span) -> Result<(), CompileError> {
    match &expr.kind {
        ExprKind::This => Err(CompileError::new(
            span,
            "Cannot use $this inside a static closure",
        )),
        ExprKind::BinaryOp { left, right, .. } => {
            expr_must_not_use_this(left, span)?;
            expr_must_not_use_this(right, span)
        }
        ExprKind::InstanceOf { value: inner, .. }
        | ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::Cast { expr: inner, .. } => expr_must_not_use_this(inner, span),
        ExprKind::NullCoalesce { value, default } => {
            expr_must_not_use_this(value, span)?;
            expr_must_not_use_this(default, span)
        }
        ExprKind::ShortTernary { value, default } => {
            expr_must_not_use_this(value, span)?;
            expr_must_not_use_this(default, span)
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. } => {
            for arg in args {
                expr_must_not_use_this(arg, span)?;
            }
            Ok(())
        }
        ExprKind::ExprCall { callee, args } => {
            expr_must_not_use_this(callee, span)?;
            for arg in args {
                expr_must_not_use_this(arg, span)?;
            }
            Ok(())
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_must_not_use_this(object, span)?;
            for arg in args {
                expr_must_not_use_this(arg, span)?;
            }
            Ok(())
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                expr_must_not_use_this(item, span)?;
            }
            Ok(())
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (k, v) in pairs {
                expr_must_not_use_this(k, span)?;
                expr_must_not_use_this(v, span)?;
            }
            Ok(())
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_must_not_use_this(array, span)?;
            expr_must_not_use_this(index, span)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_must_not_use_this(condition, span)?;
            expr_must_not_use_this(then_expr, span)?;
            expr_must_not_use_this(else_expr, span)
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_must_not_use_this(subject, span)?;
            for (patterns, value) in arms {
                for p in patterns {
                    expr_must_not_use_this(p, span)?;
                }
                expr_must_not_use_this(value, span)?;
            }
            if let Some(d) = default {
                expr_must_not_use_this(d, span)?;
            }
            Ok(())
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_must_not_use_this(object, span),
        ExprKind::NamedArg { value, .. } => expr_must_not_use_this(value, span),
        ExprKind::BufferNew { len, .. } => expr_must_not_use_this(len, span),
        ExprKind::FirstClassCallable(target) => callable_target_must_not_use_this(target, span),
        ExprKind::Closure { body, .. } => body_must_not_use_this(body, span),
        _ => Ok(()),
    }
}

fn callable_target_must_not_use_this(
    target: &CallableTarget,
    span: crate::span::Span,
) -> Result<(), CompileError> {
    match target {
        CallableTarget::Method { object, .. } => expr_must_not_use_this(object, span),
        CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => Ok(()),
    }
}
