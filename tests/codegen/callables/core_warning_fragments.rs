//! Purpose:
//! Covers explicit diagnostic boundaries for fragmented runtime warnings.
//!
//! Called from:
//! - The codegen integration harness through the callables module.
//!
//! Key details:
//! - Newlines inside dynamic keys must not complete a warning or invoke a second callback.

use crate::support::*;

/// Embedded and terminal newlines inside missing keys remain part of one callback message.
#[test]
fn test_core_warning_fragments_newline_keys() {
    let source = r#"<?php
function byteWarning(int $level, string $message): bool { echo $message, ':'; return true; }
set_error_handler('byteWarning');
$array = ['present' => 7];
$newline = 'missing' . $argc . "\n";
$middle = "mid\n" . $argc . "\n";
$first = $array[$newline];
$second = $array[$middle];
echo 'done';
"#;
    let expected = "Undefined array key \"missing1\n\":Undefined array key \"mid\n1\n\":done";
    assert_eq!(compile_and_run(source), expected);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Native fragmented warnings cross the eval callback bridge only when their suffix arrives.
#[test]
fn test_core_warning_fragments_newline_key_eval_handler() {
    let out = compile_and_run(r#"<?php
$source = 'function evalByteWarning($level, $message) { echo $message, ":"; return true; }
set_error_handler("evalByteWarning");' . ' // ' . $argc;
eval($source);
$array = ['present' => 7];
$key = 'missing' . $argc . "\n";
$value = $array[$key];
echo 'done';
"#);
    assert_eq!(out, "Undefined array key \"missing1\n\":done");
}

/// Without a handler, framing leaves the full default diagnostic byte sequence intact.
#[test]
fn test_core_warning_fragments_newline_key_default_output() {
    let output = compile_and_run_capture(r#"<?php
$array = ['present' => 7];
$key = 'missing' . $argc . "\n";
$value = $array[$key];
echo 'done';
"#);
    assert!(output.success);
    assert_eq!(output.stdout, "done");
    // php names the raise site after the quoted key: `... "missing1\n" in <file> on line 4`.
    assert!(
        output.stderr.starts_with("Warning: Undefined array key \"missing1\n\" in ")
            && output.stderr.ends_with(" on line 4\n"),
        "stderr: {:?}",
        output.stderr
    );
}
