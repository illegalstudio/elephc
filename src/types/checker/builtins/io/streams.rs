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
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::common::{ensure_stream_resource, BuiltinResult};
use super::super::super::Checker;

/// Type-checks stream I/O builtins: `fopen`, `fclose`, `fread`, `fwrite`, `fgets`, `feof`,
/// `readline`, `fseek`, `ftell`, `rewind`, `fgetcsv`, `fputcsv`, `ftruncate`, `fsync`,
/// `fflush`, `fdatasync`, `fgetc`, `fpassthru`, `flock`, and `tmpfile`.
///
/// Matches `name` against known stream builtins, validates arity and argument types via
/// `checker.infer_type()` using `env`, and returns `Some(PhpType)` on match or `None` if
/// `name` is not a recognized stream function. Errors are reported at `span`.
///
/// Notable behaviors: `flock` requires a variable (not arbitrary expression) for its
/// optional third `$would_block` parameter; `fgetc` returns `Str|Bool` on EOF; `fopen`
/// and `tmpfile` return `stream_resource|Bool` to reflect PHP's false-on-failure pattern.
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
        "fgetc" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "fgetc() takes exactly 1 argument"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Str,
                PhpType::Bool,
            ])))
        }
        "fpassthru" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "fpassthru() takes exactly 1 argument",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            Ok(Some(PhpType::Int))
        }
        "flock" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(span, "flock() takes 2 or 3 arguments"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            let operation_ty = checker.infer_type(&args[1], env)?;
            if operation_ty != PhpType::Int {
                return Err(CompileError::new(
                    args[1].span,
                    "flock() operation must be int",
                ));
            }
            if args.len() == 3 && !matches!(args[2].kind, ExprKind::Variable(_)) {
                return Err(CompileError::new(
                    args[2].span,
                    "flock() parameter $would_block must be passed a variable",
                ));
            }
            Ok(Some(PhpType::Bool))
        }
        "tmpfile" => {
            if !args.is_empty() && !is_empty_static_array_spread(args) {
                return Err(CompileError::new(span, "tmpfile() takes no arguments"));
            }
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::stream_resource(),
                PhpType::Bool,
            ])))
        }
        _ => Ok(None),
    }
}

/// Returns `true` if `args` contains exactly one element that is a `...[...]` spread
/// of an empty array literal.
///
/// PHP allows `tmpfile(...[])` as a no-argument call. This helper distinguishes that
/// valid form from `tmpfile()` with zero arguments by checking for a single `Spread`
/// node wrapping an `ArrayLiteral([])`. Returns `false` for all other argument shapes.
fn is_empty_static_array_spread(args: &[Expr]) -> bool {
    let [arg] = args else {
        return false;
    };
    let ExprKind::Spread(inner) = &arg.kind else {
        return false;
    };
    matches!(&inner.kind, ExprKind::ArrayLiteral(items) if items.is_empty())
}
