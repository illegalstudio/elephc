//! Purpose:
//! Verifies ownership while nested foreach loops rebuild mixed array values.
//!
//! Called from:
//! - Focused codegen runtime GC integration tests.
//!
//! Key details:
//! - Both EIR optimizer modes must retire all loop and container temporaries.

use crate::support::{compile_and_run_with_heap_debug, without_ir_opt};

/// Rebuilding nested options releases every original, iterator value, and copied container.
#[test]
fn test_foreach_nested_rebuild_releases_request_temporaries() {
    let check = || {
        let output = compile_and_run_with_heap_debug(r#"<?php
function rebuild(): void {
    $cfg = ['http' => ['header' => 'one']];
    $next = [];
    foreach ($cfg as $n => $s) {
        $u = [];
        foreach ($s as $k => $v) { $u[$k] = $v; }
        $u['ua'] = 'two';
        $next[$n] = $u;
    }
    echo $next['http']['header'], ':', $next['http']['ua'], "\n";
}
rebuild();
rebuild();
function rebuildWithAlias(): void {
    $cfg = ['http' => ['header' => 'one']];
    $copy = [];
    foreach ($cfg as $name => $settings) {
        $updated = [];
        foreach ($settings as $key => $value) {
            $copy = $updated;
            $updated[$key] = $value;
        }
        $updated['ua'] = 'two';
        echo count($copy), ':', $updated['header'], ':', $updated['ua'], "\n";
    }
}
rebuildWithAlias();
"#);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "one:two\none:two\n0:one:two\n");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    };
    check();
    without_ir_opt(check);
}

/// A native isset probe releases the promoted hash receiver before the following write.
#[test]
fn test_header_normalization_isset_releases_promoted_receiver() {
    let check = || {
        let output = compile_and_run_with_heap_debug(r#"<?php
function normalizeHeaders(): void {
    $fields = ['Content-Type' => ['application/json']];
    $normalized = [];
    foreach ($fields as $name => $values) {
        $key = strtolower((string) $name);
        if (!isset($normalized[$key])) {
            $normalized[$key] = [];
        }
        foreach ($values as $value) {
            $normalized[$key][] = $value;
        }
    }
    echo $normalized['content-type'][0], "\n";
}
normalizeHeaders();
normalizeHeaders();
"#);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "application/json\napplication/json\n");
        assert!(
            output.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "{}",
            output.stderr
        );
    };
    check();
    without_ir_opt(check);
}
