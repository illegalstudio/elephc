//! Purpose:
//! Covers dead-code elimination of `ctype_*` calls whose result is discarded.
//!
//! Called from:
//! - The codegen integration test harness through the optimizer module.
//!
//! Key details:
//! - A string argument makes the call pure, but every non-string argument raises PHP 8.1+'s
//!   deprecation, so a discarded call must still reach the diagnostic and any error handler.
//! - Expectations are the PHP 8.5 output of the same fixtures.

use crate::support::*;

/// Discarded non-string `ctype_*` calls keep their deprecation; a discarded string call stays silent.
#[test]
fn test_discarded_ctype_calls_keep_non_string_deprecations() {
    let out = compile_and_run_capture(
        r#"<?php
ctype_upper(65);
ctype_alpha("abc");
$box = [true, "x"];
ctype_digit($box[0]);
echo "done";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert_eq!(
        normalize_diagnostic_text(&out.stderr),
        "Deprecated: ctype_upper(): Argument of type int will be interpreted as string in the future\n\
         Deprecated: ctype_digit(): Argument of type bool will be interpreted as string in the future\n"
    );
}

/// A discarded `ctype_*` call runs the error handler, which can change globals or throw.
///
/// Constant propagation must not fold the handler's counter across the call, and a throwing
/// handler must unwind past the statement that follows the discarded call.
#[test]
fn test_discarded_ctype_calls_run_error_handlers() {
    let out = compile_and_run_capture(
        r#"<?php
$deprecations = 0;
set_error_handler(function (int $level, string $message): bool {
    global $deprecations;
    $deprecations++;
    if ($deprecations === 2) {
        throw new RuntimeException("[$level] $message");
    }
    return true;
});
ctype_xdigit(10);
echo $deprecations, "\n";
try {
    ctype_upper(65);
    echo "unreachable\n";
} catch (RuntimeException $error) {
    echo $error->getMessage(), "\n";
}
echo $deprecations;
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "1\n[8192] ctype_upper(): Argument of type int will be interpreted as string in the future\n2"
    );
    assert_eq!(out.stderr, "");
}
