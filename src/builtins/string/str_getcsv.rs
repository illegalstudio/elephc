//! Purpose:
//! Home of the PHP `str_getcsv` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Parses ONE CSV record out of a string. It is not `fgetcsv()` over a line: a newline
//!   between enclosures, and a newline inside an unenclosed field, are ordinary data. Only a
//!   trailing newline is structural, and php-src strips one in two separate places — the
//!   runtime helper documents the exact rule.
//! - `escape` defaults to `"\\"` as in PHP 8.4/8.5; PHP 9.0 will change it to `""`.
//! - The element type is `Mixed`, not `Str`, because php's return is `?string[]`: a wholly
//!   empty subject yields `[null]`, not `[""]`. Only a boxed cell can hold that null.

use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinResultType, BuiltinSemanticInput, BuiltinSemantics,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "str_getcsv",
    check: check,
    semantics: str_getcsv_semantics(),
}

/// Builds CSV-record semantics pinned to the boxed-`Mixed` row layout the runtime builds.
///
/// The EIR result type is stated rather than inferred so that a synthesized or
/// callable-dispatched `str_getcsv()` — which has no checked call-site type — still describes
/// the boxed cells `__rt_csv_row_to_mixed` produces. Reading those cells as raw string
/// pointer/length pairs would hand back header words as field bytes.
const fn str_getcsv_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::StrGetcsv);
    semantics.result_type = BuiltinResultType::Shared(eir_result_type);
    semantics
}

/// Returns the boxed-`Mixed` row layout the CSV runtime ABI produces.
fn eir_result_type(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Array(Box::new(PhpType::Mixed))
}

/// Returns `Array<Mixed>`: one record's fields, always at least one element.
///
/// `Mixed` rather than `Str` because php's own answer for a wholly empty subject is `[null]`
/// (`php_bc_fgetcsv_empty_line()`), and `null` is not a `string`. Measured on `php -n` 8.5.6:
/// `str_getcsv("")` is `[NULL]` while `str_getcsv(" ")` is `[" "]`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Array(Box::new(PhpType::Mixed)))
}
