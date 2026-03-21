use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{BinOp, Expr, ExprKind, Program, Stmt, StmtKind};
use crate::types::{PhpType, TypeEnv};

pub fn check_types(program: &Program) -> Result<(Program, TypeEnv), CompileError> {
    let mut env: HashMap<String, PhpType> = HashMap::new();

    for stmt in program {
        check_stmt(stmt, &mut env)?;
    }

    Ok((program.clone(), env))
}

fn check_stmt(stmt: &Stmt, env: &mut HashMap<String, PhpType>) -> Result<(), CompileError> {
    match &stmt.kind {
        StmtKind::Echo(expr) => {
            infer_type(expr, env)?;
            Ok(())
        }
        StmtKind::Assign { name, value } => {
            let ty = infer_type(value, env)?;
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
            infer_type(condition, env)?;
            for s in then_body {
                check_stmt(s, env)?;
            }
            for (cond, body) in elseif_clauses {
                infer_type(cond, env)?;
                for s in body {
                    check_stmt(s, env)?;
                }
            }
            if let Some(body) = else_body {
                for s in body {
                    check_stmt(s, env)?;
                }
            }
            Ok(())
        }
        StmtKind::While { condition, body } => {
            infer_type(condition, env)?;
            for s in body {
                check_stmt(s, env)?;
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
                check_stmt(s, env)?;
            }
            if let Some(c) = condition {
                infer_type(c, env)?;
            }
            if let Some(s) = update {
                check_stmt(s, env)?;
            }
            for s in body {
                check_stmt(s, env)?;
            }
            Ok(())
        }
        StmtKind::Break | StmtKind::Continue => Ok(()),
        StmtKind::ExprStmt(expr) => {
            infer_type(expr, env)?;
            Ok(())
        }
    }
}

fn infer_type(expr: &Expr, env: &HashMap<String, PhpType>) -> Result<PhpType, CompileError> {
    match &expr.kind {
        ExprKind::StringLiteral(_) => Ok(PhpType::Str),
        ExprKind::IntLiteral(_) => Ok(PhpType::Int),
        ExprKind::Variable(name) => env.get(name).cloned().ok_or_else(|| {
            CompileError::new(expr.span, &format!("Undefined variable: ${}", name))
        }),
        ExprKind::Negate(inner) => {
            let ty = infer_type(inner, env)?;
            if ty != PhpType::Int {
                return Err(CompileError::new(
                    expr.span,
                    "Cannot negate a non-integer",
                ));
            }
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
                    expr.span,
                    &format!("Undefined variable: ${}", name),
                )),
            }
        }
        ExprKind::BinaryOp { left, op, right } => {
            let lt = infer_type(left, env)?;
            let rt = infer_type(right, env)?;
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
                BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => {
                    if lt != PhpType::Int || rt != PhpType::Int {
                        return Err(CompileError::new(
                            expr.span,
                            "Comparison operators require integer operands",
                        ));
                    }
                    Ok(PhpType::Int) // 0 or 1
                }
                BinOp::Concat => Ok(PhpType::Str),
            }
        }
    }
}
