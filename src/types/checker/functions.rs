use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::Checker;

impl Checker {
    fn format_fixed_or_range_arity(min_args: usize, max_args: usize) -> String {
        if min_args == max_args {
            format!("{}", min_args)
        } else {
            format!("{} to {}", min_args, max_args)
        }
    }

    pub fn check_function_call(
        &mut self,
        name: &str,
        args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        // Count non-spread arguments for arity checking
        let effective_arg_count = args.iter().filter(|a| !matches!(a.kind, ExprKind::Spread(_))).count();
        let has_spread = args.iter().any(|a| matches!(a.kind, ExprKind::Spread(_)));

        // Already resolved or being resolved (recursive)?
        if let Some(sig) = self.functions.get(name).cloned() {
            // Count required params (those without defaults)
            let required = sig.defaults.iter().filter(|d| d.is_none()).count();
            if !has_spread {
                if sig.variadic.is_some() {
                    // Variadic: need at least the required regular params
                    if effective_arg_count < required {
                        return Err(CompileError::new(
                            span,
                            &format!(
                                "Function '{}' expects at least {} arguments, got {}",
                                name, required, effective_arg_count
                            ),
                        ));
                    }
                } else if effective_arg_count < required || effective_arg_count > sig.params.len() {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "Function '{}' expects {} arguments, got {}",
                            name,
                            Self::format_fixed_or_range_arity(required, sig.params.len()),
                            effective_arg_count
                        ),
                    ));
                }
            }
            for arg in args {
                self.infer_type(arg, caller_env)?;
            }
            return Ok(sig.return_type);
        }

        let decl = self
            .fn_decls
            .get(name)
            .cloned()
            .ok_or_else(|| CompileError::new(span, &format!("Undefined function: {}", name)))?;

        // Count required params (those without defaults)
        let required = decl.defaults.iter().filter(|d| d.is_none()).count();
        if !has_spread {
            if decl.variadic.is_some() {
                if effective_arg_count < required {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "Function '{}' expects at least {} arguments, got {}",
                            name, required, effective_arg_count
                        ),
                    ));
                }
            } else if effective_arg_count < required || effective_arg_count > decl.params.len() {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Function '{}' expects {} arguments, got {}",
                        name,
                        Self::format_fixed_or_range_arity(required, decl.params.len()),
                        effective_arg_count
                    ),
                ));
            }
        }

        let mut param_types = Vec::new();
        let mut arg_idx = 0;
        for arg in args {
            let ty = self.infer_type(arg, caller_env)?;
            if let ExprKind::Spread(_) = &arg.kind {
                // Spread into non-variadic params: fill all remaining params with element type
                for i in arg_idx..decl.params.len() {
                    param_types.push((decl.params[i].clone(), ty.clone()));
                }
                arg_idx = decl.params.len();
            } else if arg_idx < decl.params.len() {
                param_types.push((decl.params[arg_idx].clone(), ty));
                arg_idx += 1;
            } else {
                arg_idx += 1;
            }
        }
        // Fill in types for params with defaults that aren't explicitly passed
        for i in arg_idx..decl.params.len() {
            if let Some(default_expr) = &decl.defaults[i] {
                let ty = self.infer_type(default_expr, caller_env)?;
                param_types.push((decl.params[i].clone(), ty));
            }
        }

        // Add variadic param as Array type
        if let Some(ref vp) = decl.variadic {
            // Infer variadic element type from excess args
            let variadic_elem_ty = if args.len() > decl.params.len() {
                self.infer_type(&args[decl.params.len()], caller_env).unwrap_or(PhpType::Int)
            } else {
                PhpType::Int
            };
            param_types.push((vp.clone(), PhpType::Array(Box::new(variadic_elem_ty))));
        }

        let mut local_env: TypeEnv = HashMap::new();
        for (pname, pty) in &param_types {
            local_env.insert(pname.clone(), pty.clone());
        }

        // Provisional signature for recursive calls
        let provisional_sig = FunctionSig {
            params: param_types.clone(),
            defaults: decl.defaults.clone(),
            return_type: PhpType::Int,
            ref_params: decl.ref_params.clone(),
            variadic: decl.variadic.clone(),
        };
        self.functions.insert(name.to_string(), provisional_sig);

        let mut return_type = PhpType::Void;
        let mut all_return_types: Vec<PhpType> = Vec::new();
        for stmt in &decl.body {
            self.check_stmt(stmt, &mut local_env)?;
            if let Some(rt) = self.find_return_type(stmt, &local_env) {
                all_return_types.push(rt);
            }
        }

        // Pick the widest return type across all branches
        if !all_return_types.is_empty() {
            return_type = all_return_types[0].clone();
            for rt in &all_return_types[1..] {
                return_type = Self::wider_type(&return_type, rt);
            }
        }

        let sig = FunctionSig {
            params: param_types,
            defaults: decl.defaults.clone(),
            return_type: return_type.clone(),
            ref_params: decl.ref_params.clone(),
            variadic: decl.variadic.clone(),
        };
        self.functions.insert(name.to_string(), sig);

        Ok(return_type)
    }

    pub fn find_return_type(&mut self, stmt: &Stmt, env: &TypeEnv) -> Option<PhpType> {
        let mut types = Vec::new();
        self.collect_return_types(stmt, env, &mut types);
        if types.is_empty() {
            return None;
        }
        // Pick the widest type: Str > Float > Int/Bool/Void
        let mut widest = types[0].clone();
        for ty in &types[1..] {
            widest = Self::wider_type(&widest, ty);
        }
        Some(widest)
    }

    fn collect_return_types(&mut self, stmt: &Stmt, env: &TypeEnv, types: &mut Vec<PhpType>) {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                if let Ok(ty) = self.infer_type(expr, env) {
                    types.push(ty);
                }
            }
            StmtKind::Return(None) => {
                types.push(PhpType::Void);
            }
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                for s in then_body { self.collect_return_types(s, env, types); }
                for (_, body) in elseif_clauses {
                    for s in body { self.collect_return_types(s, env, types); }
                }
                if let Some(body) = else_body {
                    for s in body { self.collect_return_types(s, env, types); }
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                for s in body { self.collect_return_types(s, env, types); }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    for s in body { self.collect_return_types(s, env, types); }
                }
                if let Some(body) = default {
                    for s in body { self.collect_return_types(s, env, types); }
                }
            }
            _ => {}
        }
    }

    fn wider_type(a: &PhpType, b: &PhpType) -> PhpType {
        // Str is the widest, then Float, then Int/Bool
        match (a, b) {
            _ if a == b => a.clone(),
            (PhpType::Str, _) | (_, PhpType::Str) => PhpType::Str,
            (PhpType::Float, _) | (_, PhpType::Float) => PhpType::Float,
            (PhpType::Void, other) | (other, PhpType::Void) => other.clone(),
            _ => a.clone(),
        }
    }
}
