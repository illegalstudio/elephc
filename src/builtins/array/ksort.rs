//! Purpose:
//! Home of the PHP `ksort` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `array` by reference plus an optional `int $flags`. The `ref`
//!   marker is mandatory — it is what makes by-reference mutation lower correctly (ir_lower
//!   reads `ref_params` from the registry sig).
//! - `$flags` reaches the runtime as a plain word; `__rt_hash_key_sort_enter` resolves it into
//!   a comparator once per call. Omitting it is `SORT_REGULAR`, which keeps the original
//!   comparator untouched.
//! - The shared key-sort checker also accepts integer-indexed cells of `array<mixed>` locals and
//!   supported property places; nested lowering validates the runtime payload before sorting or
//!   returning the packed-array no-op.

builtin! {
    contract: "ksort",
    check: super::key_sort::check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Ksort,
    ),
}
