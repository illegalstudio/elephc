//! Purpose:
//! Regression coverage for php-src's two comparison modes in variadic `min()` / `max()`:
//! the numeric mode, and the generic `zend_compare` mode a call switches to for good at its
//! first non-numeric argument or at an integer a double cannot represent.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - The modes differ only on an unordered (`NAN`) comparison: a two-argument `max()`
//!   returns its right operand numerically and its left one generically, and a longer
//!   `max()` keeps its running result numerically and takes the candidate generically.
//! - The matrices print one MD5 per row group, so a mismatch names the group. Expected
//!   output comes from PHP 8.5; `ELEPHC_PHP_CHECK=1` re-runs the oracle on the same source.
//! - Every lowering is covered: runtime-typed operands through the helper, statically typed
//!   numbers into a heap result and into a frame cell, and the GMP prelude reducer.

use super::min_max_selection::{function_assembly, php_oracle, run};
use super::*;

/// Every pair and triple of a mixed value set through runtime-typed `min()` and `max()`.
const RUNTIME_MATRIX: &str = r#"<?php
function show(mixed $v): string {
    if (is_bool($v)) { return $v ? "b:true" : "b:false"; }
    if (is_null($v)) { return "null"; }
    if (is_int($v)) { return "i:" . $v; }
    if (is_float($v)) { return is_nan($v) ? "f:NAN" : "f:" . $v; }
    return "s:" . $v;
}
function mx2(mixed $a, mixed $b): mixed { return max($a, $b); }
function mn2(mixed $a, mixed $b): mixed { return min($a, $b); }
function mx3(mixed $a, mixed $b, mixed $c): mixed { return max($a, $b, $c); }
function mn3(mixed $a, mixed $b, mixed $c): mixed { return min($a, $b, $c); }
$big = PHP_INT_MAX - $argc + 1;
$pairs = [1, $big, 1.5, NAN, "x", "10", true, false, null, 9.2233720368547758E+18, -$big];
foreach ($pairs as $i => $a) {
    $row = "";
    foreach ($pairs as $b) { $row .= show(mx2($a, $b)) . " " . show(mn2($a, $b)) . "\n"; }
    echo "pair ", $i, " ", md5($row), "\n";
}
$triples = [1, $big, 2.5, NAN, "x", true, 9.2233720368547758E+18];
foreach ($triples as $i => $a) {
    $row = "";
    foreach ($triples as $b) {
        foreach ($triples as $c) { $row .= show(mx3($a, $b, $c)) . " " . show(mn3($a, $b, $c)) . "\n"; }
    }
    echo "triple ", $i, " ", md5($row), "\n";
}
"#;

/// PHP 8.5 output of `RUNTIME_MATRIX`.
const RUNTIME_MATRIX_CHECKSUMS: &str = "pair 0 28b4e12bf2d95c01f14ca554b8cbc6b2
pair 1 2347f003b06f3d971ae5d2be7bc0b1db
pair 2 790f1243e8277f6c52766ce4d066bf2f
pair 3 01a8a7ee97beccf1326f65a6728f71c5
pair 4 ecc4528db47d4addec1411a0d9e15fc4
pair 5 f1c100d8b463880b03856cb8542864e7
pair 6 147fa33778285e7053c935af6cdd053f
pair 7 b68165af3066c0b30192aa89ee1be0d0
pair 8 b9b47892a91695c5c886456aa0e089ad
pair 9 95df7a244a294ee53efb6c12177855d3
pair 10 0e492b12b76897fe9a82b5a08f4ed86d
triple 0 cedd427c6c5b1ae7463f727cfbc41a15
triple 1 2d68c2d43a1807f665ee20117a06b02f
triple 2 180c5d73f14f01d26cb09531338e8644
triple 3 cc92afc29c11b108a0bd2429b0b7fb8a
triple 4 2381499ba647bf7039027789d3d4f988
triple 5 09e951ee39149ec2030572c645adcdc7
triple 6 dc11280e40962afa40453a407e29ee56
";

/// PHP 8.5 output of `static_matrix_source()`.
const STATIC_MATRIX_CHECKSUMS: &str = "if 3957b1190529458f81d254583d58fe66
fi 5546bbd17a2640d4dda88722f7d1e33b
iif 7a69c9751f5188fc6460156a390e9956
ifi 53ba22533bf58f8a3ef16de1a0da9644
iff 0506a0169626d71021342d25aff1895f
fii 3e8489ab715bc6779c738feecb9b3496
fif af8017a6eb59c6a2adec69680d9e43ad
ffi 7262fda5301a54d307227ab8df3fb820
";

