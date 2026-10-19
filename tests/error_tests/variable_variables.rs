//! Purpose:
//! Diagnostics for variable-variable (`$$name`, `${expr}`) and `compact()`/`extract()` forms
//! the frame-name desugar cannot lower without risking a wrong read or write.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every case must fail at compile time with an explicit message; none may compile into a
//!   dispatch that silently reads or writes the wrong storage.

use super::*;

/// A dynamic name in top-level code is rejected (main's locals are typed from the final scope).
#[test]
fn test_error_variable_variable_read_top_level() {
    expect_error(
        "<?php $a = 1; $n = 'a'; echo $$n;",
        "only supported inside functions and methods",
    );
}

/// A literal name is plain variable access and works anywhere.
#[test]
fn test_variable_variable_literal_name_top_level_is_accepted() {
    assert!(check_source("<?php $a = 1; echo ${'a'}; ${'b'} = 2; echo $b;").is_ok());
}

/// Dynamic names inside closures and arrow functions are refused.
#[test]
fn test_error_variable_variable_in_closure() {
    expect_error(
        "<?php function f() { $x = 1; $g = function (string $n) use ($x) { return $$n; }; return $g('x'); }",
        "not supported inside closures and arrow functions",
    );
    expect_error(
        "<?php function f() { $x = 1; $g = fn (string $n) => $$n; return $g('x'); }",
        "not supported inside closures and arrow functions",
    );
}

/// `??=` through a variable variable is refused.
#[test]
fn test_error_variable_variable_null_coalesce_assign() {
    expect_error(
        "<?php function f(string $n) { $a = null; $$n ??= 1; return $a; }",
        "Null-coalescing assignment (`??=`) to a variable variable is not supported",
    );
}

/// An assignment through a variable variable inside a larger expression is refused.
#[test]
fn test_error_variable_variable_assignment_in_expression() {
    expect_error(
        "<?php function f(string $n) { $a = 0; if ($$n = 1) { return $a; } return 0; }",
        "Assigning to a variable variable inside an expression is not supported",
    );
}

/// Passing a variable variable to a builtin's by-reference parameter is refused: the dispatch
/// would sort a copy.
#[test]
fn test_error_variable_variable_by_ref_builtin_argument() {
    expect_error(
        "<?php function f(string $n) { $a = [3, 1]; sort($$n); return $a; }",
        "Passing a variable variable by reference to sort() is not supported",
    );
}

/// `compact()` needs literal names.
#[test]
fn test_error_compact_runtime_names() {
    expect_error(
        "<?php function f(array $names) { $a = 1; return compact($names); }",
        "compact() is only supported with literal variable names",
    );
}

/// `extract(..., EXTR_SKIP)` must run before any non-parameter local is mentioned.
#[test]
fn test_error_extract_skip_after_locals() {
    expect_error(
        "<?php function f(array $c) { $host = 'x'; extract($c, EXTR_SKIP); return $host; }",
        "extract(..., EXTR_SKIP) is only supported before the function mentions any local",
    );
}

/// `extract(..., EXTR_SKIP)` after an `unset()` of a parameter, or after a closure mentions a
/// local other than its captures, is refused.
#[test]
fn test_error_extract_skip_after_unset_or_closure_local() {
    expect_error(
        "<?php function f($a) { unset($a); extract(['a' => 1], EXTR_SKIP); return $a; }",
        "extract(..., EXTR_SKIP) is only supported before the function mentions any local",
    );
    expect_error(
        "<?php function f($x) { $c = function () use ($x) { $y = 1; extract(['x' => 1], EXTR_SKIP); return $x . $y; }; return $c(); }",
        "extract(..., EXTR_SKIP) is only supported before the function mentions any local",
    );
}

/// extract() into a local the function also unset()s is refused: the conditional store is
/// lost once another statement follows.
#[test]
fn test_error_extract_into_unset_local() {
    expect_error(
        "<?php function f($a) { unset($a); extract(['a' => 2, 'b' => 3]); return $a . $b; }",
        "extract() is not supported in a function that also unset()s $a",
    );
    // A loop may run its unset() before the next iteration's extract().
    expect_error(
        "<?php function f(array $rows) { $o = ''; foreach ($rows as $r) { extract($r); $o .= $a ?? '-'; unset($a); } return $o; }",
        "extract() is not supported in a function that also unset()s $a",
    );
}

/// Every mode other than EXTR_OVERWRITE / EXTR_SKIP (and EXTR_REFS combinations), and an
/// extract() in an arrow function, are refused.
#[test]
fn test_error_extract_other_modes_and_arrow() {
    for mode in [
        "EXTR_PREFIX_SAME, 'p'",
        "EXTR_PREFIX_ALL, 'p'",
        "EXTR_PREFIX_INVALID, 'p'",
        "EXTR_IF_EXISTS",
        "EXTR_PREFIX_IF_EXISTS, 'p'",
        "EXTR_REFS",
        "EXTR_SKIP | EXTR_REFS",
        "EXTR_OVERWRITE | EXTR_REFS",
    ] {
        expect_error(
            &format!("<?php function f(array $c) {{ extract($c, {mode}); return 1; }}"),
            "extract() is only supported with EXTR_OVERWRITE or EXTR_SKIP",
        );
    }
    expect_error(
        "<?php function f(array $c) { $g = fn() => extract($c); return $g(); }",
        "extract() is only supported as a statement",
    );
}

/// `extract()` next to a dynamic frame read is refused (unknown keys would be lost).
#[test]
fn test_error_extract_with_dynamic_read() {
    expect_error(
        "<?php function f(array $c, string $n) { extract($c); return $$n; }",
        "extract() is not supported in a function that also reads variables dynamically",
    );
}

/// `extract()` whose result is used, or with an unsupported mode, is refused.
#[test]
fn test_error_extract_expression_and_mode() {
    expect_error(
        "<?php function f(array $c) { return extract($c); }",
        "extract() is only supported as a statement",
    );
    expect_error(
        "<?php function f(array $c) { extract($c, EXTR_PREFIX_ALL, 'p'); return 1; }",
        "extract() is only supported with EXTR_OVERWRITE or EXTR_SKIP",
    );
}

/// Destructuring into a variable variable at top level is refused without leaking the internal
/// frame pseudo-variable's name.
#[test]
fn test_error_variable_variable_destructuring_top_level() {
    match check_source("<?php $a = 0; $x = 'a'; [$$x] = [5]; var_dump($a);") {
        Ok(_) => panic!("expected an error"),
        Err(msg) => {
            assert!(msg.contains("only supported inside functions and methods"), "{msg}");
            assert!(!msg.contains("__elephc_varvar_frame"), "{msg}");
        }
    }
}

/// compact() of a local that is only conditionally assigned is refused: PHP omits it when it
/// is undefined, which elephc cannot test yet.
#[test]
fn test_error_compact_possibly_undefined_local() {
    expect_error(
        "<?php function g(bool $c) { if ($c) { $x = 'str'; } return compact('x'); }",
        "compact() of $x, which may be undefined at this point, is not supported",
    );
    expect_error(
        "<?php function h() { $r = compact('later'); $later = 1; return $r; }",
        "compact() of $later, which may be undefined at this point",
    );
}

/// compact() of a name extract() may have defined is refused too.
#[test]
fn test_error_compact_after_extract() {
    expect_error(
        "<?php function h(array $d) { $x = 1; extract($d); return compact('x', 'title'); }",
        "compact() of $title, which may be undefined at this point",
    );
}
