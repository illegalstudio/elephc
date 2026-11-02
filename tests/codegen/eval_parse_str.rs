//! Purpose:
//! End-to-end PHP compatibility coverage for `parse_str()` in eval-owned code.
//!
//! Called from:
//! - `tests/codegen/mod.rs` through the codegen integration suite.

use crate::support::*;

/// `parse_str()` replaces its by-reference output array and preserves PHP query
/// decoding, repeated `[]` keys, and nested key paths.
#[test]
fn test_eval_parse_str_populates_by_reference_array_output() {
    let out = compile_and_run(
        r#"<?php
echo eval('$result = ["stale" => "discarded"];
parse_str("name=Jane+Doe&items[]=a&items[]=b&nested[x.y]=z", $result);
echo ($result["stale"] ?? "gone"), ":", $result["name"], ":", $result["items"][0], ":", $result["items"][1], ":", $result["nested"]["x.y"];');
"#,
    );

    assert_eq!(out, "gone:Jane Doe:a:b:z");
}
