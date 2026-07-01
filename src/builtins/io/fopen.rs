//! Purpose:
//! Home of the PHP `fopen` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` detects the URL scheme from a string-literal first argument and links
//!   the appropriate runtime libraries (`elephc_tls`, `z`, `bz2`, `elephc_phar`,
//!   `elephc_crypto`) at compile time. Non-literal paths conservatively link all
//!   PHAR and decompression libraries.
//! - Returns `Union(stream_resource, Bool)` via `returns: Mixed` because the union
//!   involves a resource type that the scalar `returns:` field cannot express.
//! - Arguments are pre-inferred by the registry before the hook runs; the hook does
//!   NOT re-infer them.
//! - `lower` is a thin wrapper over `io::lower_fopen` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "fopen",
    area: Io,
    params: [
        filename: Str,
        mode: Str,
        use_include_path: Bool = DefaultSpec::Bool(false),
        context: Mixed = DefaultSpec::Null
    ],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Opens file or URL.",
    php_manual: "function.fopen",
}

/// Detects URL scheme from the filename literal and links the required runtime libraries.
///
/// A literal `https://` or `ftps://` URL links `elephc_tls`. A `compress.zlib://` scheme
/// links `z`. A `compress.bzip2://` scheme links `bz2`. A `phar://` URL in write mode
/// links `elephc_phar` and `elephc_crypto`. A non-literal path conservatively links
/// `elephc_phar`, `z`, and `bz2` because the scheme is unknown until run time.
/// Returns `Union(stream_resource, Bool)` for the success/false-on-failure PHP pattern.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(ExprKind::StringLiteral(s)) = cx.args.first().map(|a| &a.kind) {
        if s.starts_with("https://") || s.starts_with("ftps://") {
            cx.checker.require_builtin_library("elephc_tls");
        }
        if s.starts_with("compress.zlib://") {
            // compress.zlib:// attaches a zlib.inflate filter, which pulls in libz.
            cx.checker.require_builtin_library("z");
        }
        if s.starts_with("compress.bzip2://") {
            // compress.bzip2:// calls libbz2's BZ2_bzBuffToBuffDecompress at fopen time.
            cx.checker.require_builtin_library("bz2");
        }
        // phar:// write mode uses the elephc-phar read-modify-write bridge when available
        // and keeps the elephc-crypto SHA1 path as the assembly fallback. Reads need
        // neither write bridge nor crypto here.
        if s.starts_with("phar://") {
            let write_mode = matches!(
                cx.args.get(1).map(|a| &a.kind),
                Some(ExprKind::StringLiteral(m))
                    if matches!(m.as_bytes().first(), Some(b'w') | Some(b'a') | Some(b'c') | Some(b'x'))
            );
            if write_mode {
                cx.checker.require_builtin_library("elephc_phar");
                cx.checker.require_builtin_library("elephc_crypto");
            }
        }
    } else {
        // Non-literal paths can route to a phar:// entry at run time for reads or
        // write-mode opens. Reads may use tar/zip and compressed entries through the
        // elephc-phar/zlib/bz2 bridge.
        cx.checker.require_builtin_library("elephc_phar");
        cx.checker.require_builtin_library("z");
        cx.checker.require_builtin_library("bz2");
    }
    Ok(cx.checker.normalize_union_type(vec![
        PhpType::stream_resource(),
        PhpType::Bool,
    ]))
}

/// Lowers an `fopen` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_fopen(ctx, inst)
}
