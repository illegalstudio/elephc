//! Purpose:
//! End-to-end coverage of PHP's libxml error state: `libxml_use_internal_errors()`,
//! `libxml_get_errors()`, `libxml_get_last_error()`, `libxml_clear_errors()` and
//! `LibXMLError`, fed by SimpleXML parses.
//!
//! Called from:
//! - `cargo test` through Rust's test harness; needs the managed libxml2 artifact.
//!
//! Key details:
//! - Expected output was produced by `php -n` (PHP 8.5.8, libxml2 2.15.3) on the same source.
//! - Pins the php-src rules from `ext/libxml/libxml.c`: the last error survives without a
//!   list, `LIBXML_NOERROR` hides warnings but not list entries, and disabling internal
//!   errors destroys the list.

use crate::support::*;

/// The error list, the last-error fallback and the option filter follow php-src.
#[test]
fn test_libxml_error_list_and_last_error_follow_php() {
    if skip_without_xml_native("test_libxml_error_list_and_last_error_follow_php") {
        return;
    }
    let out = compile_and_run_with_heap_debug(
        r#"<?php
@simplexml_load_string('<a><b></a>');
var_dump(count(libxml_get_errors()));
$last = libxml_get_last_error();
var_dump($last instanceof LibXMLError, $last->code, $last->level === LIBXML_ERR_FATAL);
libxml_clear_errors();
var_dump(libxml_get_last_error());
var_dump(libxml_use_internal_errors(true));
simplexml_load_string('<a><b></a>');
foreach (libxml_get_errors() as $error) {
    echo $error->level, ' ', $error->code, ' ', $error->line, ':', $error->column, ' ', trim($error->message), "\n";
}
var_dump(libxml_get_last_error()->code);
libxml_clear_errors();
simplexml_load_string('<a><b></a>', 'SimpleXMLElement', LIBXML_NOERROR);
var_dump(count(libxml_get_errors()));
var_dump(libxml_use_internal_errors(false), libxml_get_errors());
"#,
    );
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "int(0)\nbool(true)\nint(76)\nbool(true)\nbool(false)\nbool(false)\n\
         3 76 1:11 Opening and ending tag mismatch: b line 1 and a\nint(76)\nint(1)\n\
         bool(true)\narray(0) {\n}\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `eval()` code and compiled code read and write one libxml error state.
#[test]
fn test_libxml_error_state_is_shared_with_eval() {
    if skip_without_xml_native("test_libxml_error_state_is_shared_with_eval") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
libxml_use_internal_errors(true);
eval('simplexml_load_string("<a><b></a>"); echo count(libxml_get_errors()), "|";');
echo libxml_get_last_error()->code, "|", eval('return libxml_use_internal_errors(false);') ? "on" : "off",
    "|", count(libxml_get_errors()), "\n";
"#,
    );
    assert_eq!(out, "1|76|on|0\n");
}

/// The libxml constants php-src defines for libxml2 2.15.3.
#[test]
fn test_libxml_constants_match_php() {
    let out = compile_and_run(
        r#"<?php
echo LIBXML_VERSION, ' ', LIBXML_DOTTED_VERSION, ' ', var_export(LIBXML_LOADED_VERSION, true), "\n";
echo LIBXML_ERR_NONE, LIBXML_ERR_WARNING, LIBXML_ERR_ERROR, LIBXML_ERR_FATAL, "\n";
echo LIBXML_XINCLUDE, ' ', LIBXML_NOEMPTYTAG, ' ', LIBXML_SCHEMA_CREATE, ' ', LIBXML_HTML_NOIMPLIED, "\n";
"#,
    );
    assert_eq!(out, "21503 2.15.3 '21503'\n0123\n1024 4 1 8192\n");
}
