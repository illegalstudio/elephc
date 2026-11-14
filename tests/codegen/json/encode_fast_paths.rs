//! Purpose:
//! Regression coverage for the short plain-ASCII JSON string and object-key fast paths.
//!
//! Called from:
//! - The codegen JSON suite through `json`.
//!
//! Key details:
//! - Plain keys/values take the direct copy; slash, quote, backslash, UTF-8, long strings
//!   and every active JSON flag must fall back to the escaping encoder.

use crate::support::*;

/// Short plain keys and values stay PHP-exact across the fast and slow paths.
#[test]
fn json_short_plain_ascii_fast_paths_match_php() {
    let out = compile_and_run(
        r#"<?php
echo json_encode(["a" => 1, "b" => "text", "c" => [1, 2, 3], "d" => "/<>&'"]), "\n";
echo json_encode(["long-key-name" => 1]), "\n";
echo json_encode(["q\"z" => 1, "a\\b" => 2, "/" => 3, "é" => 4]), "\n";
echo json_encode(["a" => 1, "b" => [1, 2, 3]], JSON_PRETTY_PRINT), "\n";
echo json_encode(["a" => 1, "b" => 2], JSON_FORCE_OBJECT), "\n";
"#,
    );
    assert_eq!(
        out,
        "{\"a\":1,\"b\":\"text\",\"c\":[1,2,3],\"d\":\"\\/<>&'\"}\n\
         {\"long-key-name\":1}\n\
         {\"q\\\"z\":1,\"a\\\\b\":2,\"\\/\":3,\"\\u00e9\":4}\n\
         {\n    \"a\": 1,\n    \"b\": [\n        1,\n        2,\n        3\n    ]\n}\n\
         {\"a\":1,\"b\":2}\n"
    );
}

/// The direct key/value copy accepts at most sixteen bytes; exactly seventeen and an escaped
/// byte at the boundary must still agree with PHP's quoted representation.
#[test]
fn json_short_ascii_fast_path_length_boundaries_match_php() {
    let out = compile_and_run(
        r#"<?php
echo json_encode(["aaaaaaaaaaaaaaaa" => "bbbbbbbbbbbbbbbb"]), "\n";
echo json_encode(["aaaaaaaaaaaaaaaaa" => "bbbbbbbbbbbbbbbbb"]), "\n";
echo json_encode(["aaaaaaaaaaaaaaa\"" => 1]), "\n";
echo json_encode(["aaaaaaaaaaaaaaab" => "ccccccccccccccc/"]), "\n";
echo json_encode(["a" => "0123456789abcdef", "b" => "0123456789abcdefg"]), "\n";
"#,
    );
    assert_eq!(
        out,
        "{\"aaaaaaaaaaaaaaaa\":\"bbbbbbbbbbbbbbbb\"}\n\
         {\"aaaaaaaaaaaaaaaaa\":\"bbbbbbbbbbbbbbbbb\"}\n\
         {\"aaaaaaaaaaaaaaa\\\"\":1}\n\
         {\"aaaaaaaaaaaaaaab\":\"ccccccccccccccc\\/\"}\n\
         {\"a\":\"0123456789abcdef\",\"b\":\"0123456789abcdefg\"}\n"
    );
}

/// Once the JSON output grows onto the heap, a short key whose scan falls back must resume at
/// the saved cursor rather than the pre-growth buffer, and escaped key/value bytes must survive.
#[test]
fn json_ascii_fast_path_falls_back_after_buffer_growth() {
    let out = compile_and_run(
        r#"<?php
$payload = [];
for ($i = 0; $i < 2000; $i++) {
    $payload["k" . $i] = str_repeat("a", 40);
}
$payload["q\"z"] = "a/b";
$payload["tail"] = "end";
$json = json_encode($payload);
echo strlen($json) > 80000 ? 1 : 0, "|";
echo strpos($json, "\"q\\\"z\":\"a\\/b\"") !== false ? 1 : 0, "|";
echo str_ends_with($json, "\"tail\":\"end\"}") ? 1 : 0, "\n";
"#,
    );
    assert_eq!(out, "1|1|1\n");
}
