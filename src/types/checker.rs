use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{Expr, Program, Stmt};

#[derive(Debug, Clone, PartialEq)]
pub enum PhpType {
    Int,
    Str,
}

pub fn check_types(program: &Program) -> Result<Program, CompileError> {
    let mut env: HashMap<String, PhpType> = HashMap::new();

    for stmt in program {
        check_stmt(stmt, &mut env)?;
    }

    // For now, return the program as-is (typed AST = AST after validation)
    Ok(program.clone())
}

fn check_stmt(stmt: &Stmt, env: &mut HashMap<String, PhpType>) -> Result<(), CompileError> {
    match stmt {
        Stmt::Echo(expr) => {
            infer_type(expr, env)?;
            Ok(())
        }
        Stmt::Assign { name, value } => {
            let ty = infer_type(value, env)?;
            if let Some(existing) = env.get(name) {
                if *existing != ty {
                    return Err(CompileError::at(
                        0,
                        0,
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
    match expr {
        Expr::StringLiteral(_) => Ok(PhpType::Str),
        Expr::IntLiteral(_) => Ok(PhpType::Int),
        Expr::Variable(name) => env.get(name).cloned().ok_or_else(|| {
            CompileError::at(0, 0, &format!("Undefined variable: ${}", name))
        }),
        Expr::Negate(inner) => {
            let ty = infer_type(inner, env)?;
            if ty != PhpType::Int {
                return Err(CompileError::at(0, 0, "Cannot negate a non-integer"));
            }
            Ok(PhpType::Int)
        }
        Expr::BinaryOp { left, op, right } => {
            use crate::parser::ast::BinOp;
            let lt = infer_type(left, env)?;
            let rt = infer_type(right, env)?;
            match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    if lt != PhpType::Int || rt != PhpType::Int {
                        return Err(CompileError::at(
                            0,
                            0,
                            "Arithmetic operators require integer operands",
                        ));
                    }
                    Ok(PhpType::Int)
                }
                BinOp::Concat => {
                    if lt != PhpType::Str || rt != PhpType::Str {
                        return Err(CompileError::at(
                            0,
                            0,
                            "Concatenation operator requires string operands",
                        ));
                    }
                    Ok(PhpType::Str)
                }
            }
        }
    }
}
