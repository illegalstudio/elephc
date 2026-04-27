use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, TypeExpr};
use crate::types::{PhpType, TypeEnv};

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
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
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

    pub(crate) fn infer_instanceof_type(
        &mut self,
        value: &Expr,
        target: &Name,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        self.infer_type(value, env)?;
        self.resolve_instanceof_target_name(target, expr.span)?;
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
        body: &[Stmt],
        captures: &[String],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        // Verify captured variables exist in the enclosing scope
        for cap in captures {
            if !env.contains_key(cap) {
                return Err(CompileError::new(
                    expr.span,
                    &format!("Undefined variable in use(): ${}", cap),
                ));
            }
        }
        // Type-check the closure body in its own environment
        let mut closure_env: TypeEnv = env.clone();
        for (p, type_ann, default, _is_ref) in params {
            let ty = match type_ann {
                Some(type_ann) => {
                    let declared_ty = self.resolve_declared_param_type_hint(
                        type_ann,
                        expr.span,
                        &format!("Closure parameter ${}", p),
                    )?;
                    self.validate_declared_default_type(
                        &declared_ty,
                        default.as_ref(),
                        expr.span,
                        &format!("Closure parameter ${}", p),
                    )?;
                    declared_ty
                }
                None => PhpType::Int,
            };
            closure_env.insert(p.clone(), ty);
        }
        if let Some(vp) = variadic {
            closure_env.insert(vp.clone(), PhpType::Array(Box::new(PhpType::Int)));
        }
        let closure_ref_params: Vec<String> = params
            .iter()
            .filter(|(_, _, _, is_ref)| *is_ref)
            .map(|(name, _, _, _)| name.clone())
            .collect();
        self.with_local_storage_context(closure_ref_params, |checker| {
            for stmt in body {
                checker.check_stmt(stmt, &mut closure_env)?;
            }
            Ok(())
        })?;
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
        if callee_ty != PhpType::Callable {
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
                        return self.check_known_callable_call(
                            &specialized_sig,
                            args,
                            expr.span,
                            env,
                            &format!("callable ${}", var_name),
                        );
                    }
                    return self.check_known_callable_call(
                        &sig,
                        args,
                        expr.span,
                        env,
                        &format!("callable ${}", var_name),
                    );
                }
            }
            ExprKind::FirstClassCallable(target) => {
                let sig =
                    self.specialize_first_class_callable_target(target, args, expr.span, env)?;
                return self.check_known_callable_call(
                    &sig,
                    args,
                    expr.span,
                    env,
                    "first-class callable",
                );
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
                    return Ok(ret_ty.clone());
                }
            }
            ExprKind::ArrayAccess { array, .. } => {
                if let ExprKind::Variable(arr_name) = &array.kind {
                    if let Some(ret_ty) = self.closure_return_types.get(arr_name) {
                        return Ok(ret_ty.clone());
                    }
                }
            }
            ExprKind::Closure { body, .. } => {
                return Ok(infer_return_type_syntactic(body));
            }
            _ => {}
        }
        Ok(PhpType::Int) // fallback for unknown callables
    }
}
