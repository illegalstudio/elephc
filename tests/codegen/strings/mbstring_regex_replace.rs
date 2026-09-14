//! Purpose:
//! Compares mbregex replacement calls, coercions, diagnostics, and result ownership with PHP.
//!
//! Called from:
//! - The codegen suite using the managed Oniguruma fixture.
//!
//! Key details:
//! - Every source runs natively and through opaque eval with exact oracle output.
//! - Stringable changes must preserve the entry encoding while compilation uses live settings.

use crate::support::*;

/// Keeps eval source runtime-dependent and selects the shared provider with a reachable native operation.
fn program(source: &str, eval: bool) -> String {
    if !eval { return source.to_owned(); }
    let body = source.strip_prefix("<?php\n").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php mb_ereg_search_getpos(); $source = $argc > 0 ? '{body}' : ''; eval($source);")
}

/// Checks namespaced/named/callable calls, capture expansion, encoding snapshots, and cache invalidation.
#[test]
fn test_mbstring_regex_replace_public_calls() {
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(include_str!("fixtures/mbstring_replace_public.php"), eval)),
            include_str!("fixtures/mbstring_replace_public.out"), "eval={eval}");
    }
}

/// Checks dynamic arity/types, null subjects, nullable options, deprecations, and search-limit warnings.
#[test]
fn test_mbstring_regex_replace_runtime_errors() {
    for eval in [false, true] {
        let output = compile_and_run_capture(&program(include_str!("fixtures/mbstring_replace_errors.php"), eval));
        assert!(output.success, "eval={eval}: {}\n{}", output.stderr, output.stdout);
        assert_eq!(output.stdout, include_str!("fixtures/mbstring_replace_errors.out"), "eval={eval}");
        assert_eq!(output.stderr, include_str!("fixtures/mbstring_replace_errors.err"), "eval={eval}");
    }
}

/// Preserves retained strings across later replacements and releases every fresh result under heap debugging.
#[test]
fn test_mbstring_regex_replace_ownership() {
    let source = r#"<?php
for ($i = 0; $i < 32; $i++) {
    $result = mb_eregi_replace("(café)", "[\\1]", "CAFÉ café");
    $copy = $result;
    $result = mb_ereg_replace("a", "X", "aba");
    var_dump($copy, $result, mb_ereg_replace("a", "X", chr(255)));
}
"#;
    for eval in [false, true] {
        let output = compile_and_run_with_heap_debug(&program(source, eval));
        assert!(output.success, "eval={eval}: {}\n{}", output.stderr, output.stdout);
        assert_eq!(output.stdout, "string(15) \"[CAFÉ] [café]\"\nstring(3) \"XbX\"\nNULL\n".repeat(32), "eval={eval}");
    }
}
