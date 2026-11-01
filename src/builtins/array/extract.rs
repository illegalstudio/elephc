//! Purpose:
//! Home of PHP's `extract` builtin and its dynamic activation-scope semantics.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The source must be array-compatible; supported `EXTR_*` flags are enforced by the eval bridge.
//! - Lowering installs an eval barrier because extracted variable names are only known at runtime.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "extract",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Extract,
    ),
}

/// Validates the array boundary and returns the number of imported variables.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !crate::types::checker::builtins::array_arg_is_gradually_acceptable(&ty) {
        return Err(CompileError::new(
            cx.span,
            "extract() first argument must be array",
        ));
    }
    Ok(PhpType::Int)
}
