//! Purpose:
//! Home of the PHP `tmpfile` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `tmpfile` takes no PHP-visible arguments but the legacy allows `tmpfile(...[])`,
//!   i.e. spreading an empty array literal, as a valid zero-argument call. `max_args: 1`
//!   prevents the registry's `check_arity` from rejecting that single-spread form;
//!   `arity_error` overrides the error message for 2+-arg calls to match the legacy text.
//!   The check hook rejects any non-empty spread or any real argument explicitly.
//! - `max_args` affects only `check_arity`; `function_sig`/`arity_bounds` still derive
//!   `(0, Some(0))` from the zero-param list, keeping parity green.
//! - `is_empty_static_array_spread` is relocated here from `streams.rs` (its only caller).
//! - `returns: Mixed` is used because the union involves a resource type that the
//!   scalar `returns:` field cannot express.
//! - `lower` is a thin wrapper over `io::lower_tmpfile` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "tmpfile",
    area: Io,
    params: [],
    max_args: 1,
    arity_error: "tmpfile() takes no arguments",
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Creates a temporary file.",
    php_manual: "function.tmpfile",
}

/// Accepts `tmpfile()` and `tmpfile(...[])` (empty static-array spread) but rejects
/// any real argument. Returns `Union(stream_resource, Bool)` on success.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if !cx.args.is_empty() && !is_empty_static_array_spread(cx.args) {
        return Err(CompileError::new(cx.span, "tmpfile() takes no arguments"));
    }
    Ok(cx.checker.normalize_union_type(vec![
        PhpType::stream_resource(),
        PhpType::Bool,
    ]))
}

/// Returns `true` if `args` contains exactly one element that is a `...[...]` spread
/// of an empty array literal.
///
/// PHP allows `tmpfile(...[])` as a no-argument call. This helper distinguishes that
/// valid form from a real argument by checking for a single `Spread` node wrapping an
/// `ArrayLiteral([])`. Returns `false` for all other argument shapes.
fn is_empty_static_array_spread(args: &[crate::parser::ast::Expr]) -> bool {
    let [arg] = args else {
        return false;
    };
    let ExprKind::Spread(inner) = &arg.kind else {
        return false;
    };
    matches!(&inner.kind, ExprKind::ArrayLiteral(items) if items.is_empty())
}

/// Lowers a `tmpfile` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_tmpfile(ctx, inst)
}
