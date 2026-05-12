//! Purpose:
//! Type-checks the system PHP builtin family.
//! Validates arity, argument types, warning-producing cases, and inferred return types for direct calls.
//!
//! Called from:
//! - `crate::types::checker::builtins::check_builtin()`
//!
//! Key details:
//! - Signatures, callable aliases, optimizer effects, and codegen builtin dispatch must remain in lockstep.

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
        "time" => {
            if !args.is_empty() {
                return Err(CompileError::new(span, "time() takes no arguments"));
            }
            Ok(Some(PhpType::Int))
        }
        "microtime" => {
            if args.len() > 1 {
                return Err(CompileError::new(span, "microtime() takes 0 or 1 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Float))
        }
        "sleep" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "sleep() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Int))
        }
        "usleep" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "usleep() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Void))
        }
        "getenv" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "getenv() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Str))
        }
        "putenv" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "putenv() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "php_uname" => {
            if args.len() > 1 {
                return Err(CompileError::new(span, "php_uname() takes 0 or 1 arguments"));
            }
            if let Some(arg) = args.first() {
                let ty = checker.infer_type(arg, env)?;
                if ty != PhpType::Str {
                    return Err(CompileError::new(span, "php_uname() argument must be string"));
                }
            }
            Ok(Some(PhpType::Str))
        }
        "phpversion" => {
            if !args.is_empty() {
                return Err(CompileError::new(span, "phpversion() takes no arguments"));
            }
            Ok(Some(PhpType::Str))
        }
        "class_attribute_names" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "class_attribute_names() takes exactly 1 argument",
                ));
            }
            // Resolve at compile time: only string-literal class names are
            // supported in this iteration. Dynamic class names would require
            // a runtime name→class_id lookup table that elephc does not yet
            // expose.
            let arg_ty = checker.infer_type(&args[0], env)?;
            if !matches!(arg_ty, PhpType::Str) {
                return Err(CompileError::new(
                    span,
                    "class_attribute_names() argument must be a string class name",
                ));
            }
            let ExprKind::StringLiteral(class_name) = &args[0].kind else {
                return Err(CompileError::new(
                    span,
                    "class_attribute_names() requires a string literal class name (dynamic lookup is not yet supported)",
                ));
            };
            if !checker.classes.contains_key(class_name.as_str()) {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "class_attribute_names(): undefined class '{}'",
                        class_name
                    ),
                ));
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "class_attribute_args" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "class_attribute_args() takes exactly 2 arguments",
                ));
            }
            let class_arg_ty = checker.infer_type(&args[0], env)?;
            if !matches!(class_arg_ty, PhpType::Str) {
                return Err(CompileError::new(
                    span,
                    "class_attribute_args() first argument must be a string class name",
                ));
            }
            let attr_arg_ty = checker.infer_type(&args[1], env)?;
            if !matches!(attr_arg_ty, PhpType::Str) {
                return Err(CompileError::new(
                    span,
                    "class_attribute_args() second argument must be a string attribute name",
                ));
            }
            let ExprKind::StringLiteral(class_name) = &args[0].kind else {
                return Err(CompileError::new(
                    span,
                    "class_attribute_args() requires a string literal class name (dynamic lookup is not yet supported)",
                ));
            };
            if !matches!(args[1].kind, ExprKind::StringLiteral(_)) {
                return Err(CompileError::new(
                    span,
                    "class_attribute_args() requires a string literal attribute name (dynamic lookup is not yet supported)",
                ));
            }
            if !checker.classes.contains_key(class_name.as_str()) {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "class_attribute_args(): undefined class '{}'",
                        class_name
                    ),
                ));
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Mixed))))
        }
        "class_get_attributes" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "class_get_attributes() takes exactly 1 argument",
                ));
            }
            let arg_ty = checker.infer_type(&args[0], env)?;
            if !matches!(arg_ty, PhpType::Str) {
                return Err(CompileError::new(
                    span,
                    "class_get_attributes() argument must be a string class name",
                ));
            }
            let ExprKind::StringLiteral(class_name) = &args[0].kind else {
                return Err(CompileError::new(
                    span,
                    "class_get_attributes() requires a string literal class name (dynamic lookup is not yet supported)",
                ));
            };
            if !checker.classes.contains_key(class_name.as_str()) {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "class_get_attributes(): undefined class '{}'",
                        class_name
                    ),
                ));
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Object(
                "ReflectionAttribute".to_string(),
            )))))
        }
        "exec" | "shell_exec" | "system" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Str))
        }
        "passthru" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "passthru() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Void))
        }
        "define" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "define() takes exactly 2 arguments"));
            }
            let name_str = match &args[0].kind {
                ExprKind::StringLiteral(s) => s.clone(),
                _ => {
                    return Err(CompileError::new(
                        span,
                        "define() first argument must be a string literal",
                    ));
                }
            };
            let ty = checker.infer_type(&args[1], env)?;
            checker.constants.entry(name_str).or_insert(ty);
            Ok(Some(PhpType::Bool))
        }
        "date" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(span, "date() takes 1 or 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "mktime" => {
            if args.len() != 6 {
                return Err(CompileError::new(span, "mktime() takes exactly 6 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "strtotime" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "strtotime() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Int))
        }
        "json_encode" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "json_encode() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Str))
        }
        "json_decode" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "json_decode() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Str))
        }
        "json_last_error" => {
            if !args.is_empty() {
                return Err(CompileError::new(
                    span,
                    "json_last_error() takes no arguments",
                ));
            }
            Ok(Some(PhpType::Int))
        }
        "preg_match" | "preg_match_all" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "preg_replace" => {
            if args.len() != 3 {
                return Err(CompileError::new(
                    span,
                    "preg_replace() takes exactly 3 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "preg_split" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "preg_split() takes exactly 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        _ => Ok(None),
    }
}
