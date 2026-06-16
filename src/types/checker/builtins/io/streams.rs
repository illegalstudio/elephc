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
use crate::names::php_symbol_key;
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
            if args.len() < 2 || args.len() > 4 {
                return Err(CompileError::new(span, "fopen() takes 2 to 4 arguments"));
            }
            // An `https://` literal pulls in the elephc-tls staticlib at link
            // time. Detect the scheme here so the runner adds -lelephc_tls
            // only for programs that actually open TLS streams.
            if let Some(crate::parser::ast::ExprKind::StringLiteral(s)) =
                args.first().map(|a| &a.kind)
            {
                if s.starts_with("https://") || s.starts_with("ftps://") {
                    checker.require_builtin_library("elephc_tls");
                }
                if s.starts_with("compress.zlib://") {
                    // compress.zlib:// attaches a zlib.inflate filter, which
                    // pulls in libz.
                    checker.require_builtin_library("z");
                }
                if s.starts_with("compress.bzip2://") {
                    // compress.bzip2:// calls libbz2's
                    // BZ2_bzBuffToBuffDecompress at fopen time.
                    checker.require_builtin_library("bz2");
                }
                // phar:// write mode uses the elephc-phar read-modify-write
                // bridge when available and keeps the elephc-crypto SHA1 path
                // as the assembly fallback. Reads need neither write bridge nor
                // crypto here.
                if s.starts_with("phar://") {
                    let write_mode = matches!(
                        args.get(1).map(|a| &a.kind),
                        Some(crate::parser::ast::ExprKind::StringLiteral(m))
                            if matches!(m.as_bytes().first(), Some(b'w') | Some(b'a') | Some(b'c') | Some(b'x'))
                    );
                    if write_mode {
                        checker.require_builtin_library("elephc_phar");
                        checker.require_builtin_library("elephc_crypto");
                    }
                }
            } else {
                // Non-literal paths can route to a phar:// entry at run time
                // for reads or write-mode opens. Reads may use tar/zip and
                // compressed entries through the elephc-phar/zlib/bz2 bridge.
                checker.require_builtin_library("elephc_phar");
                checker.require_builtin_library("z");
                checker.require_builtin_library("bz2");
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            // The 3rd ($use_include_path) and 4th ($context) args are
            // accepted but not yet honored by the codegen / runtime; the
            // global _stream_context_options slot (set by
            // stream_context_create / set_option) is the effective context
            // until the consumer integration lands. Documented v1.5 limit.
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
        "fprintf" => {
            if args.len() < 2 {
                return Err(CompileError::new(span, "fprintf() takes at least 2 arguments"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            for arg in &args[1..] {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "vfprintf" => {
            if args.len() != 3 {
                return Err(CompileError::new(
                    span,
                    "vfprintf() takes exactly 3 arguments (stream, format, values)",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            for arg in &args[1..] {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
        }
        "fscanf" => {
            if args.len() < 2 {
                return Err(CompileError::new(span, "fscanf() takes at least 2 arguments"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            for arg in &args[1..] {
                checker.infer_type(arg, env)?;
            }
            // v1 supports the 2-argument form: returns an array of matched
            // fields (the by-ref output form is not yet supported, mirroring
            // sscanf()).
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "fgets" => {
            if args.len() != 1 {
                return Err(CompileError::new(span, "fgets() takes exactly 1 argument"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            // PHP fgets() returns `string|false`: false on EOF with no bytes
            // read, the line bytes otherwise. The checker exposes this as
            // Mixed so user code that compares against `false` works.
            Ok(Some(PhpType::Mixed))
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
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Str,
                PhpType::Bool,
            ])))
        }
        "fseek" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(span, "fseek() takes 2 or 3 arguments"));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            for arg in args.iter().skip(1) {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Int,
                PhpType::Bool,
            ])))
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
        "stream_isatty" | "stream_supports_lock" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "stream_is_local" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "stream_is_local() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "stream_get_transports" | "stream_get_wrappers" | "stream_get_filters" => {
            if !args.is_empty() {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes no arguments", name),
                ));
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "stream_filter_append" | "stream_filter_prepend" => {
            if args.len() < 2 || args.len() > 4 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes 2 to 4 arguments", name),
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            match &args[1].kind {
                ExprKind::StringLiteral(filter) => {
                    // The zlib.* filters call into the system zlib, so any
                    // program that attaches one must link against libz.
                    if filter.as_str() == "zlib.deflate" || filter.as_str() == "zlib.inflate" {
                        checker.require_builtin_library("z");
                    }
                    // convert.iconv.* uses libc iconv: in libc on Linux
                    // (glibc/musl) but a separate library on macOS, so only
                    // macOS needs explicit -liconv linkage.
                    if filter.starts_with("convert.iconv.") {
                        checker.require_macos_builtin_library("iconv");
                    }
                    // The bzip2.* filters call into libbz2 (BZ2_bz*), so any
                    // program that attaches one must link against -lbz2. The
                    // existing compress.bzip2:// require fires only on the fopen
                    // path, not here, so this is the filter path's own wiring.
                    if filter.as_str() == "bzip2.compress" || filter.as_str() == "bzip2.decompress" {
                        checker.require_builtin_library("bz2");
                    }
                    // Unknown built-in names are routed through the user
                    // filter registry at runtime (Phase 10 tier 3); the
                    // helper returns PHP false for unregistered names.
                }
                _ => {
                    // Dynamic filter names resolve through the user filter
                    // registry at runtime. The codegen pulls the name from
                    // the expression result regs and the helper does the
                    // lookup.
                    checker.infer_type(&args[1], env)?;
                }
            }
            for arg in &args[2..] {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Mixed))
        }
        "stream_filter_remove" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "stream_filter_remove() takes exactly 1 argument",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "stream_get_contents" => {
            if args.is_empty() || args.len() > 3 {
                return Err(CompileError::new(
                    span,
                    "stream_get_contents() takes 1 to 3 arguments",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            if let Some(length) = args.get(1) {
                ensure_optional_int(checker, name, "length", length, env)?;
            }
            if let Some(offset) = args.get(2) {
                ensure_int(checker, name, "offset", offset, env)?;
            }
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Str,
                PhpType::Bool,
            ])))
        }
        "stream_get_meta_data" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "stream_get_meta_data() takes exactly 1 argument",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            Ok(Some(PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Mixed),
            }))
        }
        "stream_copy_to_stream" => {
            if args.len() < 2 || args.len() > 4 {
                return Err(CompileError::new(
                    span,
                    "stream_copy_to_stream() takes 2 to 4 arguments",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            ensure_stream_resource(checker, name, &args[1], env)?;
            if let Some(length) = args.get(2) {
                ensure_optional_int(checker, name, "length", length, env)?;
            }
            if let Some(offset) = args.get(3) {
                ensure_int(checker, name, "offset", offset, env)?;
            }
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Int,
                PhpType::Bool,
            ])))
        }
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
        "stream_wrapper_register" => {
            // stream_wrapper_register(protocol, class[, flags]) records a
            // user-defined wrapper class for `$protocol://...` paths.
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(
                    span,
                    "stream_wrapper_register() takes 2 or 3 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            validate_registered_stream_class(checker, "stream_wrapper_register", &args[1], span)?;
            Ok(Some(PhpType::Bool))
        }
        "stream_wrapper_unregister" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "stream_wrapper_unregister() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "stream_wrapper_restore" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "stream_wrapper_restore() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
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
        "stream_context_create" => {
            if args.len() > 2 {
                return Err(CompileError::new(
                    span,
                    "stream_context_create() takes at most 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::stream_resource()))
        }
        "stream_context_get_default" => {
            if args.len() > 1 {
                return Err(CompileError::new(
                    span,
                    "stream_context_get_default() takes at most 1 argument",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::stream_resource()))
        }
        "stream_context_set_default" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "stream_context_set_default() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::stream_resource()))
        }
        "stream_context_set_params" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "stream_context_set_params() takes exactly 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Bool))
        }
        "stream_resolve_include_path" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "stream_resolve_include_path() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Mixed))
        }
        "stream_context_set_option" => {
            // PHP accepts 2 forms: (ctx, options_array) or (ctx, wrapper,
            // option, value). v1 accepts both shapes inertly.
            if args.len() < 2 || args.len() > 4 {
                return Err(CompileError::new(
                    span,
                    "stream_context_set_option() takes 2 to 4 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Bool))
        }
        "stream_context_get_options" | "stream_context_get_params" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Mixed),
            }))
        }
        "stream_filter_register" => {
            // stream_filter_register(filter_name, class) records a
            // user-defined filter class for stream_filter_append/prepend.
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "stream_filter_register() takes exactly 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            validate_registered_stream_class(checker, "stream_filter_register", &args[1], span)?;
            Ok(Some(PhpType::Bool))
        }
        "stream_bucket_make_writeable" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "stream_bucket_make_writeable() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            // Returns an object (?stdClass-like with data + datalen) or null.
            Ok(Some(PhpType::Mixed))
        }
        "stream_bucket_new" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "stream_bucket_new() takes exactly 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Mixed))
        }
        "stream_bucket_append" | "stream_bucket_prepend" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Void))
        }
        "stream_set_chunk_size" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "stream_set_chunk_size() takes exactly 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            // PHP returns the previous chunk size on success or false on
            // failure; v1 always reports the default chunk size.
            Ok(Some(PhpType::Int))
        }
        "stream_set_read_buffer" | "stream_set_write_buffer" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 2 arguments", name),
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            // PHP returns 0 on success.
            Ok(Some(PhpType::Int))
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
        "stream_get_line" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(
                    span,
                    "stream_get_line() takes 2 or 3 arguments",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            for arg in args.iter().skip(1) {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Str))
        }
        "stream_set_blocking" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "stream_set_blocking() takes exactly 2 arguments",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            checker.infer_type(&args[1], env)?;
            Ok(Some(PhpType::Bool))
        }
        "stream_select" => {
            if args.len() < 4 || args.len() > 5 {
                return Err(CompileError::new(
                    span,
                    "stream_select() takes 4 or 5 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Int))
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
        "stream_set_timeout" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(
                    span,
                    "stream_set_timeout() takes 2 or 3 arguments",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            checker.infer_type(&args[1], env)?;
            if args.len() == 3 {
                checker.infer_type(&args[2], env)?;
            }
            Ok(Some(PhpType::Bool))
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
        "popen" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    span,
                    "popen() takes exactly 2 arguments",
                ));
            }
            checker.infer_type(&args[0], env)?;
            checker.infer_type(&args[1], env)?;
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::stream_resource(),
                PhpType::Bool,
            ])))
        }
        "pclose" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "pclose() takes exactly 1 argument",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            Ok(Some(PhpType::Int))
        }
        "opendir" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "opendir() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::stream_resource(),
                PhpType::Bool,
            ])))
        }
        "readdir" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "readdir() takes exactly 1 argument",
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            Ok(Some(checker.normalize_union_type(vec![
                PhpType::Str,
                PhpType::Bool,
            ])))
        }
        "closedir" | "rewinddir" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    &format!("{}() takes exactly 1 argument", name),
                ));
            }
            ensure_stream_resource(checker, name, &args[0], env)?;
            Ok(Some(PhpType::Void))
        }
        _ => Ok(None),
    }
}

