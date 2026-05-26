//! Purpose:
//! Dispatches expression inference for assignments, class references, closures, and side-effecting forms.
//! Feeds statement checking, function call validation, and optimizer-visible type metadata.
//!
//! Called from:
//! - `crate::types::checker::Checker::infer_type()`
//!
//! Key details:
//! - Inference must preserve PHP evaluation errors and avoid treating effectful expressions as pure type facts.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{
    merge_array_key_types, normalized_array_key_type, packed_type_size, PhpType, TypeEnv,
};
mod assignments;
mod class_refs;
mod effects;
mod static_closure;
use super::super::Checker;
use super::syntactic::wider_type_syntactic;
use static_closure::body_must_not_use_this;
impl Checker {
    /// Infers the PHP return type of `expr` in the given `env`.
    ///
    /// This is the top-level dispatcher for expression type inference. It
    /// handles literals, variables, operators, array access, ternaries,
    /// function calls, closures, and all other expression forms. Errors are
    /// returned for type mismatches (e.g. negating a string) or undefined
    /// references. The result feeds statement checking, function call
    /// validation, and optimizer-visible type metadata.
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
                    return Ok(PhpType::Array(Box::new(PhpType::Never)));
                }
                if elems.iter().any(|elem| {
                    matches!(
                        &elem.kind,
                        ExprKind::Spread(inner)
                            if matches!(
                                self.infer_type(inner, env),
                                Ok(PhpType::AssocArray { .. })
                            )
                    )
                }) {
                    let value_ty = self.assoc_spread_literal_value_type(elems, env);
                    return Ok(PhpType::AssocArray {
                        key: Box::new(PhpType::Mixed),
                        value: Box::new(value_ty),
                    });
                }
                let mut elem_ty = self.infer_type(&elems[0], env)?;
                for elem in &elems[1..] {
                    let ty = self.infer_type(elem, env)?;
                    if ty != elem_ty {
                        if let Some(merged_ty) = self.merge_array_element_type(&elem_ty, &ty) {
                            elem_ty = merged_ty;
                            continue;
                        }
                        elem_ty = PhpType::Mixed;
                    }
                }
                Ok(PhpType::Array(Box::new(elem_ty)))
            }
            ExprKind::ArrayAccess { array, index } => {
                let arr_ty = self.infer_type(array, env)?;
                let idx_ty = self.infer_type(index, env)?;
                match &arr_ty {
                    PhpType::Str => {
                        if !is_valid_string_offset_index(index, &idx_ty) {
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
                    PhpType::Object(class_name) => {
                        if self.object_type_implements_interface(class_name, "ArrayAccess") {
                            Ok(self.array_access_offset_get_type(class_name))
                        } else {
                            Err(CompileError::new(expr.span, "Cannot index non-array"))
                        }
                    }
                    PhpType::Union(members) => {
                        let mut result_members = Vec::new();
                        let mut saw_indexable_member = false;
                        let mut first_index_error = None;
                        for member in members {
                            match member {
                                PhpType::Void => result_members.push(PhpType::Void),
                                PhpType::Str => {
                                    saw_indexable_member = true;
                                    if !is_valid_string_offset_index(index, &idx_ty) {
                                        first_index_error =
                                            first_index_error.or(Some("String index must be integer"));
                                        continue;
                                    }
                                    result_members.push(PhpType::Str);
                                }
                                PhpType::Array(elem_ty) => {
                                    saw_indexable_member = true;
                                    if idx_ty != PhpType::Int {
                                        first_index_error =
                                            first_index_error.or(Some("Array index must be integer"));
                                        continue;
                                    }
                                    result_members.push(*elem_ty.clone());
                                }
                                PhpType::AssocArray { value, .. } => {
                                    saw_indexable_member = true;
                                    result_members.push(*value.clone());
                                }
                                PhpType::Object(class_name) => {
                                    if self.object_type_implements_interface(
                                        class_name,
                                        "ArrayAccess",
                                    ) {
                                        saw_indexable_member = true;
                                        result_members
                                            .push(self.array_access_offset_get_type(class_name));
                                    }
                                }
                                PhpType::Buffer(elem_ty) => {
                                    saw_indexable_member = true;
                                    if idx_ty != PhpType::Int {
                                        first_index_error =
                                            first_index_error.or(Some("Buffer index must be integer"));
                                        continue;
                                    }
                                    match elem_ty.as_ref() {
                                        PhpType::Packed(name) => {
                                            result_members.push(PhpType::Pointer(Some(name.clone())))
                                        }
                                        _ => result_members.push(*elem_ty.clone()),
                                    }
                                }
                                _ => {}
                            }
                        }
                        let has_concrete_result =
                            result_members.iter().any(|member| *member != PhpType::Void);
                        if !has_concrete_result && saw_indexable_member {
                            Err(CompileError::new(
                                expr.span,
                                first_index_error.unwrap_or("Cannot index non-array"),
                            ))
                        } else if result_members.is_empty() {
                            Err(CompileError::new(expr.span, "Cannot index non-array"))
                        } else {
                            Ok(self.normalize_union_type(result_members))
                        }
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
                    // Mixed receivers fall through to runtime dispatch. The
                    // boxed payload may carry an indexed array, an assoc
                    // hash, or a stdClass; codegen unboxes and routes to
                    // the right runtime helper. Missing keys decode to
                    // `Mixed(null)` at runtime, mirroring PHP's silent
                    // "undefined index" warning behavior for this very
                    // common idiom (e.g. `json_decode($json, true)["k"]`).
                    PhpType::Mixed => Ok(PhpType::Mixed),
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
            ExprKind::Pipe { value, callable } => {
                self.infer_pipe_type(value, callable, expr, env)
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
                capture_refs,
            } => {
                if *is_static {
                    body_must_not_use_this(body, expr.span)?;
                }
                self.infer_closure_type(
                    params,
                    variadic,
                    return_type,
                    body,
                    captures,
                    capture_refs,
                    expr,
                    env,
                )
            }
            ExprKind::Spread(inner) => {
                let ty = self.infer_type(inner, env)?;
                match ty {
                    PhpType::Array(elem_ty) => Ok(*elem_ty),
                    PhpType::AssocArray { value, .. } => Ok(*value),
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
            ExprKind::PropertyAccess { object, property } => {
                self.infer_property_access_type(object, property, expr, env)
            }
            ExprKind::DynamicPropertyAccess { object, property } => {
                self.infer_dynamic_property_access_type(object, property, expr, env, false)
            }
            ExprKind::NullsafePropertyAccess { object, property } => {
                self.infer_nullsafe_property_access_type(object, property, expr, env)
            }
            ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
                self.infer_dynamic_property_access_type(object, property, expr, env, true)
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
            ExprKind::ScopedConstantAccess { receiver, name } => {
                self.infer_scoped_constant_access(receiver, name, expr)
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
            ExprKind::Yield { key, value } => {
                if let Some(k) = key {
                    self.infer_type(k, env)?;
                }
                if let Some(v) = value {
                    self.infer_type(v, env)?;
                }
                Ok(PhpType::Mixed)
            }
            ExprKind::YieldFrom(inner) => {
                let inner_ty = self.infer_type(inner, env)?;
                let supported = match &inner.kind {
                    ExprKind::ArrayLiteral(_) => true,
                    ExprKind::FunctionCall { .. } | ExprKind::Variable(_) => {
                        self.type_accepts(&PhpType::Object("Generator".to_string()), &inner_ty)
                    }
                    _ => false,
                };
                if !supported {
                    return Err(CompileError::new(
                        inner.span,
                        &format!(
                            "yield from expects an array literal or Generator, got {:?}",
                            inner_ty
                        ),
                    ));
                }
                Ok(PhpType::Mixed)
            }
            ExprKind::MagicConstant(_) => {
                unreachable!("MagicConstant must be lowered before type inference")
            }
        }
    }

    /// Returns the element type of an array literal that contains at least one
    /// spread of an associative array.
    ///
    /// Iterates over `elems`, extracting the value type from each `Spread` that
    /// wraps an `AssocArray`. All spread value types must agree, otherwise
    /// `Mixed` is returned. Non-spread elements are ignored.
    fn assoc_spread_literal_value_type(&mut self, elems: &[Expr], env: &TypeEnv) -> PhpType {
        let mut value_ty = PhpType::Never;
        for elem in elems {
            let ExprKind::Spread(inner) = &elem.kind else {
                continue;
            };
            let next = match self.infer_type(inner, env) {
                Ok(PhpType::Array(elem)) => *elem,
                Ok(PhpType::AssocArray { value, .. }) => *value,
                _ => PhpType::Mixed,
            };
            if matches!(value_ty, PhpType::Never) {
                value_ty = next;
            } else if value_ty != next {
                value_ty = PhpType::Mixed;
            }
        }
        if matches!(value_ty, PhpType::Never) {
            PhpType::Mixed
        } else {
            value_ty
        }
    }

    /// Returns the return type of the `offsetGet` method for `class_name`,
    /// or `Mixed` if no `offsetGet` method is found.
    ///
    /// Looks up `offsetGet` in the class's method table first, then falls back
    /// to the `ArrayAccess` interface. Used when indexing an `Object` that
    /// implements `ArrayAccess`.
    fn array_access_offset_get_type(&self, class_name: &str) -> PhpType {
        self.classes
            .get(class_name)
            .and_then(|class_info| class_info.methods.get("offsetget"))
            .map(|sig| sig.return_type.clone())
            .or_else(|| {
                self.interfaces
                    .get("ArrayAccess")
                    .and_then(|interface_info| interface_info.methods.get("offsetget"))
                    .map(|sig| sig.return_type.clone())
            })
            .unwrap_or(PhpType::Mixed)
    }

}

/// Returns `true` if `index` is a valid string offset index for a string receiver.
///
/// A valid index is an integer type, or a string literal whose value can be
/// parsed as a PHP string offset (e.g. `"0"`, `"-1"`, `"10"`).
fn is_valid_string_offset_index(index: &Expr, idx_ty: &PhpType) -> bool {
    *idx_ty == PhpType::Int
        || matches!(
            &index.kind,
            ExprKind::StringLiteral(value)
                if crate::types::parse_php_string_offset_literal(value).is_some()
        )
}
