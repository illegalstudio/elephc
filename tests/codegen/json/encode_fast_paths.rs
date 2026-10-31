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
