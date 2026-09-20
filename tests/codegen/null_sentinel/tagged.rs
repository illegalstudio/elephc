//! Purpose:
//! End-to-end coverage of the tagged null representation (`NullRepr::Tagged`): the §0
//! collision repros plus the tag-aware consumer surface (echo, var_dump, is_null, ??, ??=,
//! isset, strict comparison, arithmetic, string coercion) over null-capable int reads.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every fixture is compiled with `compile_and_run_tagged` (forces `NullRepr::Tagged`);
//!   expected outputs are PHP 8.4 cross-checked. The legacy sentinel default is covered by
//!   the rest of the suite and must keep passing unchanged.

use super::*;

/// The integer PHP_INT_MAX-1 (== the legacy null sentinel bit pattern) must echo as itself
/// under the tagged representation.
#[test]
fn test_tagged_int_at_sentinel_echoes_as_integer() {
    let out = compile_and_run_tagged("<?php echo 9223372036854775806;");
    assert_eq!(out, "9223372036854775806");
}

/// The sentinel-valued integer survives a variable round-trip through echo and var_dump.
#[test]
fn test_tagged_int_at_sentinel_var_roundtrips() {
    let out =
        compile_and_run_tagged("<?php $x = 9223372036854775806; echo $x, \"|\"; var_dump($x);");
    assert_eq!(out, "9223372036854775806|int(9223372036854775806)\n");
}

/// The sentinel-valued integer read back from an array prints and var_dumps as int(...).
#[test]
fn test_tagged_int_at_sentinel_in_array_roundtrips() {
    let out = compile_and_run_tagged(
        "<?php $a = [9223372036854775806]; echo $a[0], \"|\"; var_dump($a[0]);",
    );
    assert_eq!(out, "9223372036854775806|int(9223372036854775806)\n");
}

/// A nullable-int function returning the sentinel-valued integer var_dumps as int(...).
#[test]
fn test_tagged_int_at_sentinel_nullable_return_is_not_null() {
    let out = compile_and_run_tagged(
        "<?php function f(): ?int { return 9223372036854775806; } var_dump(f());",
    );
    assert_eq!(out, "int(9223372036854775806)\n");
}

/// is_null() rejects the sentinel-valued integer: a real int is never null.
#[test]
fn test_tagged_int_at_sentinel_is_not_null() {
    let out = compile_and_run_tagged("<?php $x = 9223372036854775806; var_dump(is_null($x));");
    assert_eq!(out, "bool(false)\n");
}

/// ?? keeps the sentinel-valued integer instead of falling back to the default.
#[test]
fn test_tagged_int_at_sentinel_null_coalesce_keeps_value() {
    let out = compile_and_run_tagged("<?php $x = 9223372036854775806; echo $x ?? 0;");
    assert_eq!(out, "9223372036854775806");
}

/// isset() reports true for an array element holding the sentinel-valued integer.
#[test]
fn test_tagged_int_at_sentinel_isset_is_true() {
    let out = compile_and_run_tagged(
        "<?php $a = [9223372036854775806]; echo isset($a[0]) ? \"set\" : \"unset\";",
    );
    assert_eq!(out, "set");
}

/// An out-of-bounds int-array read var_dumps as NULL, not as the sentinel integer.
#[test]
fn test_tagged_array_miss_var_dumps_null() {
    let out = compile_and_run_tagged("<?php $a = [1, 2]; var_dump($a[5]);");
    assert_eq!(out, "NULL\n");
}

/// ?? falls back to the default for an out-of-bounds int-array read.
#[test]
fn test_tagged_array_miss_null_coalesce_falls_back() {
    let out = compile_and_run_tagged("<?php $a = [1, 2]; echo $a[5] ?? -1;");
    assert_eq!(out, "-1");
}

/// A dynamic-index miss behaves like a static one (read through a variable index).
#[test]
fn test_tagged_array_miss_dynamic_index_is_null() {
    let out = compile_and_run_tagged("<?php $a = [1, 2]; $i = 9; var_dump($a[$i]);");
    assert_eq!(out, "NULL\n");
}

