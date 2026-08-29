//! Purpose:
//! Home of the PHP `getenv` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Str, False)` to reflect PHP's behaviour where `getenv`
//!   returns the value string on success or `false` if the variable is unset.
//! - The EIR result carries that union too. It used to be overridden to plain
//!   `Str` "for present and missing variables alike", which is where the two
//!   answers were collapsed: an unset variable came back as `""`, so
//!   `getenv($x) !== false` — the idiom for "is this set" — was true for every
//!   name, silently.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "getenv",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Getenv,
    ),
}

/// Returns the type PHP's signature declares, which depends on the ARITY.
///
/// `getenv($name)` answers `string|false`. `getenv()` answers the whole
/// environment as a string-keyed array, and cannot fail — there is no name to
/// miss. Returning the union for both would make every caller of the no-argument
/// form handle a `false` that cannot occur, and `foreach (getenv() as ...)` would
/// not type-check.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for arg in cx.args.iter() {
        cx.checker.infer_type(arg, cx.env)?;
    }
    if cx.args.is_empty() {
        // `Mixed`, not `AssocArray`: the result is a hash pointer BOXED in a Mixed
        // cell, exactly as `getdate` and `stat` return theirs, and those declare
        // `Mixed` for that reason. Declaring the array type instead tells every
        // consumer the value IS the hash, so `count()` reads the cell's tag as
        // the entry count and answers 5 for a 65-entry environment.
        //
        // The type is the REPRESENTATION, and losing `array<string,string>` here
        // is the price of the box.
        return Ok(PhpType::Mixed);
    }
    Ok(cx.checker.normalize_union_type(vec![PhpType::Str, PhpType::False]))
}
