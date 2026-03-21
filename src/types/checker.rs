use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{BinOp, Expr, ExprKind, Program, Stmt, StmtKind};
use crate::types::{CheckResult, FunctionSig, PhpType, TypeEnv};

struct Checker {
    fn_decls: HashMap<String, FnDecl>,
    functions: HashMap<String, FunctionSig>,
}

#[derive(Clone)]
struct FnDecl {
    params: Vec<String>,
    body: Vec<Stmt>,
}

pub fn check_types(program: &Program) -> Result<CheckResult, CompileError> {
    let mut checker = Checker {
        fn_decls: HashMap::new(),
        functions: HashMap::new(),
    };

    // Pass 1: collect function declarations
    for stmt in program {
        if let StmtKind::FunctionDecl { name, params, body } = &stmt.kind {
            checker.fn_decls.insert(
                name.clone(),
                FnDecl {
                    params: params.clone(),
                    body: body.clone(),
                },
            );
        }
    }

    // Pass 2: type-check global statements
    let mut global_env: TypeEnv = HashMap::new();
    for stmt in program {
        checker.check_stmt(stmt, &mut global_env)?;
    }

    Ok(CheckResult {
        program: program.clone(),
        global_env,
        functions: checker.functions,
    })
}

impl Checker {
    fn check_stmt(&mut self, stmt: &Stmt, env: &mut TypeEnv) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::Echo(expr) => {
                self.infer_type(expr, env)?;
                Ok(())
            }
            StmtKind::Assign { name, value } => {
                let ty = self.infer_type(value, env)?;
                if let Some(existing) = env.get(name) {
                    if *existing != ty {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!(
                                "Type error: cannot reassign ${} from {:?} to {:?}",
                                name, existing, ty
                            ),
                        ));
                    }
                } else {
                    env.insert(name.clone(), ty);
                }
                Ok(())
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                self.infer_type(condition, env)?;
                for s in then_body {
                    self.check_stmt(s, env)?;
                }
                for (cond, body) in elseif_clauses {
                    self.infer_type(cond, env)?;
                    for s in body {
                        self.check_stmt(s, env)?;
                    }
                }
                if let Some(body) = else_body {
                    for s in body {
                        self.check_stmt(s, env)?;
                    }
                }
                Ok(())
            }
            StmtKind::While { condition, body } => {
                self.infer_type(condition, env)?;
                for s in body {
                    self.check_stmt(s, env)?;
                }
                Ok(())
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(s) = init {
                    self.check_stmt(s, env)?;
                }
                if let Some(c) = condition {
                    self.infer_type(c, env)?;
                }
                if let Some(s) = update {
                    self.check_stmt(s, env)?;
                }
                for s in body {
                    self.check_stmt(s, env)?;
                }
                Ok(())
            }
            StmtKind::Break | StmtKind::Continue => Ok(()),
            StmtKind::ExprStmt(expr) => {
                self.infer_type(expr, env)?;
                Ok(())
            }
            StmtKind::FunctionDecl { .. } => Ok(()),
            StmtKind::Return(expr) => {
                if let Some(e) = expr {
                    self.infer_type(e, env)?;
                }
                Ok(())
            }
        }
    }

    fn infer_type(&mut self, expr: &Expr, env: &TypeEnv) -> Result<PhpType, CompileError> {
        match &expr.kind {
            ExprKind::StringLiteral(_) => Ok(PhpType::Str),
            ExprKind::IntLiteral(_) => Ok(PhpType::Int),
            ExprKind::Variable(name) => env.get(name).cloned().ok_or_else(|| {
                CompileError::new(expr.span, &format!("Undefined variable: ${}", name))
            }),
            ExprKind::Negate(inner) => {
                let ty = self.infer_type(inner, env)?;
                if ty != PhpType::Int {
                    return Err(CompileError::new(expr.span, "Cannot negate a non-integer"));
                }
                Ok(PhpType::Int)
            }
            ExprKind::PreIncrement(name)
            | ExprKind::PostIncrement(name)
            | ExprKind::PreDecrement(name)
            | ExprKind::PostDecrement(name) => match env.get(name) {
                Some(PhpType::Int) => Ok(PhpType::Int),
                Some(other) => Err(CompileError::new(
                    expr.span,
                    &format!("Cannot increment/decrement ${} of type {:?}", name, other),
                )),
                None => Err(CompileError::new(
                    expr.span,
                    &format!("Undefined variable: ${}", name),
                )),
            },
            ExprKind::FunctionCall { name, args } => {
                let name = name.clone();
                let args = args.clone();
                self.check_function_call(&name, &args, expr.span, env)
            }
            ExprKind::BinaryOp { left, op, right } => {
                let lt = self.infer_type(left, env)?;
                let rt = self.infer_type(right, env)?;
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                        if lt != PhpType::Int || rt != PhpType::Int {
                            return Err(CompileError::new(
                                expr.span,
                                "Arithmetic operators require integer operands",
                            ));
                        }
                        Ok(PhpType::Int)
                    }
                    BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::LtEq
                    | BinOp::GtEq => {
                        if lt != PhpType::Int || rt != PhpType::Int {
                            return Err(CompileError::new(
                                expr.span,
                                "Comparison operators require integer operands",
                            ));
                        }
                        Ok(PhpType::Int)
                    }
                    BinOp::Concat => Ok(PhpType::Str),
                }
            }
        }
    }

    fn check_function_call(
        &mut self,
        name: &str,
        args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        // Already resolved or being resolved (recursive)?
        if let Some(sig) = self.functions.get(name).cloned() {
            if sig.params.len() != args.len() {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Function '{}' expects {} arguments, got {}",
                        name,
                        sig.params.len(),
                        args.len()
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
                            i + 1,
                            sig.params[i].1,
                            arg_ty
                        ),
                    ));
                }
            }
            return Ok(sig.return_type);
        }

        // Look up declaration
        let decl = self
            .fn_decls
            .get(name)
            .cloned()
            .ok_or_else(|| {
                CompileError::new(span, &format!("Undefined function: {}", name))
            })?;

        if decl.params.len() != args.len() {
            return Err(CompileError::new(
                span,
                &format!(
                    "Function '{}' expects {} arguments, got {}",
                    name,
                    decl.params.len(),
                    args.len()
                ),
            ));
        }

        // Infer parameter types from arguments
        let mut param_types = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let ty = self.infer_type(arg, caller_env)?;
            param_types.push((decl.params[i].clone(), ty));
        }

        // Create local environment with parameters
        let mut local_env: TypeEnv = HashMap::new();
        for (pname, pty) in &param_types {
            local_env.insert(pname.clone(), pty.clone());
        }

        // Insert a provisional signature to handle recursive calls.
        // Return type defaults to Int; will be updated after body analysis.
        let provisional_sig = FunctionSig {
            params: param_types.clone(),
            return_type: PhpType::Int,
        };
        self.functions.insert(name.to_string(), provisional_sig);

        // Type-check function body
        let mut return_type = PhpType::Void;
        for stmt in &decl.body {
            self.check_stmt(stmt, &mut local_env)?;
            if let Some(rt) = self.find_return_type(stmt, &local_env) {
                return_type = rt;
            }
        }

        // Store signature
        let sig = FunctionSig {
            params: param_types,
            return_type: return_type.clone(),
        };
        self.functions.insert(name.to_string(), sig);

        Ok(return_type)
    }

    fn find_return_type(&mut self, stmt: &Stmt, env: &TypeEnv) -> Option<PhpType> {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => self.infer_type(expr, env).ok(),
            StmtKind::Return(None) => Some(PhpType::Void),
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                for s in then_body {
                    if let Some(t) = self.find_return_type(s, env) {
                        return Some(t);
                    }
                }
                for (_, body) in elseif_clauses {
                    for s in body {
                        if let Some(t) = self.find_return_type(s, env) {
                            return Some(t);
                        }
                    }
                }
                if let Some(body) = else_body {
                    for s in body {
                        if let Some(t) = self.find_return_type(s, env) {
                            return Some(t);
                        }
                    }
                }
                None
            }
            StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
                for s in body {
                    if let Some(t) = self.find_return_type(s, env) {
                        return Some(t);
                    }
                }
                None
            }
            _ => None,
        }
    }
}
