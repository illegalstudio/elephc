//! Purpose:
//! Infers callable, object, property, scoped, yield, and assignment expression types.
//!
//! Called from:
//! - "super::Checker::infer_type()" for call and object-oriented expression families.
//!
//! Key details:
//! - Retains constructor, visibility, eval-barrier, callable, and late-static-binding checks.

use super::{body_must_not_use_this, merge_null_coalesce_result_type, Checker};
use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{packed_type_size, PhpType, TypeEnv};

impl Checker {
    /// Infers types for calls, closures, objects, property access, assignments, and generators.
    pub(super) fn infer_call_or_object_expr_type(
        &mut self,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        match &expr.kind {
            ExprKind::FunctionCall { name, args } => {
                let name = name.as_str().to_string();
                let args = args.clone();
                if self.extern_functions.contains_key(name.as_str()) {
                    return self.check_extern_function_call(name.as_str(), &args, expr.span, env);
                }
                if let Some(ty) = self.check_builtin(name.as_str(), &args, expr.span, env)? {
                    self.builtin_call_types.insert(expr.span, ty.clone());
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
                if !matches!(ty, PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Void) {
                    return Err(CompileError::new(
                        expr.span,
                        "Bitwise NOT requires integer operand",
                    ));
                }
                Ok(PhpType::Int)
            }
            ExprKind::NullCoalesce { value, default } => {
                // `??` is a null probe: PHP evaluates `$neverDefined ?? $d` to `$d` without an
                // undefined-variable warning, so a never-declared chain root reads as `null`
                // here. The default operand keeps ordinary inference.
                let probed =
                    crate::types::checker::null_probe::null_probe_env(self, value, env);
                let probed_env = probed.clone();
                let vt = self
                    .infer_null_probe_operand(value, probed_env.as_ref().unwrap_or(env))?;
                let dt = self.infer_type(default, env)?;
                let non_null_value = if Self::union_contains_void(&vt) {
                    self.strip_void_from_union(&vt)
                } else {
                    vt
                };
                Ok(merge_null_coalesce_result_type(non_null_value, dt))
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
                self.constants
                    .get(name.as_str())
                    .cloned()
                    .or_else(|| self.eval_barrier_active.then_some(PhpType::Mixed))
                    .ok_or_else(|| {
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
                variadic_by_ref,
                variadic_type: _,
                return_type,
                body,
                is_arrow: _,
                is_static,
                captures,
                capture_refs,
                by_ref_return: _,
            } => {
                if *is_static {
                    body_must_not_use_this(body, expr.span)?;
                }
                self.infer_closure_type(
                    params,
                    variadic,
                    *variadic_by_ref,
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
            ExprKind::Clone(inner) => {
                let ty = self.infer_type(inner, env)?;
                match ty {
                    PhpType::Object(class_name) => {
                        self.check_clone_visibility(&class_name, expr.span)?;
                        Ok(PhpType::Object(class_name))
                    }
                    PhpType::Mixed | PhpType::Union(_) => Ok(PhpType::Mixed),
                    _ => Err(CompileError::new(expr.span, "clone requires an object value")),
                }
            }
            ExprKind::NewDynamic { name_expr, args } => {
                // The class is named at runtime; without a literal class
                // we can't typecheck constructor args or the resulting
                // object's type. Infer the name expression for its side
                // effects + warnings, type-check the args generically, and
                // return Mixed.
                // The unpack-after-named shape is syntactic in
                // PHP, so it is still rejected without a known constructor.
                self.require_no_spread_after_named_args(args, "Dynamic constructor")?;
                // No constructor signature means no parameter binding modes either, so every
                // argument is conservatively reference-aliased — see
                // `Checker::record_unresolved_callee_argument_aliases`.
                self.record_unresolved_callee_argument_aliases(args);
                self.infer_type(name_expr, env)?;
                for arg in args {
                    self.infer_type(arg, env)?;
                }
                Ok(PhpType::Mixed)
            }
            ExprKind::NewDynamicObject {
                class_name,
                fallback_class,
                args,
                ..
            } => {
                let class_ty = self.infer_type(class_name, env)?;
                if class_ty != PhpType::Str {
                    return Err(CompileError::new(
                        class_name.span,
                        "Dynamic object factory class must be a string",
                    ));
                }
                self.infer_new_object_type(fallback_class.as_str(), args, expr, env)?;
                self.require_phar_archive_libraries();
                Ok(PhpType::Object(fallback_class.as_str().to_string()))
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
                self.infer_static_property_access_type(receiver, property, expr, env)
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
            ExprKind::NullsafeDynamicMethodCall {
                object,
                method,
                args,
            } => self.infer_nullsafe_dynamic_method_call_type(object, method, args, expr, env),
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
            ExprKind::ObjectClassName { object } => {
                let object_type = self.infer_type(object, env)?;
                let object_only = match &object_type {
                    PhpType::Object(_) => true,
                    PhpType::Union(members) => {
                        !members.is_empty()
                            && members.iter().all(|member| matches!(member, PhpType::Object(_)))
                    }
                    _ => false,
                };
                if !object_only {
                    return Err(CompileError::new(
                        expr.span,
                        &format!("Cannot use \"::class\" on {}", object_type),
                    ));
                }
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
                // `yield from` over an array is desugared by EIR lowering into an
                // iterator loop that re-yields every key/value pair, and that loop
                // handles indexed and keyed literals alike (`lower_yield_from_array`
                // dispatches on `Array` *and* `AssocArray`). Accept a keyed literal
                // (`[5 => "x", "s" => "y"]`, `[$i * 10 => "L"]`) the same way an
                // indexed one is accepted; PHP accepts both.
                let supported = match &inner.kind {
                    ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => true,
                    ExprKind::FunctionCall { .. } | ExprKind::Variable(_) => {
                        matches!(inner_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
                            || self
                                .type_accepts(&PhpType::Object("Generator".to_string()), &inner_ty)
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
            _ => unreachable!("basic expression routed to call/object inference"),
        }
    }
}
