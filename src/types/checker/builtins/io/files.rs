//! Purpose:
//! Type-checks PHP IO builtin files helpers and signatures.
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

use super::common::BuiltinResult;
use super::super::super::Checker;

/// Type-checks a filesystem builtin call by name, validating argument count, argument
/// categories, and return type. Delegates to `check_touch` for `touch()` timestamp validation.
///
/// Returns `Ok(Some(PhpType))` with the return type on recognized builtins,
/// `Ok(None)` when `name` is not a filesystem builtin (caller should try the next
/// builtin category), or `Err(CompileError)` on arity/type mismatch.
pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    match name {
        "file_get_contents" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "file_get_contents() takes exactly 1 argument",
                ));
            }
            // A literal https:///ftps:// URL is read at run time over TLS.
            // Non-literal paths route through the runtime URL dispatcher, so
            // conservatively link TLS plus the PHAR bridge/decompression
            // libraries because the scheme and PHAR entry flags are unknown.
            if let Some(crate::parser::ast::ExprKind::StringLiteral(url)) =
                args.first().map(|a| &a.kind)
            {
                if url.starts_with("https://") || url.starts_with("ftps://") {
                    checker.require_builtin_library("elephc_tls");
                }
            } else {
                checker.require_builtin_library("elephc_tls");
                checker.require_builtin_library("elephc_phar");
                checker.require_builtin_library("z");
                checker.require_builtin_library("bz2");
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Union(vec![PhpType::Str, PhpType::Bool])))
        }
        "hash_file" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(span, "hash_file() takes 2 or 3 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            // hash_file() reads the file then hashes through elephc-crypto (full
            // algorithm set, raw $binary output, catchable ValueError); returns
            // the digest string or false when the file cannot be read.
            checker.require_builtin_library("elephc_crypto");
            Ok(Some(PhpType::Union(vec![PhpType::Str, PhpType::Bool])))
        }
        "file_put_contents" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "file_put_contents() takes exactly 2 arguments",
                ));
            }
            // file_put_contents("phar://...") writes through the elephc-phar
            // read-modify-write bridge, with the assembly SHA1 path retained as
            // a fallback when the bridge slot is not published.
            if let Some(crate::parser::ast::ExprKind::StringLiteral(url)) =
                args.first().map(|a| &a.kind)
            {
                if url.starts_with("phar://") {
                    checker.require_builtin_library("elephc_phar");
                    checker.require_builtin_library("elephc_crypto");
                }
            } else {
                checker.require_builtin_library("elephc_phar");
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "__elephc_phar_set_compression" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "__elephc_phar_set_compression() takes exactly 2 arguments",
                ));
            }
            checker.require_builtin_library("elephc_phar");
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Bool))
        }
        "__elephc_phar_list_entries" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "__elephc_phar_list_entries() takes exactly 1 argument",
                ));
            }
            checker.require_builtin_library("elephc_phar");
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "__elephc_phar_get_metadata" | "__elephc_phar_get_stub" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "__elephc_phar_get_metadata()/__elephc_phar_get_stub() take exactly 1 argument",
                ));
            }
            checker.require_builtin_library("elephc_phar");
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "__elephc_phar_set_metadata" | "__elephc_phar_set_stub" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "__elephc_phar_set_metadata()/__elephc_phar_set_stub() take exactly 2 arguments",
                ));
            }
            checker.require_builtin_library("elephc_phar");
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Bool))
        }
        "__elephc_phar_get_file_metadata" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "__elephc_phar_get_file_metadata() takes exactly 1 argument",
                ));
            }
            checker.require_builtin_library("elephc_phar");
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "__elephc_phar_set_file_metadata" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "__elephc_phar_set_file_metadata() takes exactly 2 arguments",
                ));
            }
            checker.require_builtin_library("elephc_phar");
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Bool))
        }
        "__elephc_phar_gzip_archive"
        | "__elephc_phar_bzip2_archive"
        | "__elephc_phar_decompress_archive"
        | "__elephc_phar_get_signature_hash"
        | "__elephc_phar_get_signature_type" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "phar archive (de)compression/signature-read intrinsics take exactly 1 argument",
                ));
            }
            checker.require_builtin_library("elephc_phar");
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "__elephc_phar_sign_openssl" | "__elephc_phar_sign_hash" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "phar signing intrinsics take exactly 2 arguments",
                ));
            }
            checker.require_builtin_library("elephc_phar");
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Bool))
        }
        "__elephc_phar_set_zip_password" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "__elephc_phar_set_zip_password takes exactly 1 argument",
                ));
            }
            checker.require_builtin_library("elephc_phar");
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "file" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "file() takes exactly 1 argument"));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "copy" | "rename" => {
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
        "unlink" | "mkdir" | "rmdir" | "chdir" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            if name == "unlink" {
                if let Some(crate::parser::ast::ExprKind::StringLiteral(url)) =
                    args.first().map(|a| &a.kind)
                {
                    if url.starts_with("phar://") {
                        checker.require_builtin_library("elephc_phar");
                    }
                } else {
                    checker.require_builtin_library("elephc_phar");
                }
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "scandir" | "glob" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "getcwd" => {
            if !args.is_empty() {
                return Err(CompileError::new(span, "getcwd() takes no arguments"));
            }
            Ok(Some(PhpType::Str))
        }
        "tempnam" => {
            if args.len() != 2 {
                return Err(CompileError::new(span, "tempnam() takes exactly 2 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "sys_get_temp_dir" => {
            if !args.is_empty() {
                return Err(CompileError::new(
                    span,
                    "sys_get_temp_dir() takes no arguments",
                ));
            }
            Ok(Some(PhpType::Str))
        }
        "chmod" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            let mode_ty = checker.infer_type(&args[1], env)?;
            if mode_ty != PhpType::Int {
                return Err(CompileError::new(args[1].span, "chmod() mode must be int"));
            }
            Ok(Some(PhpType::Bool))
        }
        "chown" | "chgrp" | "lchown" | "lchgrp" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            let principal_ty = checker.infer_type(&args[1], env)?;
            if !matches!(principal_ty, PhpType::Int | PhpType::Str) {
                return Err(CompileError::new(
                    args[1].span,
                    &format!("{}() owner/group must be int or string", name),
                ));
            }
            Ok(Some(PhpType::Bool))
        }
        "umask" => {
            if args.len() > 1 {
                return Err(CompileError::new(span, "umask() takes 0 or 1 arguments"));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "touch" => check_touch(checker, args, span, env).map(Some),
        "readfile" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "readfile() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Int,
                PhpType::Bool,
            ])))
        }
        "symlink" | "link" => {
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
        "readlink" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "readlink() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Str,
                PhpType::Bool,
            ])))
        }
        _ => Ok(None),
    }
}

