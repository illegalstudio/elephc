use crate::errors::CompileError;
use crate::parser::ast::Expr;
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
        "strlen" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "strlen() takes exactly 1 argument"));
            }
            let ty = checker.infer_type(&args[0], env)?;
            if ty != PhpType::Str {
                return Err(CompileError::new(span, "strlen() argument must be string"));
            }
            Ok(Some(PhpType::Int))
        }
        "intval" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "intval() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Int))
        }
        "substr" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(span, "substr() takes 2 or 3 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "strpos" | "strrpos" => {
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
        "strstr" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "strstr() takes exactly 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "strtolower" | "strtoupper" | "ucfirst" | "lcfirst" | "ucwords" | "trim"
        | "ltrim" | "rtrim" | "strrev" | "str_repeat" | "str_replace" | "str_ireplace"
        | "chr" | "addslashes" | "stripslashes" | "nl2br" | "bin2hex" => {
            let expected = match name {
                "str_repeat" => 2,
                "str_replace" | "str_ireplace" => 3,
                _ => 1,
            };
            if name == "chr" {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "chr() takes exactly 1 argument"));
                }
            } else if name == "trim" || name == "ltrim" || name == "rtrim" {
                if args.is_empty() || args.len() > 2 {
                    return Err(CompileError::new(
                        span,
                        &format!("{}() takes 1 or 2 arguments", name),
                    ));
                }
            } else if args.len() != expected {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "{}() takes exactly {} argument{}",
                        name,
                        expected,
                        if expected > 1 { "s" } else { "" }
                    ),
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "hex2bin" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "hex2bin() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Str))
        }
        "substr_replace" => {
            if args.len() != 3 && args.len() != 4 {
                return Err(CompileError::new(
                    span,
                    "substr_replace() takes 3 or 4 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "str_pad" => {
            if args.len() < 2 || args.len() > 4 {
                return Err(CompileError::new(span, "str_pad() takes 2 to 4 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "str_split" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(span, "str_split() takes 1 or 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "wordwrap" => {
            if args.is_empty() || args.len() > 4 {
                return Err(CompileError::new(span, "wordwrap() takes 1 to 4 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "ord" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "ord() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Int))
        }
        "strcmp" | "strcasecmp" => {
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
        "str_contains" | "str_starts_with" | "str_ends_with" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Bool))
        }
        "explode" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "explode() takes exactly 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "implode" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "implode() takes exactly 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "sprintf" => {
            if args.is_empty() {
                return Err(CompileError::new(span, "sprintf() requires at least 1 argument"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "printf" => {
            if args.is_empty() {
                return Err(CompileError::new(span, "printf() requires at least 1 argument"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "hash" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "hash() takes exactly 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            checker.require_linux_builtin_library("crypto");
            Ok(Some(PhpType::Str))
        }
        "sscanf" => {
            if args.len() < 2 {
                return Err(CompileError::new(span, "sscanf() takes at least 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "md5" | "sha1" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            checker.require_linux_builtin_library("crypto");
            Ok(Some(PhpType::Str))
        }
        "htmlspecialchars" | "htmlentities" | "html_entity_decode" | "urlencode"
        | "urldecode" | "rawurlencode" | "rawurldecode" | "base64_encode"
        | "base64_decode" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Str))
        }
        "ctype_alpha" | "ctype_digit" | "ctype_alnum" | "ctype_space" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        _ => Ok(None),
    }
}
