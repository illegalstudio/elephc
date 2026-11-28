//! Purpose:
//! Home of the internal `__elephc_ptr_read8` builtin: the compiler-prelude alias of `ptr_read8`.
//!
//! Called from:
//! - Injected prelude code (notably `pack_prelude`) through the builtin registry.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `--strict-php` keeps prelude code compiling.
//! - Delegates the checker and lowering target to `ptr_read8` so the names cannot drift.

builtin! {
    contract: "__elephc_ptr_read8",
    check: crate::builtins::pointers::ptr_read8::check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PtrRead8,
    ),
}
