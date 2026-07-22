//! Purpose:
//! Home of the internal `__elephc_ptr_is_null` builtin: the compiler-prelude
//! alias of `ptr_is_null`, sharing its checker contract and semantic target.
//!
//! Called from:
//! - Injected prelude PHP sources (`src/image_prelude.rs`) through the builtin
//!   registry.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `--strict-php` (which hides
//!   PHP-visible extension builtins from user programs) does not affect it and
//!   prelude-injected code keeps compiling in strict mode.
//! - Delegates `check` and the lowering emitter to the `ptr_is_null` home so the
//!   two names cannot drift.


builtin! {
    name: "__elephc_ptr_is_null",
    area: Pointers,
    params: [pointer: Mixed],
    returns: Bool,
    check: crate::builtins::pointers::ptr_is_null::check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcPtrIsNull,
    ),
    summary: "Internal prelude alias of ptr_is_null.",
    internal: true,
}
