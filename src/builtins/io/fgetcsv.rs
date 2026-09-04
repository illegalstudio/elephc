//! Purpose:
//! Home of the PHP `fgetcsv` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The element type is `Mixed`, not `Str`, because php's return is `?string[]`: a blank line
//!   yields `[null]`, not `[""]`. Only a boxed cell can hold that null.
//! - `check` validates the `stream` argument is a stream resource and returns `Mixed`, which is
//!   how the registry spells PHP's `array|false`. Declaring `Array<Str>` left the runtime's
//!   end-of-input answer — a null array pointer — reading as `null`, and `null !== false`, so
//!   the manual's own `while (($row = fgetcsv($h)) !== false)` loop never terminated.
//! - `returns: Mixed` is used because the array type cannot be expressed through the
//!   scalar `returns:` field. Arguments are pre-inferred by the registry before the hook runs.
//! - PHP 8.4: `escape` defaults to `"\\"` (the `""` RFC 4180 doubling mode is PHP 9.0).

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "fgetcsv",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Fgetcsv,
    ),
}

/// Validates the stream argument is a stream resource and returns `array<mixed>|false`.
///
/// The union is spelled out rather than collapsed to `Mixed` so that `!== false` narrows it
/// back to the array: `while (($row = fgetcsv($h)) !== false) { fputcsv($out, $row); }` has to
/// keep compiling, and an array-taking builtin cannot accept a bare `Mixed`. `False` is the
/// exact member the narrowing removes — `Bool` would not match. Storage is unchanged: a union
/// uses the same boxed payload as `Mixed`.
///
/// The element is `Mixed`, not `Str`, because php's row is `?string[]`: a BLANK LINE reads back
/// as `[null]`, not `[""]`. Only a boxed cell can hold that null, so this has to agree with the
/// `array<mixed>` layout `__rt_fgetcsv_row_to_mixed` builds — a `Str` element would read the box
/// pointer as a raw string pointer/length pair.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::Union(vec![
        PhpType::Array(Box::new(PhpType::Mixed)),
        PhpType::False,
    ]))
}
