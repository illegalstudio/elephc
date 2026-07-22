//! Purpose:
//! Home of the PHP `stream_filter_append` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates stream resource arg[0], then matches on a literal filter name
//!   to link the appropriate runtime libraries (zlib, iconv, bz2). Returns `Mixed`.
//! - Arguments are pre-inferred by the registry before the hook runs; the hook does NOT
//!   re-infer args[2..]. The source deliberately does not infer arg[1] in the
//!   StringLiteral branch (harmless since the common path infers it side-effect-free).

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
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
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamFilterAppend,
    ),
    requirements: crate::builtins::semantics::stream_filter_requirements,
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
        ExprKind::StringLiteral(_) => {
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
