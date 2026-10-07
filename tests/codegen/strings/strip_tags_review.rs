//! Purpose:
//! Checks strip_tags allow-list normalization after integration with main.
//!
//! Called from:
//! - The string encoding codegen integration tests.
//!
//! Key details:
//! - Boxed indexed and associative arrays normalize their values before joining.
//! - Owned normalization copies are released after each allow-list rendering.

use crate::support::*;

/// A Mixed allow-list reaches the same checked dense-array renderer as a concrete one.
#[test]
fn strip_tags_review_boxed_array_allow_lists() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function stripWith(mixed $allow): string {
    return strip_tags('<p class="x">A</p><b>B</b>', $allow);
}
for ($i = 0; $i < 8; $i++) {
    echo stripWith(['p']), '|', stripWith(['key' => 'p']), '|';
}
echo stripWith(null), '|', stripWith('<b>');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, format!("{}AB|A<b>B</b>", "<p class=\"x\">A</p>B|<p class=\"x\">A</p>B|".repeat(8)));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Concrete associative allow-lists use values rather than reading their hash header as an array.
#[test]
fn strip_tags_review_concrete_assoc_allow_list() {
    let out = compile_and_run_with_heap_debug(r#"<?php
for ($i = 0; $i < 8; $i++) {
    echo strip_tags('<p>A</p><b>B</b>', ['key' => 'p']), '|';
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "<p>A</p>B|".repeat(8));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
