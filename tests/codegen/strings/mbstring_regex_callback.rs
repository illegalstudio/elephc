//! Purpose:
//! Checks replacement callbacks through native and opaque-eval mbregex invocation.
//!
//! Called from:
//! - The focused codegen suite with the managed Oniguruma provider.
//!
//! Key details:
//! - PHP 8.5.10 supplies fixture output independently of either compiler backend.
//! - Heap-debug cases exercise callback result aliasing and ordinary exception cleanup.

use crate::support::*;

/// Selects native source or an opaque eval body while retaining the shared regex capability.
fn program(source: &str, eval: bool) -> String {
    if !eval { return source.to_owned(); }
    let body = source.strip_prefix("<?php\n").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php mb_ereg_search_getpos(); $source = $argc > 0 ? '{body}' : ''; eval($source);")
}

/// Compares callback forms, capture arrays, literal replacement bytes, and callback scalar casts.
#[test]
fn test_mbstring_regex_callback_public_calls() {
    for eval in [false, true] {
        let output = compile_and_run_capture(&program(include_str!("fixtures/mbstring_callback_public.php"), eval));
        assert!(output.success, "eval={eval}: {}\n{}", output.stderr, output.stdout);
        assert_eq!(output.stdout, include_str!("fixtures/mbstring_callback_public.out"), "eval={eval}");
        assert!(output.stderr.is_empty(), "eval={eval}: {}", output.stderr);
    }
}

/// Checks ordered argument validation, invalid patterns, and propagation of ordinary PHP callback errors.
#[test]
fn test_mbstring_regex_callback_errors() {
    for eval in [false, true] {
        let output = compile_and_run_capture(&program(include_str!("fixtures/mbstring_callback_errors.php"), eval));
        assert!(output.success, "eval={eval}: {}\n{}", output.stderr, output.stdout);
        assert_eq!(output.stdout, include_str!("fixtures/mbstring_callback_errors.out"), "eval={eval}");
        assert!(output.stderr.is_empty(), "eval={eval}: {}", output.stderr);
    }
}

/// Retains callback-returned capture aliases and retires each successful result under heap debugging.
#[test]
fn test_mbstring_regex_callback_ownership() {
    let source = r#"<?php
function retain_capture(array $matches): string { return $matches[0]; }
for ($i = 0; $i < 16; $i++) {
    $result = mb_ereg_replace_callback("(café)", "retain_capture", "café café");
    $copy = $result;
    $result = mb_ereg_replace_callback("a", function(array $m): string { return "X"; }, "aba");
    var_dump($copy, $result);
}

"#;
    for eval in [false, true] {
        let output = compile_and_run_with_heap_debug(&program(source, eval));
        assert!(output.success, "eval={eval}: {}\n{}", output.stderr, output.stdout);
        assert_eq!(output.stdout, "string(11) \"café café\"\nstring(3) \"XbX\"\n".repeat(16), "eval={eval}");
    }
}

/// Detects retained native owners by comparing short and repeated callback runs after request cleanup.
#[test]
fn test_mbstring_regex_callback_retained_owners() {
    for eval in [false, true] {
        let mut residual = Vec::new();
        for count in [1, 16] {
            let source = format!("<?php\nfunction copy_capture(array $m): string {{ return $m[0]; }}\nfor ($i = 0; $i < {count}; $i++) {{ $result = mb_ereg_replace_callback(\"(café)\", \"copy_capture\", \"café café\"); }}\nvar_dump($result);");
            let output = compile_and_run_with_gc_stats(&program(&source, eval));
            assert!(output.success, "eval={eval}: {}", output.stderr);
            assert_eq!(output.stdout, "string(11) \"café café\"\n");
            let (allocated, freed) = parse_gc_stats(&output.stderr);
            residual.push(allocated as i64 - freed as i64);
        }
        assert_eq!(residual[0], residual[1], "callback owners retained, eval={eval}");
    }
}

/// Preserves callback output and global writes when the replacement result is discarded.
#[test]
fn test_mbstring_regex_callback_discarded_result_effects() {
    let source = r#"<?php
$calls = 0;
function observe_capture(array $m): string { global $calls; $calls++; echo "called\n"; return "X"; }
mb_ereg_replace_callback("a", "observe_capture", "a a");
var_dump($calls);
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(source, eval)), "called\ncalled\nint(2)\n", "eval={eval}");
    }
}
