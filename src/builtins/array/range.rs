//! Purpose:
//! Home of the PHP `range` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` infers both arguments and always returns `Array(Int)`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "range",
    area: Array,
    params: [start: Mixed, end: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Range,
    ),
    summary: "Create an array containing a range of elements.",
    php_manual: "https://www.php.net/manual/en/function.range.php",
}

/// Infers both arguments and returns `Array(Int)`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 2 arguments).
/// Both arguments are inferred for side-effect tracking; the return type is always
/// an indexed integer array matching the runtime emitter's output shape.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.infer_type(&cx.args[1], cx.env)?;
    Ok(PhpType::Array(Box::new(PhpType::Int)))
}
