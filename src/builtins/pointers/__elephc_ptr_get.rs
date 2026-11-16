//! Purpose:
//! Home of the internal `__elephc_ptr_get` builtin: the compiler-prelude
//! alias of `ptr_get`, sharing its checker contract and semantic target.
//!
//! Called from:
//! - Injected prelude PHP sources (`src/php_ext/prelude.rs`) through the builtin
//!   registry.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `--strict-php` (which hides
//!   PHP-visible extension builtins from user programs) does not affect it and
//!   prelude-injected code keeps compiling in strict mode.
//! - Delegates `check` and the lowering emitter to the `ptr_get` home so the
//!   two names cannot drift.


builtin! {
    contract: "__elephc_ptr_get",
    check: crate::builtins::pointers::ptr_get::check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcPtrGet,
    ),
}
