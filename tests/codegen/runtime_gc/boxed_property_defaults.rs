//! Purpose:
//! Regression tests for issue #688: an array literal defaulting a `mixed` or union-typed
//! property allocates its container and then boxes it into a Mixed cell, and the box takes
//! its OWN reference — so the object-construction path must use the owned boxer.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Array fixtures assert a clean heap; enum defaults leave only the fixed singleton cache.
//! - Objects are constructed in a LOOP on purpose: the plain boxer retains without
//!   releasing and leaks exactly one block per object, which a single construction hides.
//! - Both literal spellings are covered together — keyed (`["k" => 1]`) and positional
//!   (`[1, 2]`) reach different emitters — so a future change cannot fix one and lose the
//!   other's ownership.

use crate::support::compile_and_run_with_heap_debug;

/// Verifies the boxed array-literal defaults release the container they allocate.
///
/// Each object allocates the literal and boxes it into a Mixed cell, and the box takes its own
/// reference — so the OWNED boxer is required. The plain one retains without releasing and leaks
/// one block per object, which a single-iteration total hides.
#[test]
fn test_array_literal_defaults_on_union_properties_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class A { public ?array $x = ["k" => 1, "j" => "s"]; }
class B { public mixed $y = ["a" => 1]; }
class C { public ?array $z = [1, 2]; }
for ($i = 0; $i < 32; $i++) {
    $a = new A();
    $b = new B();
    $c = new C();
}
echo count($a->x), count($b->y), count($c->z), "\n";
"#,
    );
    assert_eq!(out.stdout, "212\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "boxed array-literal property default leaked: {}",
        out.stderr
    );
}

/// Follow-up for #1152: a keyed default on an INSTANCE `array|string` union slot.
///
/// `LiteralDefaultValue::BoxedAssocArray` reaches the union emitter too, but only the
/// `?array` / `mixed` spellings had keyed fixtures — the union slot was covered by its
/// POSITIONAL sibling alone. The container is allocated per object and boxed into the slot,
/// so a plain boxer here leaks one block per construction.
#[test]
fn test_keyed_array_default_on_a_union_property_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public array|string $u = ["k" => 2, "j" => "s"]; }
for ($i = 0; $i < 32; $i++) {
    $t = new T();
}
echo count($t->u), ";";
var_dump($t->u["k"], $t->u["j"]);
"#,
    );
    assert_eq!(
        out.stdout,
        "2;int(2)\nstring(1) \"s\"\n",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "a keyed default on a union slot leaked: {}",
        out.stderr
    );
}

/// Follow-up for #1152: keyed defaults on STATIC `?array` and `array|string` slots.
///
/// The static emitter is a separate path, and it too had only positional keyed coverage. A
/// static's storage is initialized once and never released — that is true of a positional
/// default as well, so `leak summary: clean` is the wrong question here. The right one is
/// whether the cost is FIXED: the same program is run with the read loop at 32 and at 64
/// iterations and the live totals have to be identical, which a per-read allocation or a
/// re-initialized default would break.
///
/// Every value expectation is the host PHP 8.5.10 output for the same fixture.
#[test]
fn test_keyed_array_defaults_on_static_properties_cost_one_fixed_allocation() {
    let program = |iterations: usize| {
        format!(
            r#"<?php
class S {{ public static ?array $x = ["k" => 1, "j" => "s"]; }}
class U {{ public static array|string $v = ["m" => 3]; }}
$n = 0;
for ($i = 0; $i < {iterations}; $i++) {{
    $n += count(S::$x) + count(U::$v);
    foreach (S::$x as $k => $v) {{ $n++; }}
}}
echo $n, ";";
var_dump(S::$x["k"], S::$x["j"], U::$v["m"]);
"#
        )
    };

    let short = compile_and_run_with_heap_debug(&program(32));
    assert_eq!(
        short.stdout,
        "160;int(1)\nstring(1) \"s\"\nint(3)\n",
        "stderr: {}",
        short.stderr
    );

    let long = compile_and_run_with_heap_debug(&program(64));
    assert_eq!(
        long.stdout,
        "320;int(1)\nstring(1) \"s\"\nint(3)\n",
        "stderr: {}",
        long.stderr
    );

    let live = |stderr: &str| {
        stderr
            .lines()
            .find(|line| line.starts_with("HEAP DEBUG: leak summary:"))
            .unwrap_or_else(|| panic!("no heap summary in: {stderr}"))
            .to_string()
    };
    assert_eq!(
        live(&short.stderr),
        live(&long.stderr),
        "a static keyed default must cost the same however often it is read"
    );
}

/// Follow-up for #1153: an INTEGER-keyed assoc literal defaulting a union or `mixed` slot.
///
/// `[1 => "a"]` is still `BoxedAssocArray` — an explicit key of any kind makes the default a
/// hash, not packed storage — but every keyed fixture so far used string keys. An integer key
/// travels through the hash's numeric-key normalization instead of its string path, so a
/// default emitter that assumed string keys would either refuse the shape or store a key PHP
/// does not agree with. The mixed int+string literal pins the insertion ORDER too, which is
/// what `foreach` observes and what a rebuilt-by-key-kind default would lose.
///
/// Every expectation is the host PHP 8.5.10 output for the same fixture.
#[test]
fn test_integer_keyed_array_defaults_on_union_properties_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class I { public ?array $a = [1 => "a"]; }
class M { public mixed $b = [5 => "five", 7 => "seven"]; }
class X { public array|string $c = [2 => "two", "k" => "kay", 0 => "zero"]; }
for ($i = 0; $i < 32; $i++) {
    $x = new I();
    $y = new M();
    $z = new X();
}
echo count($x->a), count($y->b), count($z->c), ";";
var_dump($x->a[1], $y->b[5], $y->b[7], $z->c[2], $z->c["k"], $z->c[0]);
foreach ($z->c as $k => $v) { echo "[", var_export($k, true), "=", $v, "]"; }
echo "\n";
"#,
    );
    assert_eq!(
        out.stdout,
        concat!(
            "123;string(1) \"a\"\n",
            "string(4) \"five\"\n",
            "string(5) \"seven\"\n",
            "string(3) \"two\"\n",
            "string(3) \"kay\"\n",
            "string(4) \"zero\"\n",
            "[2=two]['k'=kay][0=zero]\n",
        ),
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "an integer-keyed default on a union slot leaked: {}",
        out.stderr
    );
}

/// Repeated boxed-default allocation and replacement leaves only the fixed enum cache.
#[test]
fn test_enum_property_defaults_ownership() {
    let source = r#"<?php
enum Level { case Low; case High; }
class Config {
    public ?Level $value = Level::Low;
    public mixed $other = Level::Low;
    public static ?Level $shared = Level::Low;
}
for ($i = 0; $i < ITERATIONS; $i++) {
    $c = new Config();
    $c->value = Level::High;
    $c->other = Level::High;
    Config::$shared = Level::High;
    Config::$shared = Level::Low;
}
echo Level::Low->name, ':', Level::High->name;
"#;
    for iterations in [8, 64] {
        let out = compile_and_run_with_heap_debug(&source.replace("ITERATIONS", &iterations.to_string()));
        assert!(out.success, "{}", out.stderr);
        assert_eq!(out.stdout, "Low:High", "{}", out.stderr);
        // As in the existing enum GC fixtures, the two cached singleton objects stay live.
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: live_blocks=2 live_bytes=80"),
            "{}", out.stderr);
    }
}
