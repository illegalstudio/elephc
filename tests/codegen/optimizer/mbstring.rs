//! Purpose:
//! Checks that optimizer passes preserve mbstring request-state reads and writes.
//!
//! Called from:
//! - The codegen test binary's optimizer module.
//!
//! Key details:
//! - Runtime-selected encodings prevent the inputs from being folded into constants.
//! - Setter return values are intentionally unused to expose incorrect dead-code removal.

use super::*;

/// Verifies discarded setters remain observable and successive default-encoding reads stay distinct.
#[test]
fn test_mbstring_optimizer_preserves_request_settings() {
    let out = compile_and_run(r#"<?php
$encoding = $argc > 0 ? "ISO-8859-1" : "UTF-8";
mb_internal_encoding($encoding);
echo mb_strlen("é"), ":", bin2hex(mb_substr("é", 0, 1)), ":";
mb_internal_encoding("UTF-8");
echo mb_strlen("é"), ":", bin2hex(mb_substr("é", 0, 1)), ":";
mb_language("Japanese");
echo mb_language(), ":";
mb_language("neutral");
echo mb_language(), ":";
mb_http_output("pass");
echo mb_http_output(), ":";
mb_http_output("UTF-16LE");
echo mb_http_output();
"#);
    assert_eq!(out, "2:c3:1:c3a9:Japanese:neutral:pass:UTF-16LE");
}

/// Keeps discarded recursive/null checks observable and preserves the request conversion counter.
#[test]
fn test_mbstring_check_optimizer_preserves_diagnostics_and_state() {
    let source = r#"<?php
mb_check_encoding();
mb_check_encoding([chr(255)]);
var_dump(mb_check_encoding(null));
mb_scrub(chr(255));
var_dump(mb_check_encoding(null));
"#;
    let out = compile_and_run_capture(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nbool(false)\n");
    assert_eq!(out.stderr.matches("Calling mb_check_encoding() without argument is deprecated").count(), 3);
}

/// Retains discarded substitution setters and reads their effects at each subsequent text call.
#[test]
fn test_mbstring_substitution_optimizer_preserves_setters() {
    let source = r#"<?php
$code = $argc > 0 ? 33 : 63;
mb_substitute_character($code);
echo bin2hex(mb_scrub(chr(255))), ":";
mb_substitute_character("none");
echo bin2hex(mb_scrub(chr(255))), ":";
mb_substitute_character("entity");
echo bin2hex(mb_scrub(chr(255)));
"#;
    assert_eq!(compile_and_run(source), "21::21");
}

/// Keeps discarded Stringable calls and observes global/request mutations made during parameter conversion.
#[test]
fn test_mbstring_optimizer_preserves_stringable_effects() {
    let source = r#"<?php
class MbStateChange {
    public function __toString(): string {
        global $mb_seen;
        $mb_seen = 42;
        mb_internal_encoding("8bit");
        echo "converted:";
        return "unused";
    }
}
$mb_seen = $argc > 0 ? 1 : 2;
mb_strlen(new MbStateChange());
echo $mb_seen, ":", mb_strlen("é");
"#;
    assert_eq!(compile_and_run(source), "converted:42:2");
}
