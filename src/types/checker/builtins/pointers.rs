use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

type BuiltinResult = Result<Option<PhpType>, CompileError>;

pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    match name {
        "ptr" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "ptr() takes exactly 1 argument"));
            }
            match &args[0].kind {
                ExprKind::Variable(_) => {
                    checker.infer_type(&args[0], env)?;
                }
                _ => {
                    return Err(CompileError::new(
                        span,
                        "ptr() argument must be a variable",
                    ));
                }
            }
            Ok(Some(PhpType::Pointer(None)))
        }
        "ptr_null" => {
            if !args.is_empty() {
                return Err(CompileError::new(span, "ptr_null() takes 0 arguments"));
            }
            Ok(Some(PhpType::Pointer(None)))
        }
        "ptr_is_null" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "ptr_is_null() takes exactly 1 argument",
                ));
            }
            let ptr_ty = checker.infer_type(&args[0], env)?;
            checker.ensure_pointer_type(&ptr_ty, span, "ptr_is_null()")?;
            Ok(Some(PhpType::Bool))
        }
        "ptr_offset" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "ptr_offset() takes exactly 2 arguments"));
            }
            let ptr_ty = checker.infer_type(&args[0], env)?;
            checker.ensure_pointer_type(&ptr_ty, span, "ptr_offset()")?;
            let offset_ty = checker.infer_type(&args[1], env)?;
            if offset_ty != PhpType::Int {
                return Err(CompileError::new(
                    span,
                    "ptr_offset() second argument must be integer",
                ));
            }
            Ok(Some(ptr_ty))
        }
        "ptr_get" | "ptr_read8" | "ptr_read32" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            let ptr_ty = checker.infer_type(&args[0], env)?;
            checker.ensure_pointer_type(&ptr_ty, span, &format!("{}()", name))?;
            Ok(Some(PhpType::Int))
        }
        "ptr_set" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "ptr_set() takes exactly 2 arguments"));
            }
            let ptr_ty = checker.infer_type(&args[0], env)?;
            checker.ensure_pointer_type(&ptr_ty, span, "ptr_set()")?;
            let value_ty = checker.infer_type(&args[1], env)?;
            checker.ensure_word_pointer_value(&value_ty, span)?;
            Ok(Some(PhpType::Void))
        }
        "ptr_write8" | "ptr_write32" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            let ptr_ty = checker.infer_type(&args[0], env)?;
            checker.ensure_pointer_type(&ptr_ty, span, &format!("{}()", name))?;
            let value_ty = checker.infer_type(&args[1], env)?;
            if value_ty != PhpType::Int {
                return Err(CompileError::new(
                    span,
                    &format!("{}() value must be int", name),
                ));
            }
            Ok(Some(PhpType::Void))
        }
        "ptr_sizeof" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "ptr_sizeof() takes exactly 1 argument",
                ));
            }
            match &args[0].kind {
                ExprKind::StringLiteral(type_name) => {
                    if checker.normalize_pointer_target_type(type_name).is_none() {
                        return Err(CompileError::new(
                            span,
                            &format!("Unknown type for ptr_sizeof(): {}", type_name),
                        ));
                    }
                }
                _ => {
                    return Err(CompileError::new(
                        span,
                        "ptr_sizeof() argument must be a string literal",
                    ));
                }
            }
            Ok(Some(PhpType::Int))
        }
        _ => Ok(None),
    }
}
