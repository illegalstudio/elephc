use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::Checker;

impl Checker {
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
                            "Function '{}' expects {} to {} arguments, got {}",
                            name, required, sig.params.len(), effective_arg_count
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
                        "Function '{}' expects {} to {} arguments, got {}",
                        name, required, decl.params.len(), effective_arg_count
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
        for stmt in &decl.body {
            self.check_stmt(stmt, &mut local_env)?;
            if let Some(rt) = self.find_return_type(stmt, &local_env) {
                return_type = rt;
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
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => self.infer_type(expr, env).ok(),
            StmtKind::Return(None) => Some(PhpType::Void),
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                for s in then_body {
                    if let Some(t) = self.find_return_type(s, env) { return Some(t); }
                }
                for (_, body) in elseif_clauses {
                    for s in body {
                        if let Some(t) = self.find_return_type(s, env) { return Some(t); }
                    }
                }
                if let Some(body) = else_body {
                    for s in body {
                        if let Some(t) = self.find_return_type(s, env) { return Some(t); }
                    }
                }
                None
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. } => {
                for s in body {
                    if let Some(t) = self.find_return_type(s, env) { return Some(t); }
                }
                None
            }
            _ => None,
        }
    }
}
