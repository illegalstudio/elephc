//! Purpose:
//! Home of the PHP `grapheme_strrev` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `PhpType::Union([Str, Bool])` (the reversed string, or `false`).
//!   A check hook is required because the `builtin!` macro `returns:` field cannot
//!   express a union inline; `returns: Mixed` is a placeholder overridden by the hook.
//! - The check hook also reproduces the legacy argument-type guard: a statically
//!   non-string argument is rejected with `"grapheme_strrev() argument must be string"`.
//!   The common registry dispatch path does not enforce parameter types, so the guard
//!   must re-infer the argument type here. Arity (exactly 1, from the param list) is
//!   pre-validated by the registry's `check_arity` before the hook fires, so the single
//!   operand index is always present.
//! - `lower` is a thin wrapper over the dedicated `lower_grapheme_strrev` emitter, which
//!   boxes the `string|false` runtime result as `Mixed`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "grapheme_strrev",
    area: String,
    params: [string: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Reverses a string by grapheme cluster, returning false on failure.",
    php_manual: "https://www.php.net/manual/en/function.grapheme-strrev.php",
}

/// Validates a `grapheme_strrev` call and returns `PhpType::Union([Str, Bool])`.
///
/// Reproduces the legacy argument-type guard: a statically non-string argument
/// (anything other than `Str`, `Mixed`, or a `Union`) is rejected. Arity is
/// pre-validated by the registry, so `cx.args[0]` is always present. The argument
/// type is re-inferred here because the common registry dispatch path discards the
/// inferred types and does not enforce declared parameter types.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Str | PhpType::Mixed | PhpType::Union(_)) {
        return Err(CompileError::new(
            cx.span,
            "grapheme_strrev() argument must be string",
        ));
    }
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::False]))
}

/// Lowers a `grapheme_strrev` call by dispatching to the dedicated emitter.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_grapheme_strrev(ctx, inst)
}
