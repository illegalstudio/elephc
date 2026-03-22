use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::types::{PhpType, TypeEnv};

use super::Checker;

impl Checker {
    pub fn check_builtin(
        &mut self,
        name: &str,
        args: &[Expr],
        span: crate::span::Span,
        env: &TypeEnv,
    ) -> Result<Option<PhpType>, CompileError> {
        match name {
            "exit" | "die" => {
                if args.len() > 1 {
                    return Err(CompileError::new(span, "exit() takes 0 or 1 arguments"));
                }
                if let Some(arg) = args.first() {
                    let ty = self.infer_type(arg, env)?;
                    if ty != PhpType::Int {
                        return Err(CompileError::new(span, "exit() argument must be integer"));
                    }
                }
                Ok(Some(PhpType::Void))
            }
            "strlen" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "strlen() takes exactly 1 argument"));
                }
                let ty = self.infer_type(&args[0], env)?;
                if ty != PhpType::Str {
                    return Err(CompileError::new(span, "strlen() argument must be string"));
                }
                Ok(Some(PhpType::Int))
            }
            "intval" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "intval() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Int))
            }
            "is_null" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "is_null() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Int))
            }
            "count" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "count() takes exactly 1 argument"));
                }
                let ty = self.infer_type(&args[0], env)?;
                if !matches!(ty, PhpType::Array(_)) {
                    return Err(CompileError::new(span, "count() argument must be array"));
                }
                Ok(Some(PhpType::Int))
            }
            "array_pop" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "array_pop() takes exactly 1 argument"));
                }
                let ty = self.infer_type(&args[0], env)?;
                match ty {
                    PhpType::Array(elem_ty) => Ok(Some(*elem_ty)),
                    _ => Err(CompileError::new(span, "array_pop() argument must be array")),
                }
            }
            "in_array" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "in_array() takes exactly 2 arguments"));
                }
                self.infer_type(&args[0], env)?;
                let arr_ty = self.infer_type(&args[1], env)?;
                if !matches!(arr_ty, PhpType::Array(_)) {
                    return Err(CompileError::new(span, "in_array() second argument must be array"));
                }
                Ok(Some(PhpType::Int))
            }
            "array_keys" | "array_values" => {
                if args.len() != 1 {
                    return Err(CompileError::new(
                        span, &format!("{}() takes exactly 1 argument", name),
                    ));
                }
                let ty = self.infer_type(&args[0], env)?;
                match (name, &ty) {
                    ("array_keys", PhpType::Array(_)) => {
                        Ok(Some(PhpType::Array(Box::new(PhpType::Int))))
                    }
                    ("array_values", PhpType::Array(elem_ty)) => {
                        Ok(Some(PhpType::Array(elem_ty.clone())))
                    }
                    _ => Err(CompileError::new(
                        span, &format!("{}() argument must be array", name),
                    )),
                }
            }
            "sort" | "rsort" => {
                if args.len() != 1 {
                    return Err(CompileError::new(
                        span, &format!("{}() takes exactly 1 argument", name),
                    ));
                }
                let ty = self.infer_type(&args[0], env)?;
                if !matches!(ty, PhpType::Array(_)) {
                    return Err(CompileError::new(
                        span, &format!("{}() argument must be array", name),
                    ));
                }
                Ok(Some(PhpType::Void))
            }
            "isset" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "isset() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Int))
            }
            "array_push" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "array_push() takes exactly 2 arguments"));
                }
                let arr_ty = self.infer_type(&args[0], env)?;
                let val_ty = self.infer_type(&args[1], env)?;
                if let PhpType::Array(elem_ty) = arr_ty {
                    if *elem_ty != val_ty {
                        return Err(CompileError::new(span, "array_push() type mismatch"));
                    }
                } else {
                    return Err(CompileError::new(span, "array_push() first argument must be array"));
                }
                Ok(Some(PhpType::Void))
            }
            _ => Ok(None),
        }
    }
}
