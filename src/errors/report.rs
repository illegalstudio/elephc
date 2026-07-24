//! Purpose:
//! Formats compiler errors and warnings for terminal output.
//! Converts optional file and span metadata into concise source-location diagnostics.
//!
//! Called from:
//! - `crate::errors::report()` and `crate::errors::report_warning()`.
//!
//! Key details:
//! - Formatting stays intentionally simple because callers may report diagnostics during early pipeline failure.
//! - Decoration (color, symbol prefix) is gated behind `crate::progress::is_decorated()`,
//!   the same switch the live spinner and `--timings` report use, so plain/piped/`--quiet`
//!   runs keep byte-identical text to before.

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
    let plain = format_diagnostic("error", &error.file, error.span, &error.message);
    if crate::progress::is_decorated() {
        eprintln!(
            "{} {}",
            console::style("\u{2717}").red().bold(),
            console::style(&plain).red().bold()
        );
    } else {
        eprintln!("{}", plain);
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
    let plain = format_diagnostic("warning", &None, warning.span, &warning.message);
    if crate::progress::is_decorated() {
        eprintln!(
            "{} {}",
            console::style("\u{26a0}").yellow().bold(),
            console::style(&plain).yellow().bold()
        );
    } else {
        eprintln!("{}", plain);
    }
}

/// Builds the plain `kind[location]: message` diagnostic text shared by both
/// `print_error` and `print_warning`, independent of decoration.
fn format_diagnostic(
    kind: &str,
    file: &Option<String>,
    span: crate::span::Span,
    message: &str,
) -> String {
    match (file, span.line > 0) {
        (Some(file), true) => format!("{}[{}:{}:{}]: {}", kind, file, span.line, span.col, message),
        (Some(file), false) => format!("{}[{}]: {}", kind, file, message),
        (None, true) => format!("{}[{}:{}]: {}", kind, span.line, span.col, message),
        (None, false) => format!("{}: {}", kind, message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    #[test]
    fn format_diagnostic_file_line_col() {
        let msg = format_diagnostic("error", &Some("a.php".to_string()), Span::new(3, 5), "bad");
        assert_eq!(msg, "error[a.php:3:5]: bad");
    }

    #[test]
    fn format_diagnostic_file_only() {
        let msg = format_diagnostic("error", &Some("a.php".to_string()), Span::new(0, 0), "bad");
        assert_eq!(msg, "error[a.php]: bad");
    }

    #[test]
    fn format_diagnostic_line_col_only() {
        let msg = format_diagnostic("warning", &None, Span::new(3, 5), "bad");
        assert_eq!(msg, "warning[3:5]: bad");
    }

    #[test]
    fn format_diagnostic_no_location() {
        let msg = format_diagnostic("warning", &None, Span::new(0, 0), "bad");
        assert_eq!(msg, "warning: bad");
    }
}
