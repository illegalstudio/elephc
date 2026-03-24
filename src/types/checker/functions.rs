use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{Expr, Stmt, StmtKind};
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
        // Already resolved or being resolved (recursive)?
        if let Some(sig) = self.functions.get(name).cloned() {
            // Count required params (those without defaults)
            let required = sig.defaults.iter().filter(|d| d.is_none()).count();
            if args.len() < required || args.len() > sig.params.len() {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Function '{}' expects {} to {} arguments, got {}",
                        name, required, sig.params.len(), args.len()
                    ),
                ));
            }
            for (i, arg) in args.iter().enumerate() {
                let arg_ty = self.infer_type(arg, caller_env)?;
                if arg_ty != sig.params[i].1 {
                    return Err(CompileError::new(
                        arg.span,
                        &format!(
                            "Argument {} type mismatch: expected {:?}, got {:?}",
                            i + 1, sig.params[i].1, arg_ty
                        ),
                    ));
                }
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
        if args.len() < required || args.len() > decl.params.len() {
            return Err(CompileError::new(
                span,
                &format!(
                    "Function '{}' expects {} to {} arguments, got {}",
                    name, required, decl.params.len(), args.len()
                ),
            ));
        }

        let mut param_types = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let ty = self.infer_type(arg, caller_env)?;
            param_types.push((decl.params[i].clone(), ty));
        }
        // Fill in types for params with defaults that aren't explicitly passed
        for i in args.len()..decl.params.len() {
            if let Some(default_expr) = &decl.defaults[i] {
                let ty = self.infer_type(default_expr, caller_env)?;
                param_types.push((decl.params[i].clone(), ty));
            }
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
