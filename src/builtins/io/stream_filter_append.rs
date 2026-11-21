//! Purpose:
//! Home of the PHP `stream_filter_append` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The checker validates the stream and returns the catalog's resource-or-false alternatives.
//! - The semantic requirements hook selects libraries for known filter names.
//! - Arguments are inferred by the registry before the check hook runs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "stream_filter_append",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamFilterAppend,
    ),
    requirements: crate::builtins::semantics::stream_filter_requirements,
}

/// Validates the stream and preserves false as the filter-creation failure alternative.
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
    Ok(PhpType::Union(vec![PhpType::Resource(None), PhpType::False]))
}
