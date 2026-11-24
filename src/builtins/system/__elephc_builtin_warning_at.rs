//! Purpose:
//! Raises an engine warning at an explicit source line from a compiler-provided prelude.
//!
//! Called from:
//! - The tokenizer prelude, whose diagnostics report the line inside the tokenized source rather
//!   than the line of the `token_get_all()` call.
//!
//! Key details:
//! - The severity reaches the active PHP handler and reporting mask through typed Core EIR.
//! - This internal alias never appears in the PHP-visible function catalog.

use super::core_support::core_builtin_home;
core_builtin_home!("__elephc_builtin_warning_at", BuiltinWarningAt);
