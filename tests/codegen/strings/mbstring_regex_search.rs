//! Purpose:
//! Verifies public progressive mbregex state and captures through AOT and opaque eval.
//!
//! Called from:
//! - The focused codegen suite with its managed Oniguruma fixture.
//!
//! Key details:
//! - Expected fixture output was captured independently from PHP 8.5.10.
//! - Errors retain previous chains; result arrays must survive subsequent searches and mutation.

use crate::support::*;

/// Leaves eval source runtime-dependent while a separate native getter activates the shared provider.
fn program(source: &str, eval: bool) -> String {
    if !eval { return source.to_owned(); }
    let body = source.strip_prefix("<?php\n").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php mb_ereg_search_getpos(); $source = $argc > 0 ? '{body}' : ''; eval($source);")
}

/// Matches PHP for byte positions, exact capture keys, nullable options, callable forms, and empty groups.
#[test]
fn test_mbstring_regex_search_public_calls() {
    let source = include_str!("fixtures/mbstring_progressive_public.php");
    let expected = include_str!("fixtures/mbstring_progressive_public.out");
    for eval in [false, true] { assert_eq!(compile_and_run(&program(source, eval)), expected, "eval={eval}"); }
}

/// Preserves partial state updates, chained option errors, malformed patterns, and dynamic argument checks.
#[test]
fn test_mbstring_regex_search_runtime_errors() {
    let source = include_str!("fixtures/mbstring_progressive_errors.php");
    let expected = include_str!("fixtures/mbstring_progressive_errors.out");
    for eval in [false, true] {
        let output = compile_and_run_capture(&program(source, eval));
        assert!(output.success, "eval={eval}: {}\nstdout: {}", output.stderr, output.stdout);
        assert_eq!(output.stdout, expected, "eval={eval}");
        assert_eq!(output.stderr, "Warning: mb_ereg_search(): mbregex compile err: premature end of char-class\n", "eval={eval}");
    }
}

/// Alternates native and opaque eval searches over one retained subject, position, and capture set.
#[test]
fn test_mbstring_regex_search_shares_aot_eval_state() {
    assert_eq!(compile_and_run(include_str!("fixtures/mbstring_progressive_cross_eval.php")),
        include_str!("fixtures/mbstring_progressive_cross_eval.out"));
}

/// Keeps copied register arrays independent of later cache replacement and copy-on-write mutation.
#[test]
fn test_mbstring_regex_search_capture_ownership() {
    let source = r#"<?php
for ($i = 0; $i < 32; $i++) {
    mb_ereg_search_init("ab", "(?<word>a(b)?)(?<empty>)");
    $left = mb_ereg_search_regs();
    $right = mb_ereg_search_getregs();
    if (is_array($left) && is_array($right)) {
        $left[0] = "changed";
        mb_ereg_search_init("z", "z");
        echo $right[0], ":", $right["word"], ":";
        var_dump($right["empty"]);
    }
}


"#;
    for eval in [false, true] {
        let output = compile_and_run_with_heap_debug(&program(source, eval));
        assert!(output.success, "eval={eval}: {}", output.stderr);
        assert_eq!(output.stdout, "ab:ab:bool(false)\n".repeat(32), "eval={eval}");
    }
}

/// Returns boxed null and keeps a previous error alive after the caught chain root is released.
#[test]
fn test_mbstring_regex_search_exception_ownership() {
    let source = r#"<?php
try { mb_regex_set_options("Q"); }
catch (ValueError $error) { var_dump($error->getPrevious()); }
for ($i = 0; $i < 32; $i++) {
    try { mb_ereg_search(options: "iQ"); }
    catch (Throwable $error) {
        $previous = $error->gEtPrEvIoUs();
        unset($error);
        if ($previous !== null) { echo $previous->getMessage(), "\n"; }
    }
}
"#;
    for eval in [false, true] {
        let output = compile_and_run_with_heap_debug(&program(source, eval));
        assert!(output.success, "eval={eval}: {}\nstdout: {}", output.stderr, output.stdout);
        assert_eq!(output.stdout, format!("NULL\n{}", "Option \"Q\" is not supported\n".repeat(32)), "eval={eval}");
    }
}