/// An associative-array miss on an int-valued map yields null for var_dump and ??.
#[test]
fn test_tagged_assoc_miss_is_null() {
    let out = compile_and_run_tagged(
        "<?php $m = [\"x\" => 1]; var_dump($m[\"zz\"]); echo $m[\"zz\"] ?? 9;",
    );
    assert_eq!(
        out,
        "NULL\n9"
    );
}

/// Strict comparison sees an array miss as identical to null and a hit as not null.
#[test]
fn test_tagged_array_miss_strict_equals_null() {
    let out = compile_and_run_tagged(
        "<?php $a = [1]; var_dump($a[5] === null); var_dump($a[0] === null); var_dump($a[5] !== null);",
    );
    assert_eq!(
        out,
        "bool(true)\nbool(false)\nbool(false)\n"
    );
}

/// Arithmetic coerces a null array read to zero on either operand side (PHP null + 1 == 1).
#[test]
fn test_tagged_array_miss_arithmetic_coerces_to_zero() {
    let out = compile_and_run_tagged(
        "<?php $a = [10]; echo $a[5] + 1, \"|\", 1 + $a[5], \"|\", $a[5] * 3;",
    );
    assert_eq!(
        out,
        "1|1|0"
    );
}

/// String contexts render a null array read as an empty string, not the sentinel digits.
#[test]
fn test_tagged_array_miss_string_contexts_are_empty() {
    let out = compile_and_run_tagged(
        "<?php $a = [10]; echo $a[5] . \"end\", \"|\", \"x{$a[5]}y\";",
    );
    assert_eq!(
        out,
        "end|xy"
    );
}

/// Truthiness of a null array read is false; an in-bounds non-zero read is truthy.
#[test]
fn test_tagged_array_miss_truthiness() {
    let out = compile_and_run_tagged(
        "<?php $a = [10]; echo $a[5] ? \"t\" : \"f\"; echo $a[0] ? \"T\" : \"F\";",
    );
    assert_eq!(out, "fT");
}

/// ??= stores the fallback when the current value is a null array read.
#[test]
fn test_tagged_null_coalesce_assign_on_null_local() {
    let out = compile_and_run_tagged("<?php $w = null; $w ??= 3; echo $w;");
    assert_eq!(out, "3");
}

/// In-bounds reads, sums, and casts keep exact int semantics across the full range.
#[test]
fn test_tagged_array_reads_exact_int_semantics() {
    let out = compile_and_run_tagged(
        "<?php $a = [10, 20, 30]; $s = 0; for ($i = 0; $i < 3; $i++) { $s = $s + $a[$i]; } echo $s, \"|\", intdiv($a[1], $a[0]), \"|\", (int)$a[1];",
    );
    assert_eq!(out, "60|2|20");
}

/// gettype() distinguishes a null miss from an integer hit at runtime.
#[test]
fn test_tagged_gettype_dispatches_on_runtime_tag() {
    let out = compile_and_run_tagged(
        "<?php $a = [10]; echo gettype($a[0]), \"|\", gettype($a[5]);",
    );
    assert_eq!(out, "integer|NULL");
}

/// empty() treats a null miss and a zero hit as empty, and a non-zero hit as non-empty.
#[test]
fn test_tagged_empty_on_array_reads() {
    let out = compile_and_run_tagged(
        "<?php $a = [0, 7]; var_dump(empty($a[5])); var_dump(empty($a[0])); var_dump(empty($a[1]));",
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(false)\n"
    );
}


/// array_pop on an empty int array yields a real tagged null (NULL, is_null true, ?? falls
/// back) instead of the sentinel integer.
#[test]
fn test_tagged_array_pop_empty_is_null() {
    let out = compile_and_run_tagged(
        "<?php $e = [1]; array_pop($e); var_dump(array_pop($e)); var_dump(is_null(array_pop($e))); echo array_pop($e) ?? -5;",
    );
    assert_eq!(out, "NULL\nbool(true)\n-5");
}

