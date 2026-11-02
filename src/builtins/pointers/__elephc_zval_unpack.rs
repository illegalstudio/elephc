//! Purpose:
//! Home of the internal `__elephc_zval_unpack` builtin: the compiler-prelude
//! alias of `zval_unpack`, sharing its checker contract and semantic target.
//!
//! Called from:
//! - Injected prelude PHP sources (`src/php_ext/prelude.rs`) through the builtin
//!   registry.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `--strict-php` (which hides
//!   PHP-visible extension builtins from user programs) does not affect it and
//!   prelude-injected code keeps compiling in strict mode.
//! - Delegates `check` and the lowering emitter to the `zval_unpack` home so the
//!   two names cannot drift.


builtin! {
    contract: "__elephc_zval_unpack",
    check: crate::builtins::pointers::zval_unpack::check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcZvalUnpack,
    ),
}
