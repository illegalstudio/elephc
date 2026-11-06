//! Purpose:
//! Regresses generic type inference from static named argument unpacks.
//! Covers functions, methods, class construction, and constructor-local templates.
//!
//! Called from:
//! - `cargo test --test codegen_tests generics_named_spreads`.
//!
//! Key details:
//! - Inference uses the shared spread expansion without changing source evaluation order.
//! - PHPDoc inference remains active in strict PHP mode.

use crate::support::*;

/// Free functions infer from declaration positions after expanding named unpacks.
#[test]
fn test_generic_named_spreads_functions_and_duplicate_keys() {
    let source = r#"<?php
function box<T>(int $n, T $v): T { return $v; }
echo box(...["v" => "abc", "n" => 1]), "|",
    box(...["v" => 7, "v" => "last", "n" => 2]);
"#;
    assert_eq!(compile_and_run(source), "abc|last");
}

/// Instance and static methods share the same named-unpack inference as functions.
#[test]
fn test_generic_named_spreads_instance_and_static_methods() {
    let source = r#"<?php
class C {
    public function box<T>(int $n, T $v): T { return $v; }
    public static function copy<T>(int $n, T $v): T { return $v; }
}
echo (new C())->box(...["v" => "abc", "n" => 1]), "|",
    C::copy(...["v" => 7, "n" => 2]) + 1;
"#;
    assert_eq!(compile_and_run(source), "abc|8");
}

/// Generic classes, their static factories and constructor templates infer from named unpacks.
#[test]
fn test_generic_named_spreads_constructions_and_static_factories() {
    let source = r#"<?php
class Box<T> {
    public function __construct(int $n, public T $v) {}
    public static function of(int $n, T $v): Box<T> { return new Box($n, $v); }
}
class Value {
    public function __construct<T>(int $n, public T $v) {}
}
$box = new Box(...["v" => "abc", "n" => 1]);
$factory = Box::of(...["v" => 7, "n" => 2]);
$value = new Value(...["v" => "def", "n" => 3]);
echo $box->v, "|", $factory->v + 1, "|", $value->v;
"#;
    assert_eq!(compile_and_run(source), "abc|8|def");
}

/// Portable templates preserve unpack expression evaluation order in both CLI modes.
#[test]
fn test_generic_named_spreads_docblock_and_evaluation_order() {
    let source = r#"<?php
/**
 * @template T
 * @param T $v
 * @return T
 */
function box(int $n, $v) { return $v; }
function word(): string { echo "v"; return "abc"; }
function number(): int { echo "n"; return 1; }
echo box(...["v" => word(), "n" => number()]);
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), "vnabc");
    }
}

/// A positional unpack after a named one starts after the named parameter, as the call planner
/// places it, on the function, method and construction paths.
///
/// Inference filled the first free slot instead, so `show(...["b" => 8], ...["Q"])` read `$a`
/// for `<T>` (left undetermined) where the call puts "Q" in `$c`, and a longer tail bound `<T>`
/// to the next integer and then refused "Q".
#[test]
fn test_positional_unpack_after_named_unpack_binds_from_the_planner_cursor() {
    let out = compile_and_run(
        r#"<?php
function show<T>(int $a = 1, int $b = 2, T $c = "z") { return "$a/$b/$c"; }
function longer<T>(int $a = 1, int $b = 2, T $c = "z", int $d = 0) { return "$a/$b/$c/$d"; }
function hole<T>(int $a = 1, int $b = 2, int $c = 3, T $d = "z") { return "$a/$b/$c/$d"; }
class C { public static function show<T>(int $a = 1, int $b = 2, T $c = "z") { return "$a/$b/$c"; } }
class Box<T> { public function __construct(public int $a = 1, public int $b = 2, public T $c = "z") {} }
$box = new Box(...["b" => 8], ...["S"]);
echo show(...["b" => 8], ...["Q"]), "|", C::show(...["b" => 8], ...["R"]), "|",
    "$box->a/$box->b/$box->c", "|", longer(...["b" => 8], ...["Q", 4]), "|",
    hole(7, ...["c" => 9], ...["Q"]);
"#,
    );
    assert_eq!(out, "1/8/Q|1/8/R|1/8/S|1/8/Q/4|7/2/9/Q");
}
