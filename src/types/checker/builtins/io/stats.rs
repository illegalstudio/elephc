//! Purpose:
//! Type-checks PHP IO builtin stats helpers and signatures.
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

/// Type-checks stat/fstat filesystem builtins: `file_exists`, `is_file`, `is_dir`,
/// `is_readable`, `is_writable`, `is_writeable`, `is_executable`, `is_link`, `filesize`,
/// `filemtime`, `fileatime`, `filectime`, `fileperms`, `fileowner`, `filegroup`, `fileinode`,
/// `filetype`, `clearstatcache`, `stat`, `lstat`, `fstat`, and `linkinfo`.
///
/// Validates argument count for each builtin, infers argument types via `checker.infer_type()`,
/// and returns the PHP return type. For `fstat`, validates the stream resource via
/// `ensure_stream_resource`. Returns `Ok(None)` for unhandled names so the caller can fall through.
///
/// # Arguments
/// - `checker`: mutable checker state for type inference and normalization
/// - `name`: lowercase builtin name (case-insensitive lookup already resolved by caller)
/// - `args`: parsed argument expressions
/// - `span`: source location for error reporting
/// - `env`: current type environment
///
/// # Returns
/// `BuiltinResult` — `Ok(Some(PhpType))` with the PHP return type, or `Ok(None)` if `name`
/// is not a stat/filesystem builtin handled by this module.
pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    match name {
        "file_exists" | "is_file" | "is_dir" | "is_readable" | "is_writable"
        | "is_writeable" | "is_executable" | "is_link" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "filesize" | "filemtime" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Int))
        }
        "disk_free_space" | "disk_total_space" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Float))
        }
        "fileatime" | "filectime" | "fileperms" | "fileowner" | "filegroup" | "fileinode" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(checker.normalize_union_type(vec![PhpType::Int, PhpType::Bool])))
        }
        "filetype" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "filetype() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(checker.normalize_union_type(vec![PhpType::Str, PhpType::Bool])))
        }
        "clearstatcache" => {
            // PHP accepts optional stat-cache arguments; elephc has no stat cache.
            if args.len() > 2 {
                return Err(CompileError::new(
                    span,
                    "clearstatcache() takes at most 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Void))
        }
        "stat" | "lstat" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(stat_result_type(checker)))
        }
        "fstat" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "fstat() takes exactly 1 argument"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            Ok(Some(stat_result_type(checker)))
        }
        "linkinfo" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "linkinfo() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Int))
        }
        _ => Ok(None),
    }
}

/// Returns the normalized return type for `stat()` / `lstat()` / `fstat()`.
///
/// Produces `assoc-array<mixed, int>|bool` as a normalized union type. PHP's stat functions
/// return `array|false` — the AssocArray represents the stat buffer keys (mode, ino, uid, etc.
/// as int values), and `Bool` represents the false return on failure. The `Mixed` key type
/// reflects PHP's heterogeneous array indexing.
///
/// # Arguments
/// - `checker`: checker instance used for `normalize_union_type()` to produce the canonical union
fn stat_result_type(checker: &Checker) -> PhpType {
    checker.normalize_union_type(vec![
        PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Int),
        },
        PhpType::Bool,
    ])
}
