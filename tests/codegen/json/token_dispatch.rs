//! Purpose:
//! Verifies structural JSON token dispatch without redundant legacy string decoding.
//!
//! Called from:
//! - The JSON codegen integration suite.
//!
//! Key details:
//! - Runtime whitespace and mixed token kinds cover borrowed source lifetimes.
//! - PHP output and clean heap ownership are checked with both optimizer modes.

use super::*;

/// Scalar, nested and escaped string values preserve PHP results after raw-token dispatch.
#[test]
fn test_json_trimmed_token_dispatch_php_and_ownership() {
    let source = r#"<?php
$pad = $argc > 0 ? " \t\r\n" : " ";
$tokens = ['42', '-42', '1.25e2', 'true', 'false', 'null',
    '9223372036854775807', '-9223372036854775808', '9223372036854775808',
    '"hi\\nthere"', '"a\\u0000b"', '"\\uD83D\\uDE00"',
    '[1, "two", null, [true, -3]]', '{"a":1,"b":"x\\ty"}'];
foreach ($tokens as $token) {
    $value = json_decode($pad . $token . $pad, true, 512, JSON_BIGINT_AS_STRING);
    echo gettype($value), '|', json_encode($value), '|', json_last_error(), "\n";
}
foreach (['01', '1e', 'trueX', '"\\uD800"', '[1,]'] as $token) {
    json_decode($pad . $token . $pad);
    echo json_last_error(), "\n";
}
"#;
    let expected = "integer|42|0\ninteger|-42|0\ndouble|125|0\nboolean|true|0\nboolean|false|0\nNULL|null|0\ninteger|9223372036854775807|0\ninteger|-9223372036854775808|0\nstring|\"9223372036854775808\"|0\nstring|\"hi\\nthere\"|0\nstring|\"a\\u0000b\"|0\nstring|\"\\ud83d\\ude00\"|0\narray|[1,\"two\",null,[true,-3]]|0\narray|{\"a\":1,\"b\":\"x\\ty\"}|0\n4\n4\n4\n10\n4\n";
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        assert_eq!(super::error_location::php_cli(source.strip_prefix("<?php").unwrap()).trim_end(),
            expected.trim_end(), "real PHP differs");
    }
    for off in [false, true] {
        let run = || {
            assert_eq!(compile_and_run(source), expected);
            let out = compile_and_run_with_heap_debug(source);
            assert!(out.success, "{}", out.stderr);
            assert_eq!(out.stdout, expected, "{}", out.stderr);
            assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        };
        if off { without_ir_opt(run); } else { run(); }
    }
}
