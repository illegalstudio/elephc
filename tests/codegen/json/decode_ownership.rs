//! Purpose:
//! Regression tests for `json_decode()` ownership: decoded containers, their elements and the
//! object keys are released with the result.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The decoder boxed every array, hash and stdClass with a RETAINING helper, pushed every
//!   element with a retaining push, and kept each object key's box after `__rt_hash_set` had
//!   persisted its own copy: `json_decode('[1,2]', true)` leaked three blocks per call, a nested
//!   document dozens.

use crate::support::*;

/// Assoc, object and list documents, nested and discarded, leave a clean heap.
#[test]
fn test_json_decode_releases_every_decoded_container() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function run(): void {
    $total = 0;
    for ($i = 0; $i < 5; $i++) {
        $a = json_decode('{"a":{"b":[1,2,{"c":"x"}]},"d":"e","7":[]}', true);
        $o = json_decode('{"a":{"b":[1,2]},"d":"e","e":{}}');
        $l = json_decode('[[1],[],{"k":null}]', true);
        json_decode('{"discarded":[1,2,3]}');
        $total += count($a) + count($l) + count($o->a->b);
    }
    echo $total, "|", $a["a"]["b"][2]["c"], "|", $o->d, "|", json_encode($l), "\n";
}
run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "40|x|e|[[1],[],{\"k\":null}]\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}
