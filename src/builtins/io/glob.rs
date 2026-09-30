//! Purpose:
//! Home of the PHP `glob` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Array<Str>` (the matched pathnames). A check hook is required
//!   because the array return type cannot be expressed through the scalar `returns:`
//!   field.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "glob",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Glob,
    ),
}

/// Returns `Array<Str>` reflecting that `glob` yields the matched pathnames.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    // php's signature is array|false; False (not Bool) is the member a !== false narrowing
    // removes, following fgetcsv and scandir. The array-taking family accepts the union
    // through the argument lowering's unbox-or-throw.
    Ok(PhpType::Union(vec![
        PhpType::Array(Box::new(PhpType::Str)),
        PhpType::False,
    ]))
}