/// The parameter shapes of the static matrix: `i` is an `int` parameter, `f` a `float` one.
const STATIC_SHAPES: [&str; 8] = ["if", "fi", "iif", "ifi", "iff", "fii", "fif", "ffi"];

/// Builds every `int`/`float` parameter shape of a two- and three-argument selection.
///
/// `h_*` returns the selection as `mixed` (a heap result). `c_*` observes each selection
/// directly with a type predicate, a float cast and a string cast, the uses that keep the
/// result in a frame cell. The values include integers a double cannot represent.
fn static_matrix_source() -> String {
    let mut source = String::from(
        "<?php\nfunction show(mixed $v): string { if (is_int($v)) { return \"i:\" . $v; } \
         return is_nan($v) ? \"f:NAN\" : \"f:\" . $v; }\n",
    );
    for shape in STATIC_SHAPES {
        let names: Vec<String> = (0..shape.len()).map(|k| format!("${}", (b'a' + k as u8) as char)).collect();
        let params: Vec<String> = shape.chars().zip(&names)
            .map(|(kind, name)| format!("{} {name}", if kind == 'i' { "int" } else { "float" }))
            .collect();
        let (params, args) = (params.join(", "), names.join(", "));
        for function in ["max", "min"] {
            let select = format!("{function}({args})");
            source.push_str(&format!("function h_{function}_{shape}({params}): mixed {{ return {select}; }}\n"));
            source.push_str(&format!(
                "function c_{function}_{shape}({params}): string {{ return (is_int({select}) ? \"i:\" : \"f:\") \
                 . (is_nan((float) {select}) ? \"NAN\" : (string) {select}); }}\n"
            ));
        }
    }
    source.push_str("$big = PHP_INT_MAX - $argc + 1;\n");
    source.push_str("$ints = [1, 9007199254740992, 9007199254740993, $big, -$big];\n");
    source.push_str("$floats = [1.5, NAN, 9007199254740992.0, 9.2233720368547758E+18, -1.0];\n");
    for shape in STATIC_SHAPES {
        let names: Vec<String> = (0..shape.len()).map(|k| format!("$x{k}")).collect();
        let call = names.join(", ");
        source.push_str("$row = \"\";");
        for (kind, name) in shape.chars().zip(&names) {
            let values = if kind == 'i' { "$ints" } else { "$floats" };
            source.push_str(&format!(" foreach ({values} as {name}) {{"));
        }
        source.push_str(&format!(
            " $row .= show(h_max_{shape}({call})) . \" \" . show(h_min_{shape}({call})) . \" \" \
             . c_max_{shape}({call}) . \" \" . c_min_{shape}({call}) . \"\\n\";"
        ));
        source.push_str(&" }".repeat(shape.len()));
        source.push_str(&format!("\necho \"{shape} \", md5($row), \"\\n\";\n"));
    }
    source
}

/// The cases a generic comparison decides differently from a numeric one, spelled out.
#[test]
fn test_min_max_generic_comparison_follows_php_around_nan() {
    let source = r#"<?php
function pair(mixed $a, mixed $b): mixed { return max($a, $b); }
function low(mixed $a, mixed $b): mixed { return min($a, $b); }
function three(mixed $a, mixed $b, mixed $c): mixed { return max($a, $b, $c); }
function typed(int $i, float $f): mixed { return max($i, $f); }
function typedRev(float $f, int $i): mixed { return max($f, $i); }
$nan = sqrt(-1.0 * $argc);
$big = PHP_INT_MAX - $argc + 1;
var_dump(pair($nan, "x"), pair("x", $nan), pair($nan, $big), pair($big, $nan), pair($nan, 1), pair(1, $nan));
var_dump(low($nan, "x"), low("x", $nan), low($nan, $big), low($big, $nan));
var_dump(three(1, "x", $nan), three($big, 2.5, $nan), three($nan, 1, $big), three("x", $nan, 2.5), three(1, $nan, 2));
var_dump(typed($big, $nan), typed(1, $nan), typedRev($nan, $big), typedRev($nan, 1));
"#;
    let expected = "float(NAN)\nstring(1) \"x\"\nfloat(NAN)\nint(9223372036854775807)\nint(1)\nfloat(NAN)\n\
string(1) \"x\"\nfloat(NAN)\nint(9223372036854775807)\nfloat(NAN)\n\
float(NAN)\nfloat(NAN)\nint(9223372036854775807)\nfloat(2.5)\nint(2)\n\
int(9223372036854775807)\nfloat(NAN)\nfloat(NAN)\nint(1)\n";
    php_oracle(source, expected);
    for optimized in [true, false] {
        assert_eq!(run(source, optimized), expected, "optimized={optimized}");
    }
}

