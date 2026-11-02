//! Purpose:
//! The interpreter's view of PHP's source highlighter.
//!
//! Called from:
//! - `crate::interpreter::builtins::strings::highlight_string` and its `highlight_file` companion.
//!
//! Key details:
//! - The renderer lives in the `elephc-highlight` crate, which compiled programs also link
//!   natively, so the interpreter and AOT code answer byte-identical output from one source.

pub(crate) use elephc_highlight::highlight_document;
