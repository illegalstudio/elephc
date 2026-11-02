//! Purpose:
//! Home of the internal `__elephc_object_is_enum` builtin: reports whether a
//! runtime value is a PHP enum case rather than an ordinary object.
//!
//! Called from:
//! - The injected `var_export` prelude (`src/var_export_prelude.rs`), which
//!   renders an enum case as `\Enum::Case` instead of `__set_state(...)`.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `--strict-php` cannot hide it from
//!   the prelude and no user program can call it.
//! - This internal alias lets injected support code query enum object metadata
//!   directly without exposing an additional PHP-visible function.
//! - Answers from the class id in the object header via `_class_enum_kinds`, so it
//!   is a bounds-checked table load with no allocation and no class-name compare.

builtin! {
    contract: "__elephc_object_is_enum",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcObjectIsEnum,
    ),
}
