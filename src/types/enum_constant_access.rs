//! Purpose:
//! Shares enum constant visibility rules between type inference and EIR lowering.
//!
//! Called from:
//! - Scoped constant inference and EIR scoped-value lowering.
//!
//! Key details:
//! - Enums are final, so protected and private constants are accessible only in the enum scope.
//! - Visibility failures remain catchable PHP Errors at the read, not declaration errors.

use crate::parser::ast::Visibility;

/// Returns the PHP access diagnostic for an enum constant read from a disallowed lexical scope.
pub(crate) fn visibility_error(
    enum_name: &str, constant_name: &str, visibility: Option<&Visibility>, scope: Option<&str>,
) -> Option<String> {
    if scope.is_some_and(|scope| scope.eq_ignore_ascii_case(enum_name)) { return None; }
    let visibility = match visibility? {
        Visibility::Public => return None,
        Visibility::Private => "private",
        Visibility::Protected => "protected",
    };
    Some(format!("Cannot access {visibility} constant {enum_name}::{constant_name}"))
}
