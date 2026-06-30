//! Purpose:
//! Type-checks the strings PHP builtin family.
//! Validates arity, argument types, warning-producing cases, and inferred return types for direct calls.
//!
//! Called from:
//! - `crate::types::checker::builtins::check_builtin()`
//!
//! Key details:
//! - Signatures, callable aliases, optimizer effects, and codegen builtin dispatch must remain in lockstep.

use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

type BuiltinResult = Result<Option<PhpType>, CompileError>;

/// Type-checks a string builtin call, validating arity, argument types, and return type.
///
/// Dispatches on `name` to validate the call and infer the return `PhpType`.
/// Calls `checker.infer_type()` on each argument to propagate type constraints.
/// The `hash_init` arm records a library requirement for the elephc-crypto bridge.
///
/// Returns `Ok(Some(PhpType))` with the inferred return type, `Ok(None)` for unknown
/// builtins (caller will fall through to other handlers), or `Err(CompileError)` on
/// arity/type mismatch.
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
            // Accept Str, Mixed, and Union types — PHP's strlen() coerces its
            // argument to a string per the standard PHP type juggling rules
            // (numbers become their decimal representation, true → "1",
            // false/null → ""). Mixed inputs flow through __rt_mixed_strlen
            // at codegen time which reads the cell tag and returns the
            // length of the coerced representation.
            if !matches!(ty, PhpType::Str | PhpType::Mixed | PhpType::Union(_)) {
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
        "str_replace" | "str_ireplace" => {
            let expected = 3;
            if args.len() != expected {
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
        "wordwrap" => {
            if args.is_empty() || args.len() > 4 {
                return Err(CompileError::new(span, "wordwrap() takes 1 to 4 arguments"));
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
        "vsprintf" | "vprintf" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments (format, values)", name),
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            // vsprintf returns the formatted string; vprintf prints it and
            // returns the number of bytes written.
            Ok(Some(if name == "vsprintf" {
                PhpType::Str
            } else {
                PhpType::Int
            }))
        }
        "hash_init" => {
            // HASH_HMAC streaming mode (flags/key) is not supported; use hash_hmac().
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "hash_init() flags/HASH_HMAC streaming mode is not supported; use hash_hmac() for HMAC",
                ));
            }
            checker.infer_type(&args[0], env)?;
            checker.require_builtin_library("elephc_crypto");
            Ok(Some(PhpType::Mixed))
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
        _ => Ok(None),
    }
}