/// Runtime-typed operands agree with PHP on every pair and triple of the value set.
#[test]
fn test_min_max_runtime_typed_matrix_matches_php() {
    php_oracle(RUNTIME_MATRIX, RUNTIME_MATRIX_CHECKSUMS);
    for optimized in [true, false] {
        assert_eq!(run(RUNTIME_MATRIX, optimized), RUNTIME_MATRIX_CHECKSUMS, "optimized={optimized}");
    }
}

/// Statically typed `int`/`float` operands agree with PHP in a heap result and in a frame cell.
#[test]
fn test_min_max_static_typed_matrix_matches_php() {
    let source = static_matrix_source();
    php_oracle(&source, STATIC_MATRIX_CHECKSUMS);
    for optimized in [true, false] {
        assert_eq!(run(&source, optimized), STATIC_MATRIX_CHECKSUMS, "optimized={optimized}");
    }
}

/// A program that declares GMP sends the same calls through the prelude reducer, which must
/// print the same matrix.
#[test]
fn test_min_max_gmp_reducer_matrix_matches_php() {
    if skip_without_gmp_native("gmp min/max matrix") {
        return;
    }
    let source = RUNTIME_MATRIX.replacen(
        "<?php\n",
        "<?php\n$g = gmp_init(7);\necho gmp_strval($g), \"\\n\";\n",
        1,
    );
    let expected = format!("7\n{RUNTIME_MATRIX_CHECKSUMS}");
    for optimized in [false, true] {
        let output = if optimized {
            compile_and_run_with_heap_debug(&source)
        } else {
            without_ir_opt(|| compile_and_run_with_heap_debug(&source))
        };
        assert!(output.success, "optimized={optimized}: {}", output.stderr);
        assert_eq!(output.stdout, expected, "optimized={optimized}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "optimized={optimized}: {}", output.stderr);
    }
}

/// A selection among three proven numbers keeps its frame cell: the helper picks the winner
/// and the cell copies it, so a hot loop allocates nothing. The computed operands are not the
/// last ones on purpose: call lowering pins an owned argument while the later ones are
/// evaluated, and that pin must not send the checked difference to a heap box.
#[test]
fn test_min_max_three_number_selection_is_allocation_free() {
    let source = r#"<?php
function hot(int $count): void {
    $sum = 0.0;
    $start = memory_get_allocations();
    for ($i = 0; $i < $count; $i++) {
        $sum += max($i, $count - $i, 2.5) + min($count - $i, $i) + min($i, 3, $count);
        $sum += max($count - $i, $i * 2, $i + 1, 0.5);
    }
    $end = memory_get_allocations();
    echo "allocations=", $end - $start, " checksum=", $sum, "\n";
}
hot(999 + $argc);
"#;
    assert_eq!(run(source, true), "allocations=0 checksum=2169161\n");
    assert!(run(source, false).ends_with(" checksum=2169161\n"));
}

/// Every target selects three proven numbers through the helper without cloning the winner,
/// and decides an unordered two-argument `max()` inline with the representability test.
#[test]
fn test_min_max_comparison_modes_emit_on_every_target() {
    let source = r#"<?php
function three(int $count): void {
    $sum = 0.0;
    for ($i = 0; $i < $count; $i++) { $sum += max($i, $count - $i, 2.5); }
    echo $sum;
}
function two(int $i, float $f): void { echo max($i, $f); }
three($argc);
two($argc, 2.5);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let three = function_assembly(source, name, "three");
        assert!(three.contains("__rt_mixed_min_max_select"), "{name}: {three}");
        assert!(!three.contains("__rt_mixed_clone"), "{name}: {three}");
        let two = function_assembly(source, name, "two");
        assert!(!two.contains("__rt_mixed_min_max_select"), "{name}: {two}");
        assert!(two.contains("fcvtzs") || two.contains("cvttsd2si"), "{name}: {two}");
    }
}
