//! Purpose:
//! Regression tests for issue #502: `array_fill()`'s `$start` and `$count` when they arrive as a
//! boxed `Mixed` rather than a plain `int`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The box comes from `ichecked_mul` / `ichecked_add`, whose result is `Mixed` because an
//!   integer product may overflow to float. `$n = $this->w * $this->h` is therefore boxed while
//!   a literal, a parameter, or the same code in a free function stays `Int` — which is what made
//!   the defect look method-only. The fixtures below keep the property reads for that reason.
//! - Loading a box straight into an ABI register passed the box POINTER as the argument, so the
//!   reported symptom was `array_fill(): Argument #2 ($count) is too large` (an exhausted heap
//!   before the count guard existed) and, for `$start`, an array keyed from the pointer value.
//! - Every expectation is the host PHP 8.5.10 output for the same fixture.

use crate::support::compile_and_run;

/// Issue #502's own repro: a count computed from property reads, inside a method.
///
/// The three controls it lists are kept in the same program — a foldable literal count, a count
/// passed in as a parameter, and a count assembled from locals — because only the boxed ones
/// failed and the contrast is the evidence.
#[test]
fn test_array_fill_count_from_property_arithmetic_in_a_method() {
    let out = compile_and_run(
        r#"<?php
class E {
    public array $a1 = [0];
    public array $a2 = [0];
    public array $a3 = [0];
    public int $w = 40;
    public int $h = 22;
    public function v1(): void { $n = 880; $this->a1 = array_fill(0, $n, 0); }
    public function v2(): void { $n = $this->w * $this->h; $this->a2 = array_fill(0, $n, 0); }
    public function v3(): void {
        $w = $this->w;
        $h = $this->h;
        $n = $w * $h;
        $this->a3 = array_fill(0, $n, 0);
    }
    public function byParam(int $n): void { $this->a1 = array_fill(0, $n, 0); }
}
$e = new E();
$e->v1();
$e->byParam(880);
$e->v2();
$e->v3();
echo count($e->a1), "|", count($e->a2), "|", count($e->a3), "\n";
"#,
    );
    assert_eq!(out, "880|880|880\n");
}

/// A boxed `$start` has the same defect, and a different symptom: the array came back keyed from
/// the box POINTER (`4336290584,4336290585,…`) instead of from the value.
///
/// A non-zero `$start` also selects the keyed fill helper, so this covers the second of the two
/// call paths the fix had to touch.
#[test]
fn test_array_fill_start_from_property_arithmetic_in_a_method() {
    let out = compile_and_run(
        r#"<?php
class E {
    public int $w = 4;
    public int $h = 3;
    public function both(): string {
        $s = $this->w * $this->h;
        $n = $this->w + $this->h;
        return implode(",", array_keys(array_fill($s, $n, 1)));
    }
}
echo (new E())->both(), "\n";
"#,
    );
    assert_eq!(out, "12,13,14,15,16,17,18\n");
}

/// The value-type paths a boxed count reaches: the `(count, ptr, len)` string helper, which keeps
/// the count in the FIRST argument register rather than the second, plus float and keyed fills.
///
/// The guard cases matter too — the count reaching the bounds check must be the unboxed integer,
/// or a negative count would sail past a check applied to a pointer.
#[test]
fn test_array_fill_boxed_count_across_value_types_and_guards() {
    let out = compile_and_run(
        r#"<?php
class E {
    public int $w = 4;
    public int $h = 3;

    public function strval(): string { $n = $this->w * $this->h; return implode("", array_fill(0, $n, "ab")); }
    public function floatval(): float { $n = $this->w * $this->h; $a = array_fill(0, $n, 1.5); return $a[0] + $a[count($a) - 1]; }
    public function keyed(): string { $n = $this->w * $this->h; return implode(",", array_keys(array_fill(5, $n, 1))); }
    public function zero(): int { $n = $this->w * $this->h - 12; return count(array_fill(0, $n, 1)); }
    public function negative(): string {
        $n = $this->w * $this->h - 100;
        try { array_fill(0, $n, 0); return "no throw"; } catch (ValueError $e) { return $e->getMessage(); }
    }
}
$e = new E();
echo $e->strval(), "\n";
echo $e->floatval(), "\n";
echo $e->keyed(), "\n";
echo $e->zero(), "\n";
echo $e->negative(), "\n";
"#,
    );
    assert_eq!(
        out,
        "abababababababababababab\n3\n5,6,7,8,9,10,11,12,13,14,15,16\n0\n\
         array_fill(): Argument #2 ($count) must be greater than or equal to 0\n"
    );
}

/// Raised in review: an integer read out of an array carries the TAGGED-NULL representation
/// under the default sentinel scheme, so `array_fill()` sees `TaggedScalar`, not `Int`.
///
/// It needs no staging, and this pins why: a tagged scalar keeps its integer payload in the
/// value's own slot and the null tag in the adjacent one, so loading the slot into an ABI
/// register already yields the integer. Only `Mixed`/`Union` has to be unboxed.
///
/// Both argument positions and every value-type path are covered, because each reaches a
/// different fill helper. Measured against the host PHP 8.5.10.
#[test]
fn test_array_fill_accepts_tagged_scalar_start_and_count() {
    let out = compile_and_run(
        r#"<?php
function maybe(bool $b): ?int { return $b ? 4 : null; }

$counts = [3, 5, 7];
$starts = [0, 2, 4];
var_dump(count(array_fill(0, $counts[0], 9)));
var_dump(count(array_fill(0, $counts[1], "s")));
var_dump(implode(",", array_keys(array_fill($starts[1], $counts[0], 1))));
var_dump(count(array_fill($starts[0], $counts[2], 1.5)));

$a = maybe(true);
var_dump(count(array_fill(0, $a ?? 1, 1)));
$b = maybe(true);
if ($b !== null) {
    var_dump(count(array_fill(0, $b, 1)));
}
"#,
    );
    assert_eq!(
        out,
        "int(3)\nint(5)\nstring(5) \"2,3,4\"\nint(7)\nint(4)\nint(4)\n"
    );
}
