//! Purpose:
//! Type-checks the numeric PHP builtin family.
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

/// Type-checks numeric and language-construct PHP builtins.
///
/// Validates argument count, argument types, and special cases (e.g., `buffer_free`
/// restriction on `$this`, locals-only) for the builtin functions in the numeric
/// family. Returns the inferred `PhpType` on success, or a `CompileError` on type/
/// arity mismatch.
///
/// ## Supported builtins
/// - Control: `exit`, `die`, `empty`
/// - Unset: `unset`
/// - Buffers: `buffer_len`, `buffer_free`
///
/// ## Arguments
/// - `checker`: mutable checker state for inference
/// - `name`: lowercase builtin name (case-insensitive lookup is handled by caller)
/// - `args`: parsed argument expressions
/// - `span`: source span for error reporting
/// - `env`: current type environment
///
/// ## Returns
/// `Ok(Some(PhpType))` with the inferred return type, `Ok(None)` for unknown builtins
/// (caller falls through), or `Err(CompileError)` on validation failure.
pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    match name {
        "exit" | "die" => {
            if args.len() > 1 {
                return Err(CompileError::new(span, "exit() takes 0 or 1 arguments"));
            }
            if let Some(arg) = args.first() {
                // PHP: an integer argument is the process/exit status; a string
                // argument is printed (as if echo'd) before exiting with status 0.
                let ty = checker.infer_type(arg, env)?;
                if !matches!(ty, PhpType::Int | PhpType::Bool | PhpType::Str) {
                    return Err(CompileError::new(
                        span,
                        "exit() argument must be an integer status or a string message",
                    ));
                }
            }
            Ok(Some(PhpType::Void))
        }
        "empty" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "empty() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "unset" => {
            if args.is_empty() {
                return Err(CompileError::new(span, "unset() takes at least 1 argument"));
            }
            for arg in args {
                // `unset($obj->prop)` on an undeclared property dispatches to
                // `__unset`; the helper infers the receiver but skips the bare
                // property access that would otherwise reject the property.
                if checker
                    .isset_unset_property_magic_class(arg, "__unset", env)?
                    .is_some()
                {
                    continue;
                }
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Void))
        }
        "buffer_len" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "buffer_len() takes exactly 1 argument"));
            }
            let ty = checker.infer_type(&args[0], env)?;
            if !matches!(ty, PhpType::Buffer(_)) {
                return Err(CompileError::new(
                    span,
                    "buffer_len() argument must be buffer<T>",
                ));
            }
            Ok(Some(PhpType::Int))
        }
        "buffer_free" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "buffer_free() takes exactly 1 argument"));
            }
            match &args[0].kind {
                ExprKind::Variable(name) => {
                    if checker.current_class.is_some() && name == "this" {
                        return Err(CompileError::new(span, "buffer_free() cannot free $this"));
                    }
                    if checker.active_ref_params.contains(name)
                        || checker.active_globals.contains(name)
                        || checker.active_statics.contains(name)
                    {
                        return Err(CompileError::new(
                            span,
                            "buffer_free() argument must be a local variable",
                        ));
                    }
                }
                _ => {
                    let ty = checker.infer_type(&args[0], env)?;
                    if !matches!(ty, PhpType::Buffer(_)) {
                        return Err(CompileError::new(
                            span,
                            "buffer_free() argument must be buffer<T>",
                        ));
                    }
                    return Err(CompileError::new(
                        span,
                        "buffer_free() argument must be a local variable",
                    ));
                }
            }
            let ty = checker.infer_type(&args[0], env)?;
            if !matches!(ty, PhpType::Buffer(_)) {
                return Err(CompileError::new(
                    span,
                    "buffer_free() argument must be buffer<T>",
                ));
            }
            Ok(Some(PhpType::Void))
        }
        _ => Ok(None),
    }
}
