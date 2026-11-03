//! Purpose:
//! Home of the PHP `locale_get_display_script` builtin declaration and its registry binding.
//!
//! Called from:
//! - The builtin registry, checker, optimizer, and AST-to-EIR builtin lowering path.
//!
//! Key details:
//! - Reuses the shared Intl typed EIR lowering declared in `super::intl_semantics`.
//! - The PHP-visible signature, defaults, and docs come from the neutral contract catalog.

builtin! {
    contract: "locale_get_display_script",
    check: super::intl_check,
    semantics: super::intl_semantics(),
}
