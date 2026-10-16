//! Purpose:
//! End-to-end coverage for `declare(strict_types=1)`: what a strict file still binds without a
//! conversion, that the directive is scoped to the physical file it appears in, and that
//! callbacks invoked by internal functions keep PHP's coercive binding.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected value is verbatim `LC_ALL=C php` 8.4.20 stdout.
//! - The conversions a strict file *rejects* are compile errors, so they are pinned in
//!   `tests/error_tests/type_system.rs` instead of here.
//! - `$argc` keeps an argument runtime-valued where the point is that the binding decision is
//!   not made by AST constant folding.

use crate::support::*;

/// Verifies the bindings PHP still performs under `strict_types=1`: exact scalar matches, the
/// `int`→`float` widening, and arguments the programmer converted with an explicit cast.
#[test]
fn test_strict_types_accepts_exact_matches_and_int_to_float() {
    let out = compile_and_run(
        r#"<?php
        declare(strict_types=1);
        function ti(int $i) { return $i; }
        function tf(float $f) { return $f; }
        function ts(string $s) { return $s; }
        function tb(bool $b) { return $b ? "T" : "F"; }
        echo ti(42), "|", tf(42), "|", tf(4.5), "|", ts("x"), "|", tb(true), "|", ti((int)"7"), "|", ts((string)9);
        "#,
    );
    assert_eq!(out, "42|42|4.5|x|T|7|9");
}

/// Verifies the `int`→`float` widening survives strict mode for a runtime value too, not just
/// for a literal a constant fold could have retyped.
#[test]
fn test_strict_types_widens_runtime_int_to_float() {
    let out = compile_and_run(
        r#"<?php
        declare(strict_types=1);
        function tf(float $f) { return $f * 2; }
        $n = 21 * $argc;
        echo tf($n);
        "#,
    );
    assert_eq!(out, "42");
}

/// Verifies `declare(strict_types=0)` is the explicit spelling of PHP's default, so every
/// coercive binding still fires in a file that declares it.
#[test]
fn test_strict_types_zero_keeps_coercive_binding() {
    let out = compile_and_run(
        r#"<?php
        declare(strict_types=0);
        function ti(int $i) { return $i; }
        function ts(string $s) { return $s; }
        echo ti(true), "|", ti("42"), "|", ts(42), "|", ti(5.0);
        "#,
    );
    assert_eq!(out, "1|42|42|5");
}

/// Verifies the directive does not propagate into an included file: the call is written in the
/// included coercive file, so PHP coerces `true` to `1` even though the includer is strict.
#[test]
fn test_strict_types_does_not_reach_an_included_file() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\ndeclare(strict_types=1);\nrequire __DIR__ . '/lib.php';\necho from_loose_file();\n",
            ),
            (
                "lib.php",
                "<?php\nfunction coerce_here(int $i) { return $i; }\nfunction from_loose_file() { return coerce_here(true); }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "1");
}

/// Verifies strictness follows the call site, not the callee: a coercive file calling a
/// function declared in a strict file still gets PHP's coercion.
#[test]
fn test_strict_types_of_the_callee_file_does_not_bind_the_caller() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire __DIR__ . '/lib.php';\necho strict_int(true);\n",
            ),
            (
                "lib.php",
                "<?php\ndeclare(strict_types=1);\nfunction strict_int(int $i) { return $i; }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "1");
}

/// Verifies a callback invoked by an internal function keeps coercive binding under
/// `strict_types=1`. PHP calls it from the engine's own frame, which never carries the
/// directive, so `array_map('g', [true, false, 2])` converts the booleans instead of throwing.
#[test]
fn test_strict_types_does_not_reach_an_engine_invoked_callback() {
    let out = compile_and_run(
        r#"<?php
        declare(strict_types=1);
        function g(int $i) { return $i; }
        echo implode(",", array_map('g', [true, false, 2]));
        "#,
    );
    assert_eq!(out, "1,0,2");
}

/// Complete control tails remain inside a leading declare before a strict directive.
#[test]
fn test_strict_types_after_complete_declare_control_bodies() {
    for (source, expected) in [
        (r#"<?php declare(ticks=1) if (true) echo 1; else echo 2; declare(strict_types=1); echo 3;"#, "13"),
        (r#"<?php declare(ticks=1) do {echo 1;} while (false); declare(strict_types=1); echo 2;"#, "12"),
        (r#"<?php declare(ticks=1) if (true): echo 1; else: echo 2; endif; declare(strict_types=1); echo 3;"#, "13"),
        (r#"<?php declare(ticks=1) try {echo 1;} catch (Exception $e) {echo 9;} finally {echo 2;} declare(strict_types=1); echo 3;"#, "123"),
        (r#"<?php declare(ticks=1) if (false) {echo 9;} elseif (true) {echo 1;} else {echo 8;} declare(strict_types=1); echo 2;"#, "12"),
        (r#"<?php declare(ticks=1) if (true) if (false) echo 9; else echo 1; else echo 8; declare(strict_types=1); echo 2;"#, "12"),
        (r#"<?php declare(ticks=1) for ($i=0;$i<2;$i++): echo $i; endfor; declare(strict_types=1); echo 2;"#, "012"),
        (r#"<?php declare(ticks=1) while (false): echo 9; endwhile; declare(strict_types=1); echo 2;"#, "2"),
        (r#"<?php declare(ticks=1) foreach ([1,2] as $i): echo $i; endforeach; declare(strict_types=1); echo 3;"#, "123"),
        (r#"<?php declare(ticks=1) switch (1): case 1: echo 1; break; default: echo 9; endswitch; declare(strict_types=1); echo 2;"#, "12"),
    ] {
        assert_eq!(compile_and_run(source), expected, "{source}");
    }
}

/// Inline HTML inside a declare's alternative if body does not end that declare early.
#[test]
fn test_strict_types_after_inline_html_control_body() {
    let out = compile_and_run(
        "<?php declare(ticks=1) if (true): ?>a<?php else: ?>b<?php endif; declare(strict_types=1); echo 'c';",
    );
    assert_eq!(out, "ac");
}

/// Closing expression braces do not end a declare body before a following operator.
#[test]
fn test_strict_types_after_declare_expression_tails() {
    for source in [
        "<?php declare(ticks=1) match (1) {1 => 1} + 2; declare(strict_types=1); echo 3;",
        "<?php declare(ticks=1) $f = function () {} ?? null; declare(strict_types=1); echo 3;",
    ] {
        assert_eq!(compile_and_run(source), "3");
    }
}
