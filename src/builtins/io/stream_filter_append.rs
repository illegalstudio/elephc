//! Purpose:
//! Home of the PHP `stream_filter_append` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates stream resource arg[0], then matches on a literal filter name
//!   to link the appropriate runtime libraries (zlib, iconv, bz2). Returns `Mixed`.
//! - Arguments are pre-inferred by the registry before the hook runs; the hook does NOT
//!   re-infer args[2..]. The source deliberately does not infer arg[1] in the
//!   StringLiteral branch (harmless since the common path infers it side-effect-free).
//! - `lower` dispatches to `io::lower_stream_filter_attach` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "stream_filter_append",
    area: Io,
    params: [
        stream: Mixed,
        filtername: Str,
        read_write: Int = DefaultSpec::Int(3),
        params: Mixed = DefaultSpec::Null
    ],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Attaches a filter to a stream.",
    php_manual: "function.stream-filter-append",
}

/// Validates the stream resource and links required filter libraries for known literal filter names.
///
/// Checks that arg[0] is a stream resource. For a string-literal arg[1], links the appropriate
/// system library: `z` for zlib filters, `iconv` (macOS only) for iconv filters, `bz2` for
/// bzip2 filters. Dynamic filter names are routed through the runtime filter registry. Returns `Mixed`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    match &cx.args[1].kind {
        ExprKind::StringLiteral(filter) => {
            // The zlib.* filters call into the system zlib, so any
            // program that attaches one must link against libz.
            if filter.as_str() == "zlib.deflate" || filter.as_str() == "zlib.inflate" {
                cx.checker.require_builtin_library("z");
            }
            // convert.iconv.* uses libc iconv: in libc on Linux
            // (glibc/musl) but a separate library on macOS, so only
            // macOS needs explicit -liconv linkage.
            if filter.starts_with("convert.iconv.") {
                cx.checker.require_macos_builtin_library("iconv");
            }
            // The bzip2.* filters call into libbz2 (BZ2_bz*), so any
            // program that attaches one must link against -lbz2. The
            // existing compress.bzip2:// require fires only on the fopen
            // path, not here, so this is the filter path's own wiring.
            if filter.as_str() == "bzip2.compress" || filter.as_str() == "bzip2.decompress" {
                cx.checker.require_builtin_library("bz2");
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
            cx.checker.infer_type(&cx.args[1], cx.env)?;
        }
    }
    Ok(PhpType::Mixed)
}

/// Lowers a `stream_filter_append` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_stream_filter_attach(ctx, inst, "stream_filter_append")
}