/// array_shift on an empty int array yields a real tagged null.
#[test]
fn test_tagged_array_shift_empty_is_null() {
    let out = compile_and_run_tagged(
        "<?php $s = [2]; array_shift($s); var_dump(array_shift($s));",
    );
    assert_eq!(out, "NULL\n");
}

/// array_pop and array_shift still return real values from non-empty int arrays.
#[test]
fn test_tagged_array_pop_shift_values_roundtrip() {
    let out = compile_and_run_tagged(
        "<?php $a = [10, 20, 30]; echo array_pop($a), \"|\", array_shift($a), \"|\"; var_dump(array_pop($a));",
    );
    assert_eq!(out, "30|10|int(20)\n");
}

/// Unary minus and abs() coerce a null array read to zero (PHP -null == 0, abs(null) == 0).
#[test]
fn test_tagged_unary_minus_and_abs_narrow_null() {
    let out = compile_and_run_tagged(
        "<?php $a = [3]; echo -$a[9], \"|\", abs($a[9]), \"|\", -$a[0], \"|\", abs(-$a[0]);",
    );
    assert_eq!(out, "0|0|-3|3");
}


/// Phase 3 narrowing proof: a plain non-nullable int emits no in-band sentinel material
/// under the tagged representation — neither the 0x7fff_ffff_ffff_fffe immediate nor its
/// decimal form appears anywhere in the user assembly for echo + var_dump of an int local.
#[test]
fn test_tagged_plain_int_emits_no_sentinel_check() {
    let dir = make_cli_test_dir("elephc_tagged_plain_int_asm");
    let (user_asm, _runtime_asm, _libs) = compile_source_to_asm_with_defines_repr(
        "<?php $x = 5; echo $x; var_dump($x);",
        &dir,
        &std::collections::HashSet::new(),
        8_388_608,
        false,
        false,
        elephc::codegen::NullRepr::Tagged,
    );
    let main_start = user_asm
        .find("_main:")
        .or_else(|| user_asm.find("\nmain:").map(|i| i + 1))
        .expect("user asm contains the main label");
    let main_body = &user_asm[main_start..];
    let main_end = main_body.find("ret").map(|i| i + main_start).unwrap_or(user_asm.len());
    let main_section = &user_asm[main_start..main_end];
    let lower = main_section.to_lowercase();
    assert!(
        !lower.contains("0xfffe") && !main_section.contains("9223372036854775806"),
        "plain-int echo/var_dump must not materialize the null sentinel under Tagged:\n{}",
        main_section
    );
}

/// Control for the narrowing proof: the same program under the sentinel representation
/// does materialize the in-band sentinel for its echo/var_dump null checks.
#[test]
fn test_sentinel_plain_int_still_emits_sentinel_check() {
    let dir = make_cli_test_dir("elephc_sentinel_plain_int_asm");
    let (user_asm, _runtime_asm, _libs) = compile_source_to_asm_with_defines_repr(
        "<?php $x = 5; echo $x; var_dump($x);",
        &dir,
        &std::collections::HashSet::new(),
        8_388_608,
        false,
        false,
        elephc::codegen::NullRepr::Sentinel,
    );
    let main_start = user_asm
        .find("_main:")
        .or_else(|| user_asm.find("\nmain:").map(|i| i + 1))
        .expect("user asm contains the main label");
    let main_body = &user_asm[main_start..];
    let main_end = main_body.find("ret").map(|i| i + main_start).unwrap_or(user_asm.len());
    let main_section = &user_asm[main_start..main_end];
    let lower = main_section.to_lowercase();
    assert!(
        lower.contains("0xfffe") || main_section.contains("9223372036854775806"),
        "sentinel-mode echo/var_dump should still materialize the null sentinel:\n{}",
        main_section
    );
}

/// The legacy in-band behavior is preserved under the explicit sentinel opt-out:
/// the same fixture still misreads PHP_INT_MAX-1 as null when --null-repr=sentinel.
#[test]
fn test_sentinel_optout_still_suppresses_collision_value() {
    let out = compile_and_run_sentinel("<?php echo 9223372036854775806;");
    assert_eq!(out, "");
}

