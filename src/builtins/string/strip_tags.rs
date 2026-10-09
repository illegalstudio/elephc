//! Purpose:
//! Home of the PHP `strip_tags` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - `allowed_tags` is optional (`null` by default) and accepts a string or an
//!   array of tag names, matching PHP 8.5. The declared return type is `Str`.
//! - HTML comments and PHP tags are always stripped and cannot be allow-listed.

builtin! {
    contract: "strip_tags",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StripTags,
    ),
}
