//! Purpose:
//! Home of the internal `__elephc_ptr_read_string` builtin: the compiler-prelude
//! alias of `ptr_read_string`, sharing its checker contract and semantic target.
//!
//! Called from:
//! - Injected prelude PHP sources (`src/image_prelude.rs`, `src/web_prelude.rs`)
//!   through the builtin registry.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `--strict-php` (which hides
//!   PHP-visible extension builtins from user programs) does not affect it and
//!   prelude-injected code keeps compiling in strict mode.
//! - Delegates `check` and the lowering emitter to the `ptr_read_string` home so
//!   the two names cannot drift.


builtin! {
    name: "__elephc_ptr_read_string",
    area: Pointers,
    params: [pointer: Mixed, length: Mixed],
    returns: Str,
    check: crate::builtins::pointers::ptr_read_string::check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcPtrReadString,
    ),
    summary: "Internal prelude alias of ptr_read_string.",
    internal: true,
}
