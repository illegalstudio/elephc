//! Purpose:
//! Home of the PHP `preg_split` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Return element type is `Mixed` when `flags` is supplied (4 args), `Str` otherwise.
//! - `arity_error` is overridden to preserve the legacy message "preg_split() takes between
//!   2 and 4 arguments" (the registry default for min=2/max=4 produces "2 to 4 arguments").
//! - The registry pre-infers arguments before calling the hook; the hook must not
//!   call `infer_type` again.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::builtins::semantics::{
    runtime_fn_semantics, with_argument_lowering, BuiltinArgumentLowering, BuiltinResultType,
    BuiltinSemanticInput, BuiltinSemantics,
};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "preg_split",
    check: check,
    semantics: preg_split_semantics(),
}

/// Builds regex-split semantics with the boxed-Mixed EIR array layout expected by the runtime.
const fn preg_split_semantics() -> BuiltinSemantics {
    let mut semantics = with_argument_lowering(
        runtime_fn_semantics(crate::ir::RuntimeFnId::PregSplit),
        BuiltinArgumentLowering::PositionalRegex,
    );
    semantics.result_type = BuiltinResultType::Shared(eir_result_type);
    semantics
}

/// Returns the conservative EIR container type used by the regex split runtime ABI.
fn eir_result_type(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Mixed
}

/// Returns the split result array type, refining the element type based on argument count.
///
/// Returns `Array(Mixed)` when all four arguments are present (the `flags` argument
/// can cause mixed-type entries via `PREG_OFFSET_CAPTURE`), or `Array(Str)` for 2 or
/// 3 arguments. The registry pre-infers arguments; the hook must not call `infer_type`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let elements = if cx.args.len() >= 4 {
        PhpType::Mixed
    } else {
        PhpType::Str
    };
    Ok(PhpType::Union(vec![
        PhpType::Array(Box::new(elements)),
        PhpType::False,
    ]))
}
