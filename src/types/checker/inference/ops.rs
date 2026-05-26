//! Purpose:
//! Infers ops type-system behavior.
//! Converts AST forms into `PhpType` facts used by validation, warnings, and codegen metadata.
//!
//! Called from:
//! - `crate::types::checker::inference`
//!
//! Key details:
//! - PHP compatibility matters for coercions, operator results, object access, and nullable/union handling.

use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{
    BinOp, CallableTarget, Expr, ExprKind, InstanceOfTarget, StaticReceiver, Stmt, TypeExpr,
};
use crate::types::{merge_array_key_types, FunctionSig, PhpType, TypeEnv};

use super::super::Checker;
use super::syntactic::infer_return_type_syntactic;

impl Checker {
    /// Infers the result type of a binary operator expression.
    ///
    /// Validates operand types according to PHP operator semantics (e.g., numeric
    /// operands for arithmetic, integer operands for bitwise). Returns the `PhpType`
    /// of the result or a compile error if operands are incompatible.
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
                let lt_ok = is_numeric_operand_type(self, &lt);
                let rt_ok = is_numeric_operand_type(self, &rt);
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
                let lt_ok = is_numeric_operand_type(self, &lt);
                let rt_ok = is_numeric_operand_type(self, &rt);
                if !lt_ok || !rt_ok {
                    return Err(CompileError::new(
                        expr.span,
                        "Arithmetic operators require numeric operands",
                    ));
                }
                if uses_mixed_numeric_dispatch(&lt) || uses_mixed_numeric_dispatch(&rt) {
                    Ok(PhpType::Mixed)
                } else if lt == PhpType::Float || rt == PhpType::Float {
                    Ok(PhpType::Float)
                } else {
                    Ok(PhpType::Int)
                }
            }
            BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                let lt_ok = is_numeric_operand_type(self, &lt);
                let rt_ok = is_numeric_operand_type(self, &rt);
                if !lt_ok || !rt_ok {
                    return Err(CompileError::new(
                        expr.span,
                        "Arithmetic operators require numeric operands",
                    ));
                }
                // Division always returns float (PHP compat: 10/3 → 3.333...)
                if *op == BinOp::Div || lt == PhpType::Float || rt == PhpType::Float {
                    Ok(PhpType::Float)
                } else if matches!(op, BinOp::Sub | BinOp::Mul)
                    && (uses_mixed_numeric_dispatch(&lt) || uses_mixed_numeric_dispatch(&rt))
                {
                    Ok(PhpType::Mixed)
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
                let lt_ok = is_numeric_operand_type(self, &lt);
                let rt_ok = is_numeric_operand_type(self, &rt);
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
                let lt_ok = is_integer_operand_type(self, &lt);
                let rt_ok = is_integer_operand_type(self, &rt);
                if !lt_ok || !rt_ok {
                    return Err(CompileError::new(
                        expr.span,
                        "Bitwise operators require integer operands",
                    ));
                }
                Ok(PhpType::Int)
            }
            BinOp::Spaceship => {
                let lt_ok = is_numeric_operand_type(self, &lt);
                let rt_ok = is_numeric_operand_type(self, &rt);
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

    /// Merges two array-like types for the `+` operator (array union).
    ///
    /// Handles `PhpType::Array` vs `PhpType::Array`, `PhpType::AssocArray` vs
    /// `PhpType::AssocArray`, and cross-typed combinations. Produces a merged
    /// element type or an error if the array kinds are incompatible.
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
            (PhpType::Array(left_elem), PhpType::AssocArray { key, value }) => {
                let value = self
                    .merge_array_element_type(left_elem, value)
                    .unwrap_or(PhpType::Mixed);
                Ok(PhpType::AssocArray {
                    key: Box::new(merge_array_key_types(PhpType::Int, *key.clone())),
                    value: Box::new(value),
                })
            }
            (PhpType::AssocArray { key, value }, PhpType::Array(right_elem)) => {
                let value = self
                    .merge_array_element_type(value, right_elem)
                    .unwrap_or(PhpType::Mixed);
                Ok(PhpType::AssocArray {
                    key: Box::new(merge_array_key_types(*key.clone(), PhpType::Int)),
                    value: Box::new(value),
                })
            }
            _ => Err(CompileError::new(
                expr.span,
                "Array union requires both operands to be arrays",
            )),
        }
    }

    /// Infers the result type of an `instanceof` expression.
    ///
    /// Validates the target class name resolves correctly (`self`, `parent`,
    /// `static`, or a concrete class). Always returns `PhpType::Bool`.
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

    /// Resolves the class name for an `instanceof` target.
    ///
    /// Rewrites `self`, `parent`, and `static` to their concrete class names
    /// in the current context. Returns the class name string or an error if
    /// used outside a class context or if `parent` has no parent class.
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

    /// Infers the type of a closure expression, including captured environment.
    ///
    /// Builds a closure signature from params, variadic, and return type. Checks
    /// the closure body with a local storage context that includes reference
    /// parameters and capture refs. Returns `PhpType::Callable`.
    pub(crate) fn infer_closure_type(
        &mut self,
        params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
        variadic: &Option<String>,
        return_type: &Option<TypeExpr>,
        body: &[Stmt],
        captures: &[String],
        capture_refs: &[String],
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
        let mut closure_ref_params: Vec<String> = params
            .iter()
            .filter(|(_, _, _, is_ref)| *is_ref)
            .map(|(name, _, _, _)| name.clone())
            .collect();
        closure_ref_params.extend(capture_refs.iter().cloned());
        self.with_local_storage_context(closure_ref_params, |checker| {
            for stmt in body {
                checker.check_stmt(stmt, &mut closure_sig.env)?;
            }
            Ok(())
        })?;
        self.resolve_closure_return_type(body, return_type, expr.span, &closure_sig.env)?;
        Ok(PhpType::Callable)
    }

    /// Infers the return type of a variable callable call: `$var(...)`.
    ///
    /// Looks up the variable's type in `env`, validates it is `PhpType::Callable`
    /// or an invokable object, then dispatches to signature specialization and
    /// `check_known_callable_call`. Falls back to the closure return type or
    /// `PhpType::Int` if the callable signature is unknown.
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
            if let Some(class_name) = self.invokable_class_for_type(&var_ty) {
                if self
                    .classes
                    .get(&class_name)
                    .is_some_and(|class_info| class_info.methods.contains_key("__invoke"))
                {
                    return self.infer_method_call_on_class_type(
                        &class_name,
                        "__invoke",
                        args,
                        expr,
                        env,
                    );
                }
            }
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
            let specialized_sig = self.specialize_callable_var_sig_from_args(
                var,
                sig,
                args,
                expr.span,
                env,
                &format!("callable ${}", var),
            )?;
            return self.check_known_callable_call(
                &specialized_sig,
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

    /// Infers the return type of an arbitrary expression callable call: `expr(...)`.
    ///
    /// Handles variable callables, first-class callables, closures, and
    /// invokable objects. Complex callees (closures with captures, captured
    /// callables) may require a runtime capture and are rejected if unsupported.
    /// Returns the specialized return type, possibly wrapped in a nullable union.
    pub(crate) fn infer_expr_call_type(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let callee_ty = self.infer_type(callee, env)?;
        if let Some(class_name) = self.invokable_class_for_type(&callee_ty) {
            if self
                .classes
                .get(&class_name)
                .is_some_and(|class_info| class_info.methods.contains_key("__invoke"))
            {
                return self.infer_method_call_on_class_type(
                    &class_name,
                    "__invoke",
                    args,
                    expr,
                    env,
                );
            }
        }
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
        if self.expr_call_complex_callee_needs_runtime_capture(callee) {
            return Err(CompileError::new(
                expr.span,
                "Direct calls of complex captured callable expressions are not supported yet",
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
                    let specialized_sig = self.specialize_callable_var_sig_from_args(
                        var_name,
                        sig,
                        args,
                        expr.span,
                        env,
                        &format!("callable ${}", var_name),
                    )?;
                    let ret_ty = self.check_known_callable_call(
                        &specialized_sig,
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

    /// Returns the class name if `ty` is an invokable object or a single-member
    /// object union; otherwise returns `None`.
    ///
    /// Used to detect `__invoke` on class-type values during callable inference.
    pub(crate) fn invokable_class_for_type(&self, ty: &PhpType) -> Option<String> {
        match ty {
            PhpType::Object(class_name) => Some(class_name.clone()),
            PhpType::Union(members) => {
                let mut class_name = None;
                for member in members {
                    match member {
                        PhpType::Void => {}
                        PhpType::Object(candidate) => {
                            if class_name
                                .as_ref()
                                .is_some_and(|existing: &String| existing != candidate)
                            {
                                return None;
                            }
                            class_name = Some(candidate.clone());
                        }
                        _ => return None,
                    }
                }
                class_name
            }
            _ => None,
        }
    }

    /// Wraps `ret_ty` in a nullable union if the callable result is nullable
    /// (e.g., from a nullsafe chain). Otherwise returns `ret_ty` unchanged.
    fn nullable_callable_result(&self, ret_ty: PhpType, nullable_callable: bool) -> PhpType {
        if nullable_callable {
            self.normalize_union_type(vec![ret_ty, PhpType::Void])
        } else {
            ret_ty
        }
    }

    /// Returns `true` if the callee expression would require a runtime capture
    /// to be passed to `__rt_call` because the callable cannot be materialized
    /// as a direct symbol address (e.g., closures with captures, captured callables).
    pub(crate) fn expr_call_complex_callee_needs_runtime_capture(&self, callee: &Expr) -> bool {
        match &callee.kind {
            ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_) | ExprKind::Variable(_) => {
                false
            }
            ExprKind::Assignment { value, .. } => self.expr_produces_captured_callable(value),
            ExprKind::Ternary {
                then_expr,
                else_expr,
                ..
            } => {
                self.expr_produces_captured_callable(then_expr)
                    || self.expr_produces_captured_callable(else_expr)
            }
            ExprKind::ShortTernary { value, default }
            | ExprKind::NullCoalesce { value, default } => {
                self.expr_produces_captured_callable(value)
                    || self.expr_produces_captured_callable(default)
            }
            _ => false,
        }
    }

    /// Returns `true` if `expr` evaluates to a callable that needs a runtime
    /// capture (closure with captures, first-class callable with method/receiver,
    /// variable with capture refs).
    fn expr_produces_captured_callable(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Closure { captures, .. } => !captures.is_empty(),
            ExprKind::FirstClassCallable(target) => {
                Self::first_class_callable_target_needs_runtime_capture(target)
            }
            ExprKind::Variable(var_name) => {
                self.callable_captures
                    .get(var_name)
                    .is_some_and(|captures| !captures.is_empty())
                    || self
                        .first_class_callable_targets
                        .get(var_name)
                        .is_some_and(Self::first_class_callable_target_needs_runtime_capture)
            }
            ExprKind::Assignment { value, .. } => self.expr_produces_captured_callable(value),
            ExprKind::Ternary {
                then_expr,
                else_expr,
                ..
            } => {
                self.expr_produces_captured_callable(then_expr)
                    || self.expr_produces_captured_callable(else_expr)
            }
            ExprKind::ShortTernary { value, default }
            | ExprKind::NullCoalesce { value, default } => {
                self.expr_produces_captured_callable(value)
                    || self.expr_produces_captured_callable(default)
            }
            _ => false,
        }
    }

    /// Returns `true` if a first-class callable target requires a runtime capture
    /// (instance method or static method with static receiver).
    fn first_class_callable_target_needs_runtime_capture(target: &CallableTarget) -> bool {
        matches!(
            target,
            CallableTarget::Method { .. }
                | CallableTarget::StaticMethod {
                    receiver: StaticReceiver::Static,
                    ..
                }
        )
    }

    /// Type-checks the PHP 8.5 pipe operator: `value |> callable`.
    ///
    /// Semantics: `callable` must evaluate to a callable, and the result is
    /// computed by invoking it with `value` as its only positional argument.
    /// By-reference parameters on the callable are rejected per the RFC.
    pub(crate) fn infer_pipe_type(
        &mut self,
        value: &Expr,
        callable: &Expr,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        // Evaluate the LHS — any type is acceptable as the piped value.
        let _value_ty = self.infer_type(value, env)?;

        // The RHS must be a callable.
        let callable_ty = self.infer_type(callable, env)?;
        if callable_ty != PhpType::Callable {
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "Pipe operator right-hand side must be a callable, got {:?}",
                    callable_ty
                ),
            ));
        }

        // Synthesize the equivalent call: `callable(value)`.
        let synth_args = vec![value.clone()];

        // Resolve the callable's signature using the same dispatch as ExprCall.
        match &callable.kind {
            ExprKind::Variable(var_name) => {
                if let Some(sig) = self.callable_sigs.get(var_name).cloned() {
                    if let Some(target) = self
                        .first_class_callable_targets
                        .get(var_name)
                        .cloned()
                    {
                        let specialized_sig = self.specialize_first_class_callable_target(
                            &target,
                            &synth_args,
                            expr.span,
                            env,
                        )?;
                        return self.check_pipe_known_callable_call(
                            &specialized_sig,
                            &synth_args,
                            expr.span,
                            env,
                            &format!("pipe target ${}", var_name),
                        );
                    }
                    return self.check_pipe_known_callable_call(
                        &sig,
                        &synth_args,
                        expr.span,
                        env,
                        &format!("pipe target ${}", var_name),
                    );
                }
            }
            ExprKind::FirstClassCallable(target) => {
                let sig = self.specialize_first_class_callable_target(
                    target,
                    &synth_args,
                    expr.span,
                    env,
                )?;
                return self.check_pipe_known_callable_call(
                    &sig,
                    &synth_args,
                    expr.span,
                    env,
                    "pipe target",
                );
            }
            ExprKind::Closure { .. } => {
                if let Some(sig) = self.resolve_expr_callable_sig(callable, env)? {
                    return self.check_pipe_known_callable_call(
                        &sig,
                        &synth_args,
                        expr.span,
                        env,
                        "pipe target",
                    );
                }
            }
            _ => {}
        }

        // No statically-known signature — fall back to syntactic return-type inference
        // (matches the unknown-callable path in infer_expr_call_type).
        match &callable.kind {
            ExprKind::Closure { body, .. } => {
                return Ok(infer_return_type_syntactic(body));
            }
            ExprKind::Variable(var_name) => {
                if let Some(ret_ty) = self.closure_return_types.get(var_name) {
                    return Ok(ret_ty.clone());
                }
            }
            _ => {}
        }
        Ok(PhpType::Int)
    }

    /// Validates and delegates to `check_known_callable_call` for pipe operator
    /// calls. Rejects signatures with by-reference parameters per the RFC.
    fn check_pipe_known_callable_call(
        &mut self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        env: &TypeEnv,
        callee_desc: &str,
    ) -> Result<PhpType, CompileError> {
        if sig.ref_params.iter().any(|is_ref| *is_ref) {
            return Err(CompileError::new(
                span,
                "Pipe operator does not support by-reference parameters",
            ));
        }
        self.check_known_callable_call(sig, args, span, env, callee_desc)
    }

    /// Specializes a callable variable's signature by inferring actual argument
    /// types and updating `sig.params` entries that are `PhpType::Mixed` with
    /// the inferred type. Stores the updated signature and return type in
    /// `closure_return_types` and `callable_sigs` if anything changed.
    fn specialize_callable_var_sig_from_args(
        &mut self,
        var: &str,
        mut sig: FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        env: &TypeEnv,
        callee_desc: &str,
    ) -> Result<FunctionSig, CompileError> {
        let normalized_args = self.normalize_named_call_args(&sig, args, span, callee_desc, env)?;
        let regular_param_count = crate::types::call_args::regular_param_count(&sig);
        let mut changed = false;
        let mut param_idx = 0usize;
        for arg in &normalized_args {
            let actual_ty = self.infer_type(arg, env)?;
            if matches!(arg.kind, ExprKind::Spread(_)) {
                continue;
            }
            if param_idx < regular_param_count
                && !sig
                    .declared_params
                    .get(param_idx)
                    .copied()
                    .unwrap_or(false)
                && !sig.ref_params.get(param_idx).copied().unwrap_or(false)
                && sig.params[param_idx].1 == PhpType::Mixed
                && actual_ty != PhpType::Never
            {
                sig.params[param_idx].1 = actual_ty;
                changed = true;
            }
            param_idx += 1;
        }
        if changed {
            self.closure_return_types
                .insert(var.to_string(), sig.return_type.clone());
            self.callable_sigs.insert(var.to_string(), sig.clone());
        }
        Ok(sig)
    }

    /// Returns `true` if `callee_ty` is a union containing both `Callable` and
    /// `Void` and the callee expression contains a nullsafe member access.
    fn is_nullable_callable_from_nullsafe_chain(callee: &Expr, callee_ty: &PhpType) -> bool {
        let PhpType::Union(members) = callee_ty else {
            return false;
        };
        members.iter().any(|member| *member == PhpType::Callable)
            && members.iter().any(|member| *member == PhpType::Void)
            && expr_contains_nullsafe_member(callee)
    }
}

