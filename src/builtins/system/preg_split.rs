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
    runtime_fn_semantics, with_argument_lowering, BuiltinArgumentLowering, BuiltinSemantics,
};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "preg_split",
    check: check,
    semantics: preg_split_semantics(),
}

/// Builds regex-split semantics.
///
/// The EIR result follows the checker's type for the call, vetted by the runtime id's
/// `checked_result_type_fits_operands`: the runtime always builds boxed-Mixed
/// element slots, so an `array<string>` checked type falls back to `array<mixed>`, while the
/// `array|false` of a pattern that can fail stays boxed and the lowering adds the false.
const fn preg_split_semantics() -> BuiltinSemantics {
    with_argument_lowering(
        runtime_fn_semantics(crate::ir::RuntimeFnId::PregSplit),
        BuiltinArgumentLowering::PositionalRegex,
    )
}

/// Returns the split result array type, refining the element type based on argument count.
///
/// Returns `Array(Mixed)` when all four arguments are present (the `flags` argument
/// can cause mixed-type entries via `PREG_OFFSET_CAPTURE`), or `Array(Str)` for 2 or
/// 3 arguments. The registry pre-infers arguments; the hook must not call `infer_type`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let elem = if cx.args.len() >= 4 { PhpType::Mixed } else { PhpType::Str };
    let array = PhpType::Array(Box::new(elem));
    if crate::builtins::system::preg_match::preg_pattern_can_fail(&cx.args[0]) {
        return Ok(PhpType::Union(vec![array, PhpType::False]));
    }
    Ok(array)
}
