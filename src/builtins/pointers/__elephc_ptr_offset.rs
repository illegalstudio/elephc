//! Purpose:
//! Home of the internal `__elephc_ptr_offset` builtin: the compiler-prelude alias of `ptr_offset`.
//!
//! Called from:
//! - Injected prelude code (notably `pack_prelude`) through the builtin registry.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `--strict-php` keeps prelude code compiling.
//! - Delegates the checker and lowering target to `ptr_offset` so the names cannot drift.

builtin! {
    contract: "__elephc_ptr_offset",
    check: crate::builtins::pointers::ptr_offset::check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PtrOffset,
    ),
}
