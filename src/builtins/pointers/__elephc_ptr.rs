//! Purpose:
//! Home of the internal `__elephc_ptr` builtin: the compiler-prelude alias of `ptr`.
//!
//! Called from:
//! - Injected prelude code (notably `pack_prelude`) through the builtin registry.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `--strict-php` keeps prelude code compiling.
//! - Delegates the checker and lowering target to `ptr` so the names cannot drift.

builtin! {
    contract: "__elephc_ptr",
    check: crate::builtins::pointers::ptr::check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Ptr,
    ),
}
