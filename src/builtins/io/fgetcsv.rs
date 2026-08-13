//! Purpose:
//! Home of the PHP `fgetcsv` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the `stream` argument is a stream resource and returns `Array<Str>`.
//! - `returns: Mixed` is used because the array type cannot be expressed through the
//!   scalar `returns:` field. Arguments are pre-inferred by the registry before the hook runs.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "fgetcsv",
    area: Io,
    params: [stream: Mixed, length: Int = DefaultSpec::Null, separator: Str = DefaultSpec::Str(",")],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Fgetcsv,
    ),
    summary: "Gets line from file pointer and parse for CSV fields.",
    php_manual: "function.fgetcsv",
}

/// Validates the stream argument is a stream resource and returns `Array<Str>|bool`.
///
/// The union is what makes the manual's own read loop terminate: `fgetcsv()` answers
/// `false` at end of file, and while the declared type was `Array<Str>` the runtime
/// returned an empty array instead — never `!== false`, so
/// `while (($row = fgetcsv($h)) !== false)` looped forever.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    // `False`, not `Bool`: the guard narrowing strips an exact `False` member, so
    // `while (($row = fgetcsv($h)) !== false)` leaves `$row` an array in the body —
    // with `Bool` the union survives the guard and `count($row)` stops compiling.
    Ok(cx
        .checker
        .normalize_union_type(vec![PhpType::Array(Box::new(PhpType::Str)), PhpType::False]))
}
