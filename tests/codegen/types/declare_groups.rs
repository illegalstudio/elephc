//! Purpose:
//! Regressions for declaration-group exposure without erasing lowered array groups.
//! Verifies namespace discovery alongside array typing and ownership.
//!
//! Called from:
//! - The codegen types integration tests.
//!
//! Key details:
//! - Checks complete values, copy-on-write, namespace discovery and clean heaps.

use crate::support::{compile_and_run_with_heap_debug, ProgramOutput};

/// Checks execution, complete output and absence of undefined-key warnings or leaks.
fn assert_clean(output: ProgramOutput, expected: &str) {
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, expected, "{}", output.stderr);
    assert!(!output.stderr.contains("Undefined array key"), "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Indexed nested appends retain their specialization and copy a shared inner bucket.
#[test]
fn test_declare_groups_preserve_indexed_nested_append_cow() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
$groups = [];
$groups[0][] = 1;
$copy = $groups[0];
$groups[0][] = 2;
echo count($copy), ":", count($groups[0]), "\n";
"#), "1:2\n");
}

/// Associative autovivification neither reads a missing key nor loses its inserted value.
#[test]
fn test_declare_groups_preserve_associative_nested_append() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
$groups = [];
$groups["k"][] = 1;
var_dump($groups);
"#), "array(1) {\n  [\"k\"]=>\n  array(1) {\n    [0]=>\n    int(1)\n  }\n}\n");
}

/// Declarations in a namespace's declare body are exposed while its array groups stay intact.
#[test]
fn test_declare_groups_namespace_body_keeps_declarations_and_arrays() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
namespace Declared {
    declare(ticks=1) {
        function value(): int { return 7; }
        class Item { public function value(): int { return 42; } }
        $groups = [];
        $groups["k"][] = value();
    }
    echo (new Item())->value(), ":", count($groups["k"]), ":", $groups["k"][0], "\n";
}
"#), "42:1:7\n");
}

/// Nested transparent declare groups retain their named declaration discovery.
#[test]
fn test_declare_groups_discover_nested_declarations() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
namespace Declared;
declare(ticks=1) {
    declare(ticks=1) {
        function value(): int { return 7; }
    }
}
echo value(), "\n";
"#), "7\n");
}

/// A supported literal conditional is visible before the function collector runs.
#[test]
fn test_declare_groups_discover_statically_enabled_function() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
namespace Declared;
declare(ticks=1) {
    if (true) {
        function value(): int { return 7; }
    }
}
echo value(), "\n";
"#), "7\n");
}
