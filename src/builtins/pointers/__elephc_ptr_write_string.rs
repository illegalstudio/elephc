//! Purpose:
//! Home of the internal `__elephc_ptr_write_string` builtin: the compiler-prelude
//! alias of `ptr_write_string`, sharing its checker contract and semantic target.
//!
//! Called from:
//! - Injected prelude PHP sources (`src/image_prelude.rs`) through the builtin
//!   registry.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `--strict-php` (which hides
//!   PHP-visible extension builtins from user programs) does not affect it and
//!   prelude-injected code keeps compiling in strict mode.
//! - Delegates `check` and the lowering emitter to the `ptr_write_string` home so
//!   the two names cannot drift.


builtin! {
    name: "__elephc_ptr_write_string",
    area: Pointers,
    params: [pointer: Mixed, string: Mixed],
    returns: Int,
    check: crate::builtins::pointers::ptr_write_string::check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcPtrWriteString,
    ),
    summary: "Internal prelude alias of ptr_write_string.",
    internal: true,
}
