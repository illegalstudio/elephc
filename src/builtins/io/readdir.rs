//! Purpose:
//! Home of the PHP `readdir` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the `dir_handle` argument is a stream resource and returns
//!   `Union(Str, False)` — php documents `string|false`, not `string|bool`, and the wider union
//!   made `function f(): string|false { return readdir($h); }` a compile error on a signature php
//!   accepts. `readlink()` and `ob_get_clean()` already use the narrow form.
//! - `returns: Mixed` is used because the union cannot be expressed through the scalar
//!   `returns:` field. Arguments are pre-inferred by the registry before the hook runs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "readdir",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Readdir,
    ),
}

/// Validates the directory handle is a stream resource and returns `Union(Str, False)`.
///
/// `$dir_handle` is OPTIONAL — omitted, it means php's last opened directory stream — so the
/// argument is only type-checked when one is actually written.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(arg) = cx.args.first() {
        crate::types::checker::builtins::io::common::ensure_optional_stream_resource(
            cx.checker,
            cx.name,
            arg,
            cx.env,
        )?;
    }
    Ok(cx.checker.normalize_union_type(vec![
        PhpType::Str,
        PhpType::False,
    ]))
}
