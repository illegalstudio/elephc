//! Purpose:
//! Home of the PHP `stream_context_get_options` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `AssocArray{Str, Mixed}` which is not scalar-expressible, so
//!   `returns: Mixed` is used and the hook overrides the return type.
//! - Arguments are pre-inferred by the registry before the hook runs; the hook does NOT
//!   re-infer them.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "stream_context_get_options",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamContextGetOptions,
    ),
}

/// Returns `AssocArray{Str, Mixed}` reflecting the context options map structure.
///
/// MEASURED and left alone on purpose. php's map is TWO deep — an options array is
/// `["wrappername"]["optionname"] = $value`, the same rule the ValueError guard enforces on the
/// way IN — so `AssocArray{Str, AssocArray{Str, Mixed}}` is the honest type. Writing it changes
/// nothing a program can see: the foreach VALUE over a nested map is typed `Mixed` regardless, so
///
/// ```text
/// foreach (stream_context_get_options($c) as $wrapper => $pairs) { ksort($pairs); }
/// ```
///
/// still does not compile — the checker's `ksort() argument must be array` merely becomes the
/// backend's `ksort for PHP type Mixed`, which is a worse error for the same program. The gap is
/// the foreach element type and the sort family's missing Mixed receiver, not this declaration.
///
/// Arguments are pre-inferred by the registry; this hook only refines the return type
/// beyond what the scalar `returns: Mixed` field can express.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    })
}
