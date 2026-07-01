//! Purpose:
//! Type-checks PHP IO builtin streams helpers and signatures.
//! Validates arity, argument categories, resource handling, and return types before codegen sees calls.
//!
//! Called from:
//! - `crate::types::checker::builtins::io::check_builtin()`
//!
//! Key details:
//! - Return types and diagnostics must stay aligned with `crate::types::signatures` and builtin codegen emitters.
//! - The stream_get/set/misc family (io batch F) has been migrated to `src/builtins/io/`; only
//!   socket/network arms remain here for io batch G.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::common::{ensure_stream_resource, BuiltinResult};
use super::super::super::Checker;

/// Type-checks the stream I/O builtins not yet migrated to the `builtin!` registry:
/// the `stream_*` / socket / directory / host-lookup / pipe families. The file-handle
/// read/write/seek/flush/sync/csv/lock/tmp/dir/pipe family (`fopen`, `fclose`, `fread`,
/// `fwrite`, `fprintf`, `vfprintf`, `fscanf`, `fgets`, `feof`, `fseek`, `ftell`, `rewind`,
/// `ftruncate`, `fgetc`, `fpassthru`, `fsync`, `fflush`, `fdatasync`, `fgetcsv`, `fputcsv`,
/// `flock`, `readline`, `tmpfile`, `popen`, `pclose`, `opendir`, `readdir`, `closedir`,
/// `rewinddir`) now lives in `src/builtins/io/` and is dispatched by the registry.
///
/// Matches `name` against known stream builtins, validates arity and argument types via
/// `checker.infer_type()` using `env`, and returns `Some(PhpType)` on match or `None` if
/// `name` is not a recognized stream function. Errors are reported at `span`.
pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    match name {
        "stream_socket_server" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "stream_socket_server() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::stream_resource(),
                PhpType::Bool,
            ])))
        }
        "stream_socket_client" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "stream_socket_client() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::stream_resource(),
                PhpType::Bool,
            ])))
        }
        "stream_socket_enable_crypto" => {
            // The builtin publishes the elephc-tls function pointers and
            // calls into elephc_tls_attach_fd, so programs that invoke it
            // must link against the rustls-backed crate.
            if args.len() < 2 || args.len() > 4 {
                return Err(CompileError::new(
                    span,
                    "stream_socket_enable_crypto() takes 2 to 4 arguments",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            for arg in &args[1..] {
                checker.infer_type(arg, env)?;
            }
            checker.require_builtin_library("elephc_tls");
            Ok(Some(PhpType::Bool))
        }
        "fsockopen" | "pfsockopen" => {
            // [p]fsockopen(hostname, port, &error_code, &error_message, timeout).
            if args.len() < 2 || args.len() > 5 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes 2 to 5 arguments", name),
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            // The error-code and error-message outputs are written by
            // reference, so they must be passed as variables.
            for (idx, label) in [(2usize, "$error_code"), (3usize, "$error_message")] {
                if let Some(arg) = args.get(idx) {
                    if !matches!(arg.kind, ExprKind::Variable(_)) {
                        return Err(CompileError::new(
                            arg.span,
                            &format!(
                                "{}() parameter {} must be passed a variable",
                                name, label
                            ),
                        ));
                    }
                }
            }
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::stream_resource(),
                PhpType::Bool,
            ])))
        }
        "stream_socket_accept" => {
            if args.is_empty() || args.len() > 3 {
                return Err(CompileError::new(
                    span,
                    "stream_socket_accept() takes 1 to 3 arguments",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            if let Some(timeout) = args.get(1) {
                checker.infer_type(timeout, env)?;
            }
            if let Some(peer_arg) = args.get(2) {
                if !matches!(peer_arg.kind, ExprKind::Variable(_)) {
                    return Err(CompileError::new(
                        peer_arg.span,
                        "stream_socket_accept() parameter $peer_name must be passed a variable",
                    ));
                }
            }
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::stream_resource(),
                PhpType::Bool,
            ])))
        }
        "stream_socket_shutdown" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "stream_socket_shutdown() takes exactly 2 arguments",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            checker.infer_type(&args[1], env)?;
            Ok(Some(PhpType::Bool))
        }
        "gethostname" => {
            if !args.is_empty() {
                return Err(CompileError::new(
                    span,
                    "gethostname() takes no arguments",
                ));
            }
            Ok(Some(PhpType::Str))
        }
        "gethostbyname" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "gethostbyname() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Str))
        }
        "gethostbyaddr" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "gethostbyaddr() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Str,
                PhpType::Bool,
            ])))
        }
        "getprotobyname" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "getprotobyname() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Int,
                PhpType::Bool,
            ])))
        }
        "getprotobynumber" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "getprotobynumber() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Str,
                PhpType::Bool,
            ])))
        }
        "getservbyname" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "getservbyname() takes exactly 2 arguments",
                ));
            }
            checker.infer_type(&args[0], env)?;
            checker.infer_type(&args[1], env)?;
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Int,
                PhpType::Bool,
            ])))
        }
        "getservbyport" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "getservbyport() takes exactly 2 arguments",
                ));
            }
            checker.infer_type(&args[0], env)?;
            checker.infer_type(&args[1], env)?;
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Str,
                PhpType::Bool,
            ])))
        }
        "stream_socket_sendto" => {
            if args.len() < 2 || args.len() > 4 {
                return Err(CompileError::new(
                    span,
                    "stream_socket_sendto() takes 2 to 4 arguments",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            for arg in &args[1..] {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Int,
                PhpType::Bool,
            ])))
        }
        "stream_socket_recvfrom" => {
            if args.len() < 2 || args.len() > 4 {
                return Err(CompileError::new(
                    span,
                    "stream_socket_recvfrom() takes 2 to 4 arguments",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            for arg in &args[1..] {
                checker.infer_type(arg, env)?;
            }
            if args.len() == 4 {
                if !matches!(args[3].kind, ExprKind::Variable(_)) {
                    return Err(CompileError::new(
                        args[3].span,
                        "stream_socket_recvfrom() parameter $address must be passed a variable",
                    ));
                }
                let addr_ty = checker.infer_type(&args[3], env)?;
                if !matches!(addr_ty, PhpType::Str) {
                    return Err(CompileError::new(
                        args[3].span,
                        "stream_socket_recvfrom() parameter $address must be a string",
                    ));
                }
            }
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Str,
                PhpType::Bool,
            ])))
        }
        "stream_socket_get_name" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "stream_socket_get_name() takes exactly 2 arguments",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            checker.infer_type(&args[1], env)?;
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Str,
                PhpType::Bool,
            ])))
        }
        "stream_socket_pair" => {
            if args.len() != 3 {
                return Err(CompileError::new(
                    span,
                    "stream_socket_pair() takes exactly 3 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            // PHP returns `array|false`. The builtin emitter widens the
            // success array's slots through __rt_array_to_mixed so the
            // value flows through Mixed pipelines (count, indexing,
            // === comparison) without per-call special-casing. Falling
            // back to Mixed for the static type keeps every consumer
            // happy.
            Ok(Some(PhpType::Mixed))
        }
        _ => Ok(None),
    }
}
