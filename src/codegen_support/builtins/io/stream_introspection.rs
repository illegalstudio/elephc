//! Purpose:
//! Emits the PHP stream-introspection stub builtins `stream_is_local`,
//! `stream_supports_lock`, `stream_get_wrappers`, `stream_get_transports`,
//! and `stream_get_filters`.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Argument expressions are still evaluated so their side effects and any
//!   resource TypeError stay observable; the returned values are fixed.

use crate::codegen_support::builtins::io::stream_arg::emit_stream_fd_arg;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_introspection()` stream and I/O builtin calls.
pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    match name {
        "stream_supports_lock" => emit_true(name, &args[0], true, emitter, ctx, data),
        "stream_is_local" => emit_true(name, &args[0], false, emitter, ctx, data),
        "stream_get_wrappers" => emit_string_array(
            // Static list of built-in wrappers compiled into the runtime.
            // User wrappers registered through stream_wrapper_register are
            // not surfaced here in v1 — PHP code that needs to enumerate
            // them would have to track them application-side.
            &[
                "file", "php", "data", "ftp", "http", "https", "ftps",
                "compress.zlib", "compress.bzip2", "phar", "glob",
            ],
            emitter,
            ctx,
            data,
        ),
        "stream_get_filters" => emit_string_array(
            // Built-in filters. User filters registered via
            // stream_filter_register are not yet enumerated here.
            &[
                "string.toupper",
                "string.tolower",
                "string.rot13",
                "string.strip_tags",
                "convert.base64-encode",
                "convert.base64-decode",
                "convert.quoted-printable-encode",
                "convert.quoted-printable-decode",
                "convert.iconv.*",
                "dechunk",
                "zlib.deflate",
                "zlib.inflate",
                "bzip2.compress",
                "bzip2.decompress",
            ],
            emitter,
            ctx,
            data,
        ),
        "stream_get_transports" => emit_string_array(
            // Transports recognised by stream_socket_client / server. `tls`
            // and `ssl` are available through stream_socket_enable_crypto
            // promoting a connected tcp:// socket, so they belong in this
            // list per PHP's conventions. tlsv1.x / sslv2 / sslv3 are
            // surfaced as aliases — they all route through the same
            // openssl-backed enable_crypto path with default version
            // negotiation.
            &[
                "tcp", "udp", "unix", "udg",
                "tls", "ssl", "sslv2", "sslv3",
                "tlsv1.0", "tlsv1.1", "tlsv1.2", "tlsv1.3",
            ],
            emitter,
            ctx,
            data,
        ),
        _ => None,
    }
}

/// Evaluates the argument (preserving side effects) and yields a fixed `true`.
fn emit_true(
    name: &str,
    arg: &Expr,
    validate_resource: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment(&format!("{}()", name));
    if validate_resource {
        emit_stream_fd_arg(name, arg, emitter, ctx, data);
    } else {
        emit_expr(arg, emitter, ctx, data);
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #1");                                  // every elephc stream satisfies this predicate
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, 1");                                  // every elephc stream satisfies this predicate
        }
    }
    Some(PhpType::Bool)
}

/// Builds an indexed PHP array of string literals by lowering a synthesized
/// `ArrayLiteral`, reusing the regular array-literal codegen path.
fn emit_string_array(
    items: &[&str],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let elements: Vec<Expr> = items.iter().map(|item| Expr::string_lit(*item)).collect();
    let array = Expr::new(ExprKind::ArrayLiteral(elements), Span::dummy());
    emit_expr(&array, emitter, ctx, data);
    Some(PhpType::Array(Box::new(PhpType::Str)))
}
