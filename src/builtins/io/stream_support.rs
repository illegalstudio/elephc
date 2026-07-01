//! Purpose:
//! Shared helpers for stream wrapper/filter registration validation in the io builtin homes.
//! Provides class existence checks used by `stream_filter_register` and `stream_wrapper_register`.
//!
//! Called from:
//! - `crate::builtins::io::stream_filter_register` (check hook)
//! - `crate::builtins::io::stream_wrapper_register` (check hook)
//!
//! Key details:
//! - `validate_registered_stream_class` checks that a string-literal class argument refers to a
//!   declared class; non-literal arguments pass through unchecked (dynamic dispatch at runtime).
//! - `stream_registered_class_exists` uses PHP's case-insensitive class key for lookup.

use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind};
use crate::errors::CompileError;
use crate::types::checker::Checker;

/// Validates a literal stream wrapper/filter class name against declared classes.
///
/// If the class argument is a string literal and the named class is not declared,
/// returns a compile error at `span`. Non-literal arguments are accepted without
/// checking (the class name is resolved at runtime).
pub(crate) fn validate_registered_stream_class(
    checker: &Checker,
    builtin: &str,
    class_arg: &Expr,
    span: crate::span::Span,
) -> Result<(), CompileError> {
    let ExprKind::StringLiteral(class_name) = &class_arg.kind else {
        return Ok(());
    };
    if stream_registered_class_exists(checker, class_name) {
        return Ok(());
    }
    Err(CompileError::new(
        span,
        &format!("{}(): undefined class '{}'", builtin, class_name),
    ))
}

/// Returns true when `class_name` exists under PHP's case-insensitive class lookup.
///
/// Strips a leading backslash from `class_name` before the key comparison so that
/// both `\Foo` and `Foo` resolve to the same class.
pub(crate) fn stream_registered_class_exists(checker: &Checker, class_name: &str) -> bool {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    checker
        .classes
        .keys()
        .any(|existing| php_symbol_key(existing) == class_key)
}
