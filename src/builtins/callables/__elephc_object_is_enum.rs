//! Purpose:
//! Home of the internal `__elephc_object_is_enum` builtin: reports whether a
//! runtime value is a PHP enum case rather than an ordinary object.
//!
//! Called from:
//! - The injected `var_export` prelude (`src/var_export_prelude.rs`), which
//!   renders an enum case as `\Enum::Case` instead of `__set_state(...)`.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `--php-only` cannot hide it from
//!   the prelude and no user program can call it.
//! - There is no PHP-visible equivalent to alias. `$v instanceof UnitEnum` is the
//!   PHP spelling, but elephc does not yet report enum cases as implementing
//!   `UnitEnum`, and `enum_exists()` requires a string LITERAL in AOT mode — so a
//!   prelude that only ever sees a runtime `mixed` has no other way to ask.
//! - Answers from the class id in the object header via `_class_enum_kinds`, so it
//!   is a bounds-checked table load with no allocation and no class-name compare.

builtin! {
    contract: "__elephc_object_is_enum",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcObjectIsEnum,
    ),
}