/// Ensures a stream builtin argument is an `int`, emitting a parameter-specific
/// compile error otherwise.
fn ensure_int(
    checker: &mut Checker,
    builtin: &str,
    param: &str,
    arg: &Expr,
    env: &TypeEnv,
) -> Result<(), CompileError> {
    let ty = checker.infer_type(arg, env)?;
    if accepts_int(&ty) {
        return Ok(());
    }
    Err(CompileError::new(
        arg.span,
        &format!("{}() {} must be int", builtin, param),
    ))
}

/// Ensures a stream builtin length argument is `int|null`, matching PHP's
/// nullable `$length` parameter while keeping codegen from seeing strings/floats.
fn ensure_optional_int(
    checker: &mut Checker,
    builtin: &str,
    param: &str,
    arg: &Expr,
    env: &TypeEnv,
) -> Result<(), CompileError> {
    let ty = checker.infer_type(arg, env)?;
    if accepts_int_or_null(&ty) {
        return Ok(());
    }
    Err(CompileError::new(
        arg.span,
        &format!("{}() {} must be int or null", builtin, param),
    ))
}

/// Returns true when a type is statically compatible with an `int` parameter.
fn accepts_int(ty: &PhpType) -> bool {
    match ty {
        PhpType::Int => true,
        PhpType::Union(members) => members.iter().all(accepts_int),
        _ => false,
    }
}

/// Returns true when a type is statically compatible with an `int|null` parameter.
fn accepts_int_or_null(ty: &PhpType) -> bool {
    match ty {
        PhpType::Int | PhpType::Void => true,
        PhpType::Union(members) => members.iter().all(accepts_int_or_null),
        _ => false,
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

/// Validates a literal stream wrapper/filter class name against declared classes.
fn validate_registered_stream_class(
    checker: &Checker,
    builtin: &str,
    class_arg: &Expr,
    span: crate::span::Span,
) -> Result<(), CompileError> {
    let ExprKind::StringLiteral(class_name) = &class_arg.kind else {
        return Ok(());
    };
    if stream_registered_class_exists(checker, class_name) {
        return Ok(());
    }
    Err(CompileError::new(
        span,
        &format!("{}(): undefined class '{}'", builtin, class_name),
    ))
}

/// Returns true when `class_name` exists under PHP's case-insensitive class lookup.
fn stream_registered_class_exists(checker: &Checker, class_name: &str) -> bool {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    checker
        .classes
        .keys()
        .any(|existing| php_symbol_key(existing) == class_key)
}
