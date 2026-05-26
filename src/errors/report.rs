//! Purpose:
//! Formats compiler errors and warnings for terminal output.
//! Converts optional file and span metadata into concise source-location diagnostics.
//!
//! Called from:
//! - `crate::errors::report()` and `crate::errors::report_warning()`.
//!
//! Key details:
//! - Formatting stays intentionally simple because callers may report diagnostics during early pipeline failure.

use super::{CompileError, CompileWarning};

/// Prints a compiler error to stderr with optional source location.
///
/// Format varies based on available metadata:
/// - With file, line, and column: `error[file:line:col]: message`
/// - With file only: `error[file]: message`
/// - With line and column only: `error[line:col]: message`
/// - With no location metadata: `error: message`
///
/// Recursively prints any related errors attached to the main error.
pub fn print_error(error: &CompileError) {
    match (&error.file, error.span.line > 0) {
        (Some(file), true) => {
            eprintln!(
                "error[{}:{}:{}]: {}",
                file, error.span.line, error.span.col, error.message
            );
        }
        (Some(file), false) => {
            eprintln!("error[{}]: {}", file, error.message);
        }
        (None, true) => {
            eprintln!(
                "error[{}:{}]: {}",
                error.span.line, error.span.col, error.message
            );
        }
        (None, false) => {
            eprintln!("error: {}", error.message);
        }
    }
    for related in &error.related {
        print_error(related);
    }
}

/// Prints a compiler warning to stderr with optional line and column.
///
/// Format varies based on available metadata:
/// - With line and column: `warning[line:col]: message`
/// - With no location metadata: `warning: message`
pub fn print_warning(warning: &CompileWarning) {
    if warning.span.line > 0 {
        eprintln!(
            "warning[{}:{}]: {}",
            warning.span.line, warning.span.col, warning.message
        );
    } else {
        eprintln!("warning: {}", warning.message);
    }
}