/// Verifies `json_encode()` of a tagged nullable int encodes its VALUE, not `null`.
///
/// `emit_json_encode_loaded_value` dispatches on `codegen_repr()`, and `int|null` is the one
/// union that does not map to `Mixed`: under the default tagged representation it stays an
/// unboxed two-word `{payload, tag}` pair. It therefore fell to the `_` arm, which calls
/// `__rt_json_encode_null` — so every value encoded as `null`, silently (#1121).
///
/// Both polarities are covered, and `0` with them: a tag test that read the payload instead of
/// the tag would answer `null` for zero.
#[test]
fn test_json_encode_of_a_tagged_nullable_int_encodes_its_value() {
    let out = compile_and_run(
        r#"<?php
function nint(int $i): ?int { return $i >= 0 ? $i : null; }
echo json_encode(nint(5)), "|";
echo json_encode(nint(0)), "|";
echo json_encode(nint(-1)), "|";
echo json_encode(nint(-1) ?? -7);
"#,
    );

    assert_eq!(out, "5|0|null|-7");
}

/// Verifies the encoder does not consume the tagged value it reads.
///
/// The branch reads the payload in place rather than boxing, so the same local must encode twice
/// and still print as an int afterwards.
#[test]
fn test_json_encode_does_not_consume_the_tagged_value() {
    let out = compile_and_run(
        r#"<?php
function nint(int $i): ?int { return $i >= 0 ? $i : null; }
$n = nint(42);
echo json_encode($n), "|", json_encode($n), "|", $n;
"#,
    );

    assert_eq!(out, "42|42|42");
/// A tagged nullable int reaching a `mixed` PARAMETER must be boxed by the call ABI.
///
/// It was not. `emit_box_current_value_as_mixed` matched on the DECLARED type, where
/// `PhpType::Union(_)` means "already a boxed Mixed" — true of every nullable union except this
/// one, which is the unboxed two-word `{payload, tag}` pair. Nothing was emitted, the callee read
/// the raw payload as a Mixed pointer, and the caller's own `__rt_decref_mixed` ran on the
/// integer: `var_export()` on a `?int` segfaulted whether it held `5` or `null` (#1040).
#[test]
fn test_tagged_nullable_int_boxes_for_a_mixed_parameter() {
    let out = compile_and_run_tagged(
        r#"<?php
class N { public ?int $n = 5; }
class M { public ?int $n = null; }

function nint(int $i): ?int { return $i >= 0 ? 5 : null; }
function take(mixed $v): string { return is_null($v) ? "null" : "set:" . $v; }

echo take(nint(1)), "|", take(nint(-1)), "\n";
echo var_export((new N())->n, true), "|", var_export((new M())->n, true), "\n";
echo var_export(nint(1), true), "\n";
"#,
    );
    assert_eq!(out, "set:5|null\n5|NULL\n5\n");
}

/// The same value reaching a `mixed` parameter of a METHOD, and through a method's return.
#[test]
fn test_tagged_nullable_int_boxes_for_a_mixed_method_parameter() {
    let out = compile_and_run_tagged(
        r#"<?php
class Box {
    public ?int $n = 5;
    public function get(): ?int { return $this->n; }
    public function take(mixed $v): string { return is_null($v) ? "null" : "set:" . $v; }
}

$b = new Box();
echo $b->take($b->get()), "|", $b->take($b->n), "\n";
echo var_export($b->get(), true), "\n";
"#,
    );
    assert_eq!(out, "set:5|set:5\n5\n");
}

/// A static property and a `int|null` spelled without `?` take the same path.
#[test]
fn test_tagged_nullable_int_boxes_from_every_storage() {
    let out = compile_and_run_tagged(
        r#"<?php
class S { public static ?int $n = 7; }
class U { public int|null $n = 9; }

function take(mixed $v): string { return is_null($v) ? "null" : "set:" . $v; }

echo take(S::$n), "|", take((new U())->n), "\n";
echo var_export(S::$n, true), "|", var_export((new U())->n, true), "\n";
"#,
    );
    assert_eq!(out, "set:7|set:9\n7|9\n");
}

/// The other nullable scalars were never broken — they are already boxed Mixed — and must stay
/// that way, which is what makes `int|null` the single exception worth the arm.
#[test]
fn test_other_nullable_scalars_still_reach_a_mixed_parameter() {
    let out = compile_and_run_tagged(
        r#"<?php
function nstr(int $i): ?string { return $i >= 0 ? "x" : null; }
function nflt(int $i): ?float { return $i >= 0 ? 1.5 : null; }
function nbool(int $i): ?bool { return $i >= 0 ? true : null; }

echo var_export(nstr(1), true), "|", var_export(nflt(1), true), "|", var_export(nbool(1), true), "\n";
"#,
    );
    assert_eq!(out, "'x'|1.5|true\n");
}

/// A builtin whose result type its own check hook derives from the ARGUMENT must not be disturbed.
///
/// An earlier cut boxed at the EIR argument boundary instead of in the ABI. `abs($n)` is typed
/// `int` because the checker saw `int|null`; handing the runtime a boxed Mixed made it return a
/// boxed Mixed, which the caller then read as a raw integer — `abs(5)` answered `4330504864`.
/// Fixing the ABI rather than the argument leaves every such hook's answer intact.
#[test]
fn test_tagged_nullable_int_still_reaches_builtins_unboxed() {
    let out = compile_and_run_tagged(
        r#"<?php
function nint(int $i): ?int { return $i >= 0 ? 5 : null; }

$n = nint(1);
echo abs($n), "|", intdiv($n, 1), "|", max($n, 1), "|", sprintf("%d", $n), "|", strval($n), "\n";
echo gettype($n), "|", is_int($n) ? "y" : "n", "|", $n, "\n";
"#,
    );
    assert_eq!(out, "5|5|5|5|5\ninteger|y|5\n");
}

/// The other ways a value reaches a `mixed` parameter: a variadic, a closure, a first-class
/// callable, `call_user_func`, and a `mixed` property write. All were already correct — the call
/// ABI is shared — and this pins them so the shared fix cannot regress one of them quietly.
#[test]
fn test_tagged_nullable_int_reaches_every_mixed_call_shape() {
    let out = compile_and_run_tagged(
        r#"<?php
function nint(int $i): ?int { return $i >= 0 ? 5 : null; }
function variadic(mixed ...$vs): string { return implode(",", array_map(fn($x) => var_export($x, true), $vs)); }
function plain(mixed $v): string { return var_export($v, true); }

class Holder { public mixed $slot = null; }

$n = nint(1);
$c = function (mixed $v): string { return var_export($v, true); };
$f = plain(...);
$h = new Holder();
$h->slot = $n;

echo "variadic:", variadic($n, $n), "\n";
echo "closure:", $c($n), "\n";
echo "fcc:", $f($n), "\n";
echo "cuf:", call_user_func('plain', $n), "\n";
echo "prop:", var_export($h->slot, true), "\n";
"#,
    );
    assert_eq!(
        out,
        "variadic:5,5\nclosure:5\nfcc:5\ncuf:5\nprop:5\n"
    );
}

/// A generator taking a `?int` keeps it across the frame, including when a second parameter
/// follows it — the two-word pair must not eat the next parameter's register.
#[test]
fn test_tagged_nullable_int_survives_a_generator_parameter() {
    let out = compile_and_run_tagged(
        r#"<?php
function gen(?int $n) { yield $n; yield 7; }
function gen2(?int $n, string $tail) { yield $n; yield $tail; }

foreach (gen(5) as $v) { echo var_export($v, true), "|"; }
echo "\n";
foreach (gen2(5, "x") as $v) { echo var_export($v, true), "|"; }
echo "\n";
"#,
    );
    assert_eq!(out, "5|7|\n5|'x'|\n");
}
