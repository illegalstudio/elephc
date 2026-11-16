//! Purpose:
//! Checks temporary segment owners returned by eval string splitting.
//!
//! Called from:
//! - The runtime GC integration suite.
//!
//! Key details:
//! - Direct, named and callable dispatch share the same retaining array builder.

use crate::support::*;

/// Split arrays preserve empty segments and limits while releasing all construction owners.
#[test]
fn test_eval_string_split_results_release_segment_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$source = $argc > 0 ? '
for ($index = 0; $index < 3; $index++) {
    $parts = explode("|", "|middle|");
    echo json_encode($parts), "|";
    unset($parts);
    echo json_encode(explode(separator: "|", string: "left|right", limit: 1)), "|";
    echo json_encode(call_user_func("explode", ":", "a:b:tail", -1)), "|";
}
' : '';
eval($source);
unset($source);
"#);
    assert!(out.success, "stdout={:?}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "[\"\",\"middle\",\"\"]|[\"left|right\"]|[\"a\",\"b\"]|".repeat(3), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