/// Validates `touch()` arity (1–3 args) and timestamp argument types.
/// Timestamp args must be `int` (a Unix timestamp) or `null` (omit to use current time).
///
/// # Errors
/// Returns an error if:
/// - Arity is 0 or greater than 3
/// - Any timestamp arg is neither `int` nor `null`
/// - `atime` is `null` but `mtime` is non-null (atime implies current time, so mtime cannot be set separately)
///
/// # Returns
/// `Ok(PhpType::Bool)` on success.
fn check_touch(
    checker: &mut Checker,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> Result<PhpType, CompileError> {
    if args.is_empty() || args.len() > 3 {
        return Err(CompileError::new(span, "touch() takes 1, 2, or 3 arguments"));
    }
    checker.infer_type(&args[0], env)?;
    let mut timestamp_types = Vec::new();
    for arg in args.iter().skip(1) {
        let ty = checker.infer_type(arg, env)?;
        if !matches!(ty, PhpType::Int | PhpType::Void) {
            return Err(CompileError::new(
                arg.span,
                "touch() timestamp arguments must be int or null",
            ));
        }
        timestamp_types.push(ty);
    }
    if matches!(timestamp_types.first(), Some(PhpType::Void))
        && matches!(timestamp_types.get(1), Some(ty) if !matches!(ty, PhpType::Void))
    {
        return Err(CompileError::new(
            span,
            "touch() mtime cannot be null when atime is provided",
        ));
    }
    Ok(PhpType::Bool)
}
