//! Purpose:
//! Home of the PHP `implode` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `implode` is the one migrated builtin whose legacy CHECK arm (exactly 2 arguments)
//!   was STRICTER than its golden signature's minimum. The golden marks `array` optional
//!   (required count 1), which the parity gate compares against, so `array` must keep a
//!   default here. `max_args` caps only the maximum, so it cannot raise the minimum;
//!   the exact-2 requirement is therefore re-enforced inside the `check` hook to keep the
//!   legacy `"implode() takes exactly 2 arguments"` diagnostic for the tested 1-arg call.
//! - `check` returns `PhpType::Str`.
//! - `lower` is a thin wrapper over the shared `lower_implode` emitter, which itself
//!   requires exactly two operands.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "implode",
    area: String,
    params: [separator: Str, array: Mixed = DefaultSpec::Null],
    max_args: 2,
    returns: Str,
    returns_independent_storage: true,
    check: check,
    lower: lower,
    summary: "Joins array elements into a single string using a separator.",
    php_manual: "https://www.php.net/manual/en/function.implode.php",
}

/// Returns `PhpType::Str` for an `implode` call, enforcing the legacy exactly-2 arity.
///
/// The golden signature marks `array` optional (so the parity gate sees one required
/// param), but the legacy CHECK arm required exactly two arguments. `check_arity`'s
/// `max_args` override caps the maximum only and cannot raise the minimum, so the
/// exact-2 requirement is re-enforced here to preserve the legacy diagnostic. Argument
/// types are inferred by the common registry dispatch path before this hook fires.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if cx.args.len() != 2 {
        return Err(CompileError::new(
            cx.span,
            "implode() takes exactly 2 arguments",
        ));
    }
    Ok(PhpType::Str)
}

/// Lowers an `implode` call by dispatching to the shared `lower_implode` emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_implode(ctx, inst)
}
