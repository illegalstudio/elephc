use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{BinOp, Expr, ExprKind, InstanceOfTarget, Stmt, TypeExpr};
use crate::types::{merge_array_key_types, PhpType, TypeEnv};

use super::super::Checker;
use super::syntactic::infer_return_type_syntactic;

impl Checker {
    pub(crate) fn infer_binary_op_type(
        &mut self,
        left: &Expr,
        op: &BinOp,
        right: &Expr,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let lt = self.infer_type(left, env)?;
        let rt = self.infer_type(right, env)?;
        match op {
            BinOp::Pow => {
                let lt_ok = matches!(
                    lt,
                    PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                ) || self.is_union_with_mixed_int_dispatch(&lt);
                let rt_ok = matches!(
                    rt,
                    PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                ) || self.is_union_with_mixed_int_dispatch(&rt);
                if !lt_ok || !rt_ok {
                    return Err(CompileError::new(
                        expr.span,
                        "Exponentiation requires numeric operands",
                    ));
                }
                Ok(PhpType::Float)
            }
            BinOp::Add => {
                if is_array_like_type(&lt) || is_array_like_type(&rt) {
                    return self.infer_array_union_type(&lt, &rt, left, right, expr);
                }
                let lt_ok = matches!(
                    lt,
                    PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                ) || self.is_union_with_mixed_int_dispatch(&lt);
                let rt_ok = matches!(
                    rt,
                    PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                ) || self.is_union_with_mixed_int_dispatch(&rt);
                if !lt_ok || !rt_ok {
                    return Err(CompileError::new(
                        expr.span,
                        "Arithmetic operators require numeric operands",
                    ));
                }
                if lt == PhpType::Float || rt == PhpType::Float {
                    Ok(PhpType::Float)
                } else {
                    Ok(PhpType::Int)
                }
            }
            BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                let lt_ok = matches!(
                    lt,
                    PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                ) || self.is_union_with_mixed_int_dispatch(&lt);
                let rt_ok = matches!(
                    rt,
                    PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                ) || self.is_union_with_mixed_int_dispatch(&rt);
                if !lt_ok || !rt_ok {
                    return Err(CompileError::new(
                        expr.span,
                        "Arithmetic operators require numeric operands",
                    ));
                }
                // Division always returns float (PHP compat: 10/3 → 3.333...)
                if *op == BinOp::Div || lt == PhpType::Float || rt == PhpType::Float {
                    Ok(PhpType::Float)
                } else {
                    Ok(PhpType::Int)
                }
            }
            BinOp::Eq | BinOp::NotEq => {
                if Self::is_pointer_type(&lt) || Self::is_pointer_type(&rt) {
                    return Err(CompileError::new(
                        expr.span,
                        "Loose pointer comparison is not supported; use === or !==",
                    ));
                }
                // Loose comparison accepts any types — coerces at runtime
                Ok(PhpType::Bool)
            }
            BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => {
                let lt_ok = matches!(
                    lt,
                    PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                ) || self.is_union_with_mixed_int_dispatch(&lt);
                let rt_ok = matches!(
                    rt,
                    PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                ) || self.is_union_with_mixed_int_dispatch(&rt);
                if !lt_ok || !rt_ok {
                    return Err(CompileError::new(
                        expr.span,
                        "Comparison operators require numeric operands",
                    ));
                }
                Ok(PhpType::Bool)
            }
            BinOp::StrictEq | BinOp::StrictNotEq => {
                // Strict comparison accepts any types — compares both type and value
                Ok(PhpType::Bool)
            }
            BinOp::Concat => Ok(PhpType::Str),
            BinOp::And | BinOp::Or | BinOp::Xor => Ok(PhpType::Bool),
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight => {
                let lt_ok = matches!(lt, PhpType::Int | PhpType::Bool | PhpType::Void)
                    || self.is_union_with_mixed_int_dispatch(&lt);
                let rt_ok = matches!(rt, PhpType::Int | PhpType::Bool | PhpType::Void)
                    || self.is_union_with_mixed_int_dispatch(&rt);
                if !lt_ok || !rt_ok {
                    return Err(CompileError::new(
                        expr.span,
                        "Bitwise operators require integer operands",
                    ));
                }
                Ok(PhpType::Int)
            }
            BinOp::Spaceship => {
                let lt_ok = matches!(
                    lt,
                    PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                ) || self.is_union_with_mixed_int_dispatch(&lt);
                let rt_ok = matches!(
                    rt,
                    PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                ) || self.is_union_with_mixed_int_dispatch(&rt);
                if !lt_ok || !rt_ok {
                    return Err(CompileError::new(
                        expr.span,
                        "Spaceship operator requires numeric operands",
                    ));
                }
                Ok(PhpType::Int)
            }
            BinOp::NullCoalesce => {
                // Handled by ExprKind::NullCoalesce — shouldn't reach here
                // but handle gracefully
                if lt == PhpType::Void {
                    Ok(rt)
                } else {
                    Ok(lt)
                }
            }
        }
    }

    fn infer_array_union_type(
        &self,
        lt: &PhpType,
        rt: &PhpType,
        left: &Expr,
        right: &Expr,
        expr: &Expr,
    ) -> Result<PhpType, CompileError> {
        match (lt, rt) {
            (PhpType::Array(left_elem), PhpType::Array(right_elem)) => {
                if is_empty_indexed_array_literal(left) {
                    return Ok(PhpType::Array(right_elem.clone()));
                }
                if is_empty_indexed_array_literal(right) {
                    return Ok(PhpType::Array(left_elem.clone()));
                }
                self.merge_array_element_type(left_elem, right_elem)
                    .map(|elem| PhpType::Array(Box::new(elem)))
                    .ok_or_else(|| {
                        CompileError::new(
                            expr.span,
                            "Array union requires compatible indexed array element types",
                        )
                    })
            }
            (
                PhpType::AssocArray {
                    key: left_key,
                    value: left_value,
                },
                PhpType::AssocArray {
                    key: right_key,
                    value: right_value,
                },
            ) => {
                let key = self
                    .merge_array_element_type(left_key, right_key)
                    .unwrap_or_else(|| merge_array_key_types(*left_key.clone(), *right_key.clone()));
                let value = self
                    .merge_array_element_type(left_value, right_value)
                    .unwrap_or(PhpType::Mixed);
                Ok(PhpType::AssocArray {
                    key: Box::new(key),
                    value: Box::new(value),
                })
            }
            _ => Err(CompileError::new(
                expr.span,
                "Array union requires both operands to be arrays of the same kind",
            )),
        }
    }

    pub(crate) fn infer_instanceof_type(
        &mut self,
        value: &Expr,
        target: &InstanceOfTarget,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        self.infer_type(value, env)?;
        match target {
            InstanceOfTarget::Name(name) => {
                self.resolve_instanceof_target_name(name, expr.span)?;
            }
            InstanceOfTarget::Expr(target_expr) => {
                self.infer_type(target_expr, env)?;
            }
        }
        Ok(PhpType::Bool)
    }

    pub(crate) fn resolve_instanceof_target_name(
        &self,
        target: &Name,
        span: crate::span::Span,
    ) -> Result<String, CompileError> {
        match target.as_str() {
            "self" => self.current_class.clone().ok_or_else(|| {
                CompileError::new(span, "Cannot use self in instanceof outside of a class context")
            }),
            "parent" => {
                let current_class = self.current_class.as_ref().ok_or_else(|| {
                    CompileError::new(
                        span,
                        "Cannot use parent in instanceof outside of a class context",
                    )
                })?;
                self.classes
                    .get(current_class)
                    .and_then(|class_info| class_info.parent.clone())
                    .ok_or_else(|| CompileError::new(span, "Class has no parent class"))
            }
            "static" => self.current_class.clone().ok_or_else(|| {
                CompileError::new(
                    span,
                    "Cannot use static in instanceof outside of a class context",
                )
            }),
            _ => Ok(target.as_str().to_string()),
        }
    }

    pub(crate) fn infer_closure_type(
        &mut self,
        params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
        variadic: &Option<String>,
        return_type: &Option<TypeExpr>,
        body: &[Stmt],
        captures: &[String],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let mut closure_sig = self.prepare_closure_signature_context(
            params,
            variadic,
            captures,
            expr.span,
            env,
        )?;
        let closure_ref_params: Vec<String> = params
            .iter()
            .filter(|(_, _, _, is_ref)| *is_ref)
            .map(|(name, _, _, _)| name.clone())
            .collect();
        self.with_local_storage_context(closure_ref_params, |checker| {
            for stmt in body {
                checker.check_stmt(stmt, &mut closure_sig.env)?;
            }
            Ok(())
        })?;
        self.resolve_closure_return_type(body, return_type, expr.span, &closure_sig.env)?;
        Ok(PhpType::Callable)
    }

    pub(crate) fn infer_closure_call_type(
        &mut self,
        var: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let var_ty = env.get(var).cloned().ok_or_else(|| {
            CompileError::new(expr.span, &format!("Undefined variable: ${}", var))
        })?;
        if var_ty != PhpType::Callable {
            return Err(CompileError::new(
                expr.span,
                &format!("Cannot call ${} — not a callable (got {:?})", var, var_ty),
            ));
        }
        if let Some(sig) = self.callable_sigs.get(var).cloned() {
            if let Some(target) = self.first_class_callable_targets.get(var).cloned() {
                let specialized_sig =
                    self.specialize_first_class_callable_target(&target, args, expr.span, env)?;
                self.callable_sigs
                    .insert(var.to_string(), specialized_sig.clone());
                self.closure_return_types
                    .insert(var.to_string(), specialized_sig.return_type.clone());
                return self.check_known_callable_call(
                    &specialized_sig,
                    args,
                    expr.span,
                    env,
                    &format!("callable ${}", var),
                );
            }
            return self.check_known_callable_call(
                &sig,
                args,
                expr.span,
                env,
                &format!("callable ${}", var),
            );
        }
        if Self::has_named_args(args) {
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "callable ${} does not support named arguments without a known signature",
                    var
                ),
            ));
        }
        for arg in args {
            self.infer_type(arg, env)?;
        }
        Ok(self
            .closure_return_types
            .get(var)
            .cloned()
            .unwrap_or(PhpType::Int))
    }

    pub(crate) fn infer_expr_call_type(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let callee_ty = self.infer_type(callee, env)?;
        let nullable_callable =
            Self::is_nullable_callable_from_nullsafe_chain(callee, &callee_ty);
        if callee_ty != PhpType::Callable && !nullable_callable {
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "Cannot call expression — not a callable (got {:?})",
                    callee_ty
                ),
            ));
        }
        match &callee.kind {
            ExprKind::Variable(var_name) => {
                if let Some(sig) = self.callable_sigs.get(var_name).cloned() {
                    if let Some(target) = self.first_class_callable_targets.get(var_name).cloned() {
                        let specialized_sig = self.specialize_first_class_callable_target(
                            &target, args, expr.span, env,
                        )?;
                        self.callable_sigs
                            .insert(var_name.clone(), specialized_sig.clone());
                        self.closure_return_types
                            .insert(var_name.clone(), specialized_sig.return_type.clone());
                        let ret_ty = self.check_known_callable_call(
                            &specialized_sig,
                            args,
                            expr.span,
                            env,
                            &format!("callable ${}", var_name),
                        )?;
                        return Ok(self.nullable_callable_result(ret_ty, nullable_callable));
                    }
                    let ret_ty = self.check_known_callable_call(
                        &sig,
                        args,
                        expr.span,
                        env,
                        &format!("callable ${}", var_name),
                    )?;
                    return Ok(self.nullable_callable_result(ret_ty, nullable_callable));
                }
            }
            ExprKind::FirstClassCallable(target) => {
                let sig =
                    self.specialize_first_class_callable_target(target, args, expr.span, env)?;
                let ret_ty = self.check_known_callable_call(
                    &sig,
                    args,
                    expr.span,
                    env,
                    "first-class callable",
                )?;
                return Ok(self.nullable_callable_result(ret_ty, nullable_callable));
            }
            _ => {}
        }
        if Self::has_named_args(args) {
            return Err(CompileError::new(
                expr.span,
                "Callable expression does not support named arguments without a known signature",
            ));
        }
        for arg in args {
            self.infer_type(arg, env)?;
        }
        // Try to determine return type from closure signature
        match &callee.kind {
            ExprKind::Variable(var_name) => {
                if let Some(ret_ty) = self.closure_return_types.get(var_name) {
                    return Ok(self.nullable_callable_result(ret_ty.clone(), nullable_callable));
                }
            }
            ExprKind::ArrayAccess { array, .. } => {
                if let ExprKind::Variable(arr_name) = &array.kind {
                    if let Some(ret_ty) = self.closure_return_types.get(arr_name) {
                        return Ok(self.nullable_callable_result(ret_ty.clone(), nullable_callable));
                    }
                }
            }
            ExprKind::Closure { body, .. } => {
                let ret_ty = infer_return_type_syntactic(body);
                return Ok(self.nullable_callable_result(ret_ty, nullable_callable));
            }
            _ => {}
        }
        Ok(self.nullable_callable_result(PhpType::Int, nullable_callable)) // fallback for unknown callables
    }

    fn nullable_callable_result(&self, ret_ty: PhpType, nullable_callable: bool) -> PhpType {
        if nullable_callable {
            self.normalize_union_type(vec![ret_ty, PhpType::Void])
        } else {
            ret_ty
        }
    }

    fn is_nullable_callable_from_nullsafe_chain(callee: &Expr, callee_ty: &PhpType) -> bool {
        let PhpType::Union(members) = callee_ty else {
            return false;
        };
        members.iter().any(|member| *member == PhpType::Callable)
            && members.iter().any(|member| *member == PhpType::Void)
            && expr_contains_nullsafe_member(callee)
    }
}

fn expr_contains_nullsafe_member(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::NullsafePropertyAccess { .. } | ExprKind::NullsafeMethodCall { .. } => true,
        ExprKind::PropertyAccess { object, .. } | ExprKind::MethodCall { object, .. } => {
            expr_contains_nullsafe_member(object)
        }
        ExprKind::ArrayAccess { array, .. } => expr_contains_nullsafe_member(array),
        ExprKind::ExprCall { callee, .. } => expr_contains_nullsafe_member(callee),
        _ => false,
    }
}

fn is_array_like_type(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. })
}

fn is_empty_indexed_array_literal(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::ArrayLiteral(elems) if elems.is_empty())
}
