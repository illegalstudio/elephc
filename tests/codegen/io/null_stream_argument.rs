//! Purpose:
//! Integration tests for a NULL where a stream resource is required: php raises a catchable
//! `TypeError` when the call runs, and the rest of the program compiles and runs.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - elephc REFUSED the program — `error: fgetc() expects resource, got null` — so an undefined
//!   `$h` reaching `while (fgetc($h) !== false)` failed the whole file, and the `try`/`catch` php
//!   programmers write around such a call could not even be compiled.
//! - php numbers and NAMES the parameter, from the same shared contract the closed-stream
//!   TypeError beside this one already reads: `fgetc(): Argument #1 ($stream)`,
//!   `stream_copy_to_stream(): Argument #2 ($to)`. Hard-coding either half produced a message php
//!   never prints.
//! - `closedir(null)` is NOT this case: php DEPRECATES passing null there and uses the last
//!   opened directory stream, which is a different rule and stays on its own path.
//! - Every expectation was measured on `php -n` 8.5.6.

use crate::support::*;

/// Verifies the TypeError is catchable and the program continues.
#[test]
fn test_a_null_stream_argument_is_a_catchable_type_error() {
    let out = compile_and_run_capture(
        r#"<?php
try {
    fgetc(null);
} catch (TypeError $e) {
    echo "caught: ", $e->getMessage(), "\n";
}
try {
    $r = fread($undefinedHandle, 10);
    var_dump($r);
} catch (TypeError $e) {
    echo "caught2: ", $e->getMessage(), "\n";
}
echo "still running\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "caught: fgetc(): Argument #1 ($stream) must be of type resource, null given\n\
         caught2: fread(): Argument #1 ($stream) must be of type resource, null given\n\
         still running\n"
    );
    assert_eq!(
        out.diagnostics,
        "Warning: Undefined variable $undefinedHandle\n"
    );
}

/// Verifies the parameter is numbered and named from the contract, not assumed.
///
/// `stream_copy_to_stream()` is the one builtin in this family whose SECOND parameter is also a
/// stream, and php calls it `Argument #2 ($to)`.
#[test]
fn test_the_type_error_names_the_parameter_php_names() {
    let out = compile_and_run_capture(
        r#"<?php
$h = fopen("php://memory", "w+");
foreach (["copy", "fclose", "feof", "fwrite"] as $which) {
    try {
        match ($which) {
            "copy" => stream_copy_to_stream($h, null),
            "fclose" => fclose(null),
            "feof" => feof(null),
            "fwrite" => fwrite(null, "x"),
        };
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
fclose($h);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "stream_copy_to_stream(): Argument #2 ($to) must be of type resource, null given\n\
         fclose(): Argument #1 ($stream) must be of type resource, null given\n\
         feof(): Argument #1 ($stream) must be of type resource, null given\n\
         fwrite(): Argument #1 ($stream) must be of type resource, null given\n"
    );
}

/// Verifies the UNCAUGHT shape matches php's report, down to the exit status.
///
/// This is the auditor program that named the refusal: a loop over an undefined handle.
#[test]
fn test_an_uncaught_null_stream_argument_matches_phps_report() {
    let out = compile_and_run_capture(
        r#"<?php
$n = 0;
while (fgetc($h) !== false) {
    $n++;
}
echo $n, "\n";
"#,
    );
    assert!(!out.success, "the program should have exited non-zero");
    assert_eq!(out.exit_code, Some(255));
    assert_eq!(
        out.located_diagnostics,
        "Warning: Undefined variable $h in test.php on line 3\n\
         Fatal error: Uncaught TypeError: fgetc(): Argument #1 ($stream) must be of type \
         resource, null given in test.php:3\n"
    );
}

/// Verifies `closedir(null)` keeps php's OTHER rule for a null stream argument.
///
/// The directory family declares `?resource $dir_handle = null` and reads php's LAST opened
/// directory stream for it: passing null is DEPRECATED rather than refused, and the TypeError
/// that follows says `No resource supplied` — there was no directory open. Two different rules
/// for the same-looking argument, and this one must not be pulled onto the other path.
#[test]
fn test_closedir_keeps_its_deprecation_and_its_own_wording() {
    let out = compile_and_run_capture(
        r#"<?php
try {
    closedir(null);
} catch (TypeError $e) {
    echo "TypeError: ", $e->getMessage(), "\n";
}
echo "still running\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "TypeError: No resource supplied\nstill running\n");
    assert_eq!(
        out.diagnostics,
        "Deprecated: closedir(): Passing null is deprecated, instead the last opened directory \
         stream should be provided\n"
    );
}
