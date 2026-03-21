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
        ExprKind::BinaryOp { left, op, right } => {
            let lt = infer_type(left, env)?;
            let rt = infer_type(right, env)?;
            match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    if lt != PhpType::Int || rt != PhpType::Int {
                        return Err(CompileError::new(
                            expr.span,
                            "Arithmetic operators require integer operands",
                        ));
                    }
                    Ok(PhpType::Int)
                }
                BinOp::Concat => {
                    // Concat coerces any type to string (like PHP)
                    Ok(PhpType::Str)
                }
            }
        }
    }
}
