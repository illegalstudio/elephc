mod builtins;
mod functions;

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{BinOp, Expr, ExprKind, Program, Stmt, StmtKind};
use crate::types::{CheckResult, FunctionSig, PhpType, TypeEnv};

pub(crate) struct Checker {
    pub fn_decls: HashMap<String, FnDecl>,
    pub functions: HashMap<String, FunctionSig>,
}

#[derive(Clone)]
pub(crate) struct FnDecl {
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
}

pub fn check_types(program: &Program) -> Result<CheckResult, CompileError> {
    let mut checker = Checker {
        fn_decls: HashMap::new(),
        functions: HashMap::new(),
    };

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

    let mut global_env: TypeEnv = HashMap::new();
    global_env.insert("argc".to_string(), PhpType::Int);
    global_env.insert("argv".to_string(), PhpType::Array(Box::new(PhpType::Str)));
    for stmt in program {
        checker.check_stmt(stmt, &mut global_env)?;
    }

    Ok(CheckResult {
        global_env,
        functions: checker.functions,
    })
}

impl Checker {
    pub fn check_stmt(&mut self, stmt: &Stmt, env: &mut TypeEnv) -> Result<(), CompileError> {
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
            StmtKind::ArrayAssign { array, index, value } => {
                let arr_ty = env.get(array).cloned().ok_or_else(|| {
                    CompileError::new(stmt.span, &format!("Undefined variable: ${}", array))
                })?;
                self.infer_type(index, env)?;
                let val_ty = self.infer_type(value, env)?;
                if let PhpType::Array(elem_ty) = arr_ty {
                    if *elem_ty != val_ty {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!(
                                "Array element type mismatch: expected {:?}, got {:?}",
                                elem_ty, val_ty
                            ),
                        ));
                    }
                }
                Ok(())
            }
            StmtKind::ArrayPush { array, value } => {
                let arr_ty = env.get(array).cloned().ok_or_else(|| {
                    CompileError::new(stmt.span, &format!("Undefined variable: ${}", array))
                })?;
                let val_ty = self.infer_type(value, env)?;
                if let PhpType::Array(elem_ty) = arr_ty {
                    if *elem_ty != val_ty {
                        return Err(CompileError::new(stmt.span, "Array push type mismatch"));
                    }
                }
                Ok(())
            }
            StmtKind::Foreach { array, value_var, body } => {
                let arr_ty = self.infer_type(array, env)?;
                if let PhpType::Array(elem_ty) = arr_ty {
                    env.insert(value_var.clone(), *elem_ty);
                } else {
                    return Err(CompileError::new(stmt.span, "foreach requires an array"));
                }
                for s in body {
                    self.check_stmt(s, env)?;
                }
                Ok(())
            }
            StmtKind::If {
                condition, then_body, elseif_clauses, else_body,
            } => {
                self.infer_type(condition, env)?;
                for s in then_body { self.check_stmt(s, env)?; }
                for (cond, body) in elseif_clauses {
                    self.infer_type(cond, env)?;
                    for s in body { self.check_stmt(s, env)?; }
                }
                if let Some(body) = else_body {
                    for s in body { self.check_stmt(s, env)?; }
                }
                Ok(())
            }
            StmtKind::DoWhile { body, condition } => {
                for s in body { self.check_stmt(s, env)?; }
                self.infer_type(condition, env)?;
                Ok(())
            }
            StmtKind::While { condition, body } => {
                self.infer_type(condition, env)?;
                for s in body { self.check_stmt(s, env)?; }
                Ok(())
            }
            StmtKind::For { init, condition, update, body } => {
                if let Some(s) = init { self.check_stmt(s, env)?; }
                if let Some(c) = condition { self.infer_type(c, env)?; }
                if let Some(s) = update { self.check_stmt(s, env)?; }
                for s in body { self.check_stmt(s, env)?; }
                Ok(())
            }
            StmtKind::Break | StmtKind::Continue => Ok(()),
            StmtKind::ExprStmt(expr) => {
                self.infer_type(expr, env)?;
                Ok(())
            }
            StmtKind::FunctionDecl { .. } => Ok(()),
            StmtKind::Return(expr) => {
                if let Some(e) = expr { self.infer_type(e, env)?; }
                Ok(())
            }
        }
    }

    pub fn infer_type(&mut self, expr: &Expr, env: &TypeEnv) -> Result<PhpType, CompileError> {
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
            ExprKind::Not(inner) => {
                self.infer_type(inner, env)?;
                Ok(PhpType::Int)
            }
            ExprKind::PreIncrement(name) | ExprKind::PostIncrement(name)
            | ExprKind::PreDecrement(name) | ExprKind::PostDecrement(name) => {
                match env.get(name) {
                    Some(PhpType::Int) => Ok(PhpType::Int),
                    Some(other) => Err(CompileError::new(
                        expr.span,
                        &format!("Cannot increment/decrement ${} of type {:?}", name, other),
                    )),
                    None => Err(CompileError::new(
                        expr.span, &format!("Undefined variable: ${}", name),
                    )),
                }
            }
            ExprKind::ArrayLiteral(elems) => {
                if elems.is_empty() {
                    return Err(CompileError::new(
                        expr.span, "Cannot infer type of empty array literal",
                    ));
                }
                let first_ty = self.infer_type(&elems[0], env)?;
                for elem in &elems[1..] {
                    let ty = self.infer_type(elem, env)?;
                    if ty != first_ty {
                        return Err(CompileError::new(
                            elem.span,
                            &format!("Array element type mismatch: expected {:?}, got {:?}", first_ty, ty),
                        ));
                    }
                }
                Ok(PhpType::Array(Box::new(first_ty)))
            }
            ExprKind::ArrayAccess { array, index } => {
                let arr_ty = self.infer_type(array, env)?;
                let idx_ty = self.infer_type(index, env)?;
                if idx_ty != PhpType::Int {
                    return Err(CompileError::new(expr.span, "Array index must be integer"));
                }
                match arr_ty {
                    PhpType::Array(elem_ty) => Ok(*elem_ty),
                    _ => Err(CompileError::new(expr.span, "Cannot index non-array")),
                }
            }
            ExprKind::Ternary { condition, then_expr, else_expr } => {
                self.infer_type(condition, env)?;
                let then_ty = self.infer_type(then_expr, env)?;
                let else_ty = self.infer_type(else_expr, env)?;
                if then_ty != else_ty {
                    return Err(CompileError::new(
                        expr.span,
                        &format!("Ternary branches must have the same type: {:?} vs {:?}", then_ty, else_ty),
                    ));
                }
                Ok(then_ty)
            }
            ExprKind::FunctionCall { name, args } => {
                let name = name.clone();
                let args = args.clone();
                if let Some(ty) = self.check_builtin(&name, &args, expr.span, env)? {
                    return Ok(ty);
                }
                self.check_function_call(&name, &args, expr.span, env)
            }
            ExprKind::BinaryOp { left, op, right } => {
                let lt = self.infer_type(left, env)?;
                let rt = self.infer_type(right, env)?;
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                        if lt != PhpType::Int || rt != PhpType::Int {
                            return Err(CompileError::new(
                                expr.span, "Arithmetic operators require integer operands",
                            ));
                        }
                        Ok(PhpType::Int)
                    }
                    BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt
                    | BinOp::LtEq | BinOp::GtEq => {
                        if lt != PhpType::Int || rt != PhpType::Int {
                            return Err(CompileError::new(
                                expr.span, "Comparison operators require integer operands",
                            ));
                        }
                        Ok(PhpType::Int)
                    }
                    BinOp::Concat => Ok(PhpType::Str),
                    BinOp::And | BinOp::Or => Ok(PhpType::Int),
                }
            }
        }
    }
}
