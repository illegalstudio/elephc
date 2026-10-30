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
