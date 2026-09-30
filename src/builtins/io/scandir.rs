//! Purpose:
//! Home of the PHP `scandir` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Array<Str>` (the directory entries). A check hook is required
//!   because the array return type cannot be expressed through the scalar `returns:`
//!   field.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "scandir",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Scandir,
    ),
}

/// Returns `array|false`, the signature php documents.
///
/// `False`, not `Bool`: the member a `!== false` narrowing removes, following `fgetcsv`.
/// The union is what lets the failure case exist at all — with a bare `Array<Str>` the
/// runtime's `false` had no representation and a failed listing was indistinguishable from
/// an empty directory. The array-taking consumers accept the union by unboxing at their own
/// call sites.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Union(vec![
        PhpType::Array(Box::new(PhpType::Str)),
        PhpType::False,
    ]))
}
