//! Purpose:
//! Type-checks PHP IO builtin streams helpers and signatures.
//! Validates arity, argument categories, resource handling, and return types before codegen sees calls.
//!
//! Called from:
//! - `crate::types::checker::builtins::io::check_builtin()`
//!
//! Key details:
//! - Return types and diagnostics must stay aligned with `crate::types::signatures` and builtin codegen emitters.

use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::types::{PhpType, TypeEnv};

use super::common::{ensure_stream_resource, BuiltinResult};
use super::super::super::Checker;

pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    match name {
        "fopen" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "fopen() takes exactly 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::stream_resource(),
                PhpType::Bool,
            ])))
        }
        "fclose" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "fclose() takes exactly 1 argument"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "fread" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "fread() takes exactly 2 arguments"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            checker.infer_type(&args[1], env)?;
            Ok(Some(PhpType::Str))
        }
        "fwrite" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "fwrite() takes exactly 2 arguments"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            checker.infer_type(&args[1], env)?;
            Ok(Some(PhpType::Int))
        }
        "fgets" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "fgets() takes exactly 1 argument"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            Ok(Some(PhpType::Str))
        }
        "feof" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "feof() takes exactly 1 argument"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "readline" => {
            if !args.is_empty() && args.len() > 1 {
                return Err(CompileError::new(span, "readline() takes 0 or 1 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "fseek" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(span, "fseek() takes 2 or 3 arguments"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            for arg in args.iter().skip(1) {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "ftell" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "ftell() takes exactly 1 argument"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            Ok(Some(PhpType::Int))
        }
        "rewind" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "rewind() takes exactly 1 argument"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "fgetcsv" => {
            if args.is_empty() || args.len() > 3 {
                return Err(CompileError::new(span, "fgetcsv() takes 1 to 3 arguments"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            for arg in args.iter().skip(1) {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "fputcsv" => {
            if args.len() < 2 || args.len() > 4 {
                return Err(CompileError::new(span, "fputcsv() takes 2 to 4 arguments"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            for arg in args.iter().skip(1) {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "ftruncate" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "ftruncate() takes exactly 2 arguments",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            checker.infer_type(&args[1], env)?;
            Ok(Some(PhpType::Bool))
        }
        "fsync" | "fflush" | "fdatasync" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        _ => Ok(None),
    }
}