/// Returns `true` if `expr` contains a nullsafe member access anywhere in
/// its subtree (nullsafe property, dynamic property, method call).
fn expr_contains_nullsafe_member(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::NullsafePropertyAccess { .. }
        | ExprKind::NullsafeDynamicPropertyAccess { .. }
        | ExprKind::NullsafeMethodCall { .. } => true,
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::DynamicPropertyAccess { object, .. }
        | ExprKind::MethodCall { object, .. } => expr_contains_nullsafe_member(object),
        ExprKind::ArrayAccess { array, .. } => expr_contains_nullsafe_member(array),
        ExprKind::ExprCall { callee, .. } => expr_contains_nullsafe_member(callee),
        _ => false,
    }
}

/// Returns `true` if `ty` is an array-like type (flat `Array` or `AssocArray`).
fn is_array_like_type(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. })
}

/// Returns `true` if `ty` is a valid operand type for numeric binary operators
/// (addition, subtraction, multiplication, division, modulo, comparison, spaceship).
/// Numeric operands include `Int`, `Float`, `Bool`, `Void`, `Mixed`, or a union
/// with mixed integer dispatch behavior.
fn is_numeric_operand_type(checker: &Checker, ty: &PhpType) -> bool {
    matches!(
        ty,
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void | PhpType::Mixed
    ) || checker.is_union_with_mixed_int_dispatch(ty)
}

/// Returns `true` if `ty` is a valid operand type for bitwise binary operators.
/// Accepts `Int`, `Bool`, `Void`, `Mixed`, or a union with mixed integer dispatch.
fn is_integer_operand_type(checker: &Checker, ty: &PhpType) -> bool {
    matches!(
        ty,
        PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Mixed
    ) || checker.is_union_with_mixed_int_dispatch(ty)
}

/// Returns `true` if `ty` uses mixed numeric dispatch — i.e., the result type
/// cannot be narrowed to a single concrete numeric type at compile time.
fn uses_mixed_numeric_dispatch(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Mixed | PhpType::Union(_))
}

/// Returns `true` if `expr` is an empty array literal (`[]`).
fn is_empty_indexed_array_literal(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::ArrayLiteral(elems) if elems.is_empty())
}
