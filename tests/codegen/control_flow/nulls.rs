//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow nulls, including ternary null is falsy.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Tests that `null` in a ternary condition is treated as falsy, matching PHP semantics.
/// Verifies that `$x = null; echo $x ? "yes" : "no";` outputs `"no"`.
#[test]
fn test_ternary_null_is_falsy() {
    let out = compile_and_run("<?php $x = null; echo $x ? \"yes\" : \"no\";");
    assert_eq!(out, "no");
}

/// Regression test for issue #549 (`??` sibling): a nullable array value side
/// (runtime boxed-Mixed elements from the `?array` return contract) merged
/// with an array<string> default must produce an array-of-Mixed temp. Before
/// the fix the default's element type won and the value side's boxed cells
/// were read back as string descriptors.
#[test]
fn test_null_coalesce_nullable_array_value_widens_string_default_merge() {
    let out = compile_and_run(
        r#"<?php
function f(int $n): ?array {
    if ($n === 1) {
        return [1, 2];
    }
    return null;
}
$r = f($argc) ?? ["a", "b"];
echo $r[0], "\n", $r[1], "\n";
"#,
    );
    assert_eq!(out, "1\n2\n");
}

/// End-to-end companion for issue #587: `??` keeps the heterogeneous result
/// array-shaped in the checker while #549 provides boxed-Mixed merge storage,
/// allowing array builtins and spread to consume the selected branch.
#[test]
fn test_null_coalesce_heterogeneous_array_result_supports_array_operations() {
    let out = compile_and_run(
        r#"<?php
function maybe(int $n) {
    return $n === 1 ? [1, 2] : null;
}
$r = maybe($argc) ?? ["a", "b"];
$spread = [...$r];
echo array_sum($r), "|", in_array(2, $r), "|", count($spread);
"#,
    );
    assert_eq!(out, "3|1|2");
}

/// Regression test for issue #549 (borrowed-cell review follow-up): a
/// borrowed `?array` parameter cell flowing through `??` into a widened
/// merge shares its payload with the caller's live array, so the widening
/// conversion must copy-on-write-split instead of rewriting the caller's
/// slots in place.
#[test]
fn test_null_coalesce_borrowed_param_cell_preserves_caller_array() {
    let out = compile_and_run(
        r#"<?php
function g(?array $a): int {
    $r = $a ?? ["x", "y"];
    return count($r);
}
$src = [1, 2];
$n = g($src);
$m = g($src);
echo $n, $m, "|", $src[0], "|", $src[1], "\n";
"#,
    );
    assert_eq!(out, "22|1|2\n");
}

/// Regression test for issue #549 (borrowed-cell review follow-up): repeated
/// borrowed-cell widenings against the same live caller array must keep the
/// heap balanced — the conversion owns exactly the payload reference the
/// unbox coercion retained.
#[test]
fn test_null_coalesce_borrowed_param_cell_loop_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function g(?array $a): int {
    $r = $a ?? ["x", "y"];
    return count($r);
}
$c = 0;
$src = [1, 2];
for ($i = 0; $i < 20; $i++) {
    if ($i % 2 == 0) {
        $c = $c + g($src);
    } else {
        $c = $c + g(null);
    }
}
echo $c, "|", $src[0];
"#,
    );
    assert_eq!(out.stdout, "40|1");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap summary, got: {}",
        out.stderr
    );
}

/// Regression test for issue #549 (borrowed-cell review follow-up): the
/// associative sibling of the borrowed `?array` parameter merge must also
/// preserve the caller's live hash across the widening conversion.
#[test]
fn test_null_coalesce_borrowed_assoc_param_cell_preserves_caller_hash() {
    let out = compile_and_run(
        r#"<?php
function g(?array $a): int {
    $r = $a ?? ["k" => "x"];
    return count($r);
}
$src = ["k" => 1, "n" => 2];
$n = g($src);
$m = g($src);
echo $n, $m, "|", $src["k"], "|", $src["n"], "\n";
"#,
    );
    assert_eq!(out, "22|1|2\n");
}

/// Regression test for issue #549 (borrowed-cell review follow-up): a local
/// `?array` cell read by two `??` merges must survive the first widening
/// conversion with payload and cell intact.
#[test]
fn test_null_coalesce_local_nullable_cell_survives_two_merges() {
    let out = compile_and_run(
        r#"<?php
function f(int $n): ?array {
    if ($n === 1) {
        return [1, 2];
    }
    return null;
}
$x = f($argc);
$r = $x ?? ["a", "b"];
$s = $x ?? ["c", "d"];
echo $r[0], $r[1], $s[0], $s[1], "\n";
"#,
    );
    assert_eq!(out, "1212\n");
}

/// Regression test for issue #554: `??` must select its default when a missing
/// indexed-array read has a statically-known `Str` result, while a present empty
/// string keeps `''`. Indices are bound to `$argc`-derived locals so the element
/// read stays statically `Str` (an inline arithmetic index like `$a[$argc + 6]`
/// is typed `Mixed` and would route through the boxed-Mixed null path instead),
/// forcing the miss through the `IsNull`(`Str`) branch decision: a missed string
/// slot carries the null-string pointer sentinel, a real empty string does not.
/// Covers both the first-index miss (missed outer container) and the second-index
/// miss (missed inner string slot).
#[test]
fn test_null_coalesce_string_array_miss_selects_default() {
    let out = compile_and_run(
        r#"<?php
$a = [['', 'word0'], ['', 'word1']];
$miss_i = $argc + 6;
$hit_i = $argc;
$miss_j = $argc + 6;
$hit_j = $argc - 1;
echo '[' . ($a[$miss_i][$hit_j] ?? 'dflt') . "]\n";
echo '[' . ($a[$hit_i][$miss_j] ?? 'dflt') . "]\n";
echo '[' . ($a[$hit_i][$hit_j] ?? 'dflt') . "]\n";
"#,
    );
    assert_eq!(out, "[dflt]\n[dflt]\n[]\n");
}

/// Regression test for issue #554 using the issue's verbatim literal-index
/// reproduction: `$a[7][1] ?? 'dflt'` and `$a[1][7] ?? 'dflt'` select the
/// default, and the present empty string `$a[1][0]` stays `''`, matching PHP's
/// `[dflt]` / `[dflt]` / `[]` output exactly.
#[test]
fn test_null_coalesce_string_array_miss_literal_repro() {
    let out = compile_and_run(
        r#"<?php
$a = [['', 'word0'], ['', 'word1']];
echo '[' . ($a[7][1] ?? 'dflt') . "]\n";
echo '[' . ($a[1][7] ?? 'dflt') . "]\n";
echo '[' . ($a[1][0] ?? 'dflt') . "]\n";
"#,
    );
    assert_eq!(out, "[dflt]\n[dflt]\n[]\n");
}

/// Regression test for issue #554 (string-keyed associative sibling): a missing
/// string-keyed read of a statically-`Str`-valued hash selects the `??` default,
/// a present key keeps its value, and a present empty string stays `''`. The
/// runtime-unknown key defeats folding so the miss travels the hash-get null
/// fallback into the `IsNull`(`Str`) branch decision.
#[test]
fn test_null_coalesce_assoc_string_miss_selects_default() {
    let out = compile_and_run(
        r#"<?php
$m = ['a' => 'x', 'b' => 'y'];
$miss = $argc > 100 ? 'a' : 'zzz';
echo '[' . ($m[$miss] ?? 'dflt') . "]\n";
$hit = $argc > 100 ? 'zzz' : 'a';
echo '[' . ($m[$hit] ?? 'dflt') . "]\n";
$e = ['k' => ''];
echo '[' . ($e['k'] ?? 'dflt') . "]\n";
"#,
    );
    assert_eq!(out, "[dflt]\n[x]\n[]\n");
}

/// Regression test for issue #554 (nullable-string local/return sibling): a
/// `?string` return that is null at runtime selects the `??` default, a non-null
/// return keeps its value, and a present empty-string local stays `''`. Confirms
/// the same null-string representation used for array-element misses drives the
/// coalesce decision for scalar nullable-string storage too.
#[test]
fn test_null_coalesce_nullable_string_local_selects_default() {
    let out = compile_and_run(
        r#"<?php
function f(int $n): ?string { return $n > 100 ? 'hit' : null; }
$s = f($argc);
echo '[' . ($s ?? 'dflt') . "]\n";
$t = f($argc + 1000);
echo '[' . ($t ?? 'dflt') . "]\n";
$u = '';
echo '[' . ($u ?? 'dflt') . "]\n";
"#,
    );
    assert_eq!(out, "[dflt]\n[hit]\n[]\n");
}

/// Regression test for issue #554: the `??` string-miss decision keeps the heap
/// balanced across a loop that mixes a missing read (default 'dflt') and a
/// present empty-string control (kept `''`). The missed read materializes a
/// non-owned null-string sentinel and the default is a persistent literal, so no
/// per-iteration allocation should leak.
#[test]
fn test_null_coalesce_string_array_miss_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [['', 'word0'], ['', 'word1']];
$hit_i = $argc;
$miss_j = $argc + 6;
$hit_j = $argc - 1;
$c = 0;
for ($i = 0; $i < 20; $i++) {
    $miss = ($a[$hit_i][$miss_j] ?? 'dflt');
    $present = ($a[$hit_i][$hit_j] ?? 'dflt');
    $c = $c + strlen($miss) + strlen($present);
}
echo $c;
"#,
    );
    assert_eq!(out.stdout, "80");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap summary, got: {}",
        out.stderr
    );
}

/// Regression test for issue #554: the `??` string-miss path must stay silent,
/// matching PHP. A bare missed read warns "Undefined array key", but wrapping it
/// in `??` suppresses the warning (issue #533's warning behavior on the coalesce
/// path). Asserts the default is selected with no diagnostics on stderr.
#[test]
fn test_null_coalesce_string_array_miss_is_silent() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [['', 'word0'], ['', 'word1']];
$hit_i = $argc;
$miss_j = $argc + 6;
echo ($a[$hit_i][$miss_j] ?? 'dflt');
"#,
    );
    assert_eq!(out.stdout, "dflt");
    assert!(
        !out.stderr.contains("Undefined") && !out.stderr.contains("Warning"),
        "expected silent coalesce path, got stderr: {}",
        out.stderr
    );
    assert!(out.success, "program should exit successfully");
}

/// A nullable TYPED array flowing through `??` must unbox, not be stored as a cell.
///
/// `?array<int>` is a boxed nullable cell, and the `??` merge temp is a raw `array<int>`. The
/// coercion that fills that temp only ever ran for a `mixed`-payload target, because before
/// `array<T>` existed a container merge target could not be typed — so a typed one took an early
/// return and stored the CELL into the raw slot. `g([1, 2, 3])` then printed `0` and dumped four
/// zeros where php prints `1` and `[1, 2, 3]`.
///
/// A ternary was correct, and a bare `?array` was correct: only the typed element type through
/// `??` was wrong, which is why the suite never caught it. Found by an external review that ran
/// it.
#[test]
fn test_nullable_typed_array_through_null_coalesce_unboxes() {
    let out = compile_and_run(
        "<?php \
         function g(?array<int> $a): string { \
           $r = $a ?? [4, 5]; \
           return $r[0] . '/' . count($r); \
         } \
         echo g([1, 2, 3]), '|', g(null);",
    );
    assert_eq!(out, "1/3|4/2");
}

/// The same for string elements, and from a CALL RESULT rather than a parameter.
#[test]
fn test_nullable_typed_array_from_a_call_through_null_coalesce() {
    let out = compile_and_run(
        "<?php \
         function src(bool $empty): ?array<string> { return $empty ? null : ['a', 'b']; } \
         function pick(bool $empty): string { $r = src($empty) ?? ['z']; return $r[0]; } \
         echo pick(false), pick(true);",
    );
    assert_eq!(out, "az");
}

/// The caller's array stays intact and the ledger stays balanced.
///
/// The unbox takes its own reference to the payload rather than consuming the cell's, so the
/// live array a caller still holds must not be rewritten in place — and 200 round trips must
/// free everything they allocate.
#[test]
fn test_nullable_typed_array_merge_preserves_the_caller_array() {
    let out = compile_and_run_with_heap_debug(
        "<?php \
         function g(?array<int> $a): int { $r = $a ?? [9]; return $r[1]; } \
         $src = [1, 2, 3]; \
         $t = 0; \
         for ($i = 0; $i < 200; $i++) { $t = $t + g($src); } \
         echo $t, '|', $src[1], '|', count($src);",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "400|2|3");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "the unbox takes its own payload reference, so nothing may be left live: {}",
        out.stderr
    );
}

/// Coalescing a nullable object with `new` (`$o ??= new Box()`, `$o = $o ?? new Box()`) leaves
/// the object itself in `$o`, not the boxed cell that held `?Box`: every member access after it
/// read the cell as the object and crashed or answered garbage (#1628). The unbox is taken by a
/// `?Box` parameter both null and not, a nullable call result and a nullable property (`a`-`d`),
/// and by a nullable callable coalesced with a closure (`h`). Object (`g`) and callable (`j`)
/// null defaults preserve both the null input and the original non-null value.
/// A local merged from a ternary (`e`) and `?:` (`f`) keep a `Mixed` temp and pin the unchanged
/// neighbouring paths. Runs over a loop under `--heap-debug`.
#[test]
fn test_null_coalesce_nullable_object_with_new_keeps_the_object() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box {
    public array $list = [1, 2, 3];
    public int $n = 7;
    public ?Box $next = null;
    public function items(): array { return $this->list; }
}
function find(int $n): ?Box { return $n > 0 ? new Box() : null; }
function a(?Box $o): int { $o ??= new Box(); return $o->n + count($o->items()); }
function b(?Box $o): int { $o = $o ?? new Box(); return $o->list[2]; }
function c(int $n): int { $o = find($n) ?? new Box(); return $o->n; }
function d(Box $h): int { $x = $h->next ?? new Box(); return $x->n; }
function e(?Box $o, int $n): int { $x = $n > 0 ? $o : new Box(); $x ??= new Box(); return $x->n; }
function f(?Box $o): int { $x = $o ?: new Box(); return $x->n; }
function g(?Box $o): string { $x = $o ?? null; return $x === null ? "null" : (string) $x->n; }
function h(?callable $cb): string { $cb2 = $cb ?? fn() => "dflt"; return $cb2(); }
function j(?callable $cb): string { $cb2 = $cb ?? null; return $cb2 === null ? "null" : $cb2(); }
$total = 0;
$tail = "";
for ($i = 0; $i < 20 + ($argc > 5 ? 1 : 0); $i++) {
    $total += a(null) + a(new Box()) + b(null) + b(new Box()) + c(0) + c(1) + d(new Box()) + e(null, 1) + e(new Box(), 0) + f(null) + f(new Box());
    $tail = g(null) . g(new Box()) . h(null) . h(fn() => "cb") . j(null) . j(fn() => "kept");
}
echo $total, " ", $tail, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1500 null7dfltcbnullkept\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The value of an assignment expression stays a raw PHP null; a null condition is falsy, not
/// the nonzero null sentinel (#1990). Covers `&&`, `if`, `!`, `?:`, `while`, a mixed-returning
/// closure and an assignment forwarded through another local.
#[test]
fn test_assignment_expression_null_is_falsy_in_conditions() {
    let out = compile_and_run(
        r#"<?php
function f1(mixed $x): string { return ($x = null) && true ? "T" : "F"; }
function f2(mixed $x): string { if ($x = null) { return "T"; } return "F"; }
function f3(mixed $x): string { return !($x = null) ? "T" : "F"; }
function f4(mixed $x): string { return ($x = null) ?: "D"; }
function f5(mixed $x): string { while ($x = null) { return "T"; } return "F"; }
function f6(): string { $h = function (): mixed { return null; }; return ($h()) ? "T" : "F"; }
function f7(mixed $x): string { $y = ($x = null); return $y ? "T" : "F"; }
echo f1(5), f2(5), f3(5), f4(5), f5(5), f6(), f7(5), "\n";
"#,
    );
    assert_eq!(out, "FFTDFFF\n");
}

/// A builtin whose result PHP answers as `true` stays truthy: the raw-null exclusion only
/// applies to values whose PHP type really is null, and the sort family's contract is `bool`
/// now (#1990 review). `var_dump` and `clearstatcache` answer null, so they stay falsy.
#[test]
fn test_sort_family_results_are_truthy_like_php() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1];
echo usort($a, function ($p, $q) { return $p <=> $q; }) ? "T" : "F";
$b = [3, 1];
echo sort($b) ? "T" : "F";
$c = [1, 2];
echo shuffle($c) ? "T" : "F";
echo natsort($c) ? "T" : "F";
$d = [1];
echo array_walk($d, function (&$v) { $v = $v + 1; }) ? "T" : "F";
echo var_dump(1) ? "T" : "F";
echo clearstatcache() ? "T" : "F";
echo "\n";
"#,
    );
    assert_eq!(out, "TTTTTint(1)\nFF\n");
}

/// The sort family and `array_walk` return PHP's exact `true`, not merely a truthy integer, so
/// `=== true` and `(int) ... === 1` both hold (#1990 review).
#[test]
fn test_sort_family_returns_exact_boolean_true() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1];
echo (usort($a, function ($p, $q) { return $p <=> $q; }) === true) ? "1" : "0";
$b = [3, 1];
echo (sort($b) === true) ? "1" : "0";
echo ((int) sort($b) === 1) ? "1" : "0";
$c = [1, 2];
echo (natsort($c) === true) ? "1" : "0";
$d = [1];
echo (array_walk($d, function (&$v) { $v = $v + 1; }) === true) ? "1" : "0";
$e = [1, 2, 3];
echo (array_walk_recursive($e, function (&$v) { $v = $v + 1; }) === true) ? "1" : "0";
$f = [3, 1];
echo (rsort($f) === true) ? "1" : "0";
echo "\n";
"#,
    );
    assert_eq!(out, "1111111\n");
}

/// A nullable scalar parameter holding null is falsy in every condition consumer. Its storage
/// is the inline tagged pair while its semantic type is the `int|null` union, so the truthiness
/// dispatch must look at the storage repr; reading it as a Mixed cell dereferenced the raw null
/// sentinel and crashed (#1996). Covers `if`, `&&`, `!`, `?:`, `while`, `(bool)` and the
/// `?string`/`?float`/`?bool` nullables, in both directions.
#[test]
fn test_tagged_nullable_scalar_is_falsy_in_conditions() {
    let out = compile_and_run(
        r#"<?php
function ifc(?int $x): string { if ($x) { return "T"; } return "F"; }
function neg(?int $x): string { return $x ? "T" : "F"; }
function notc(?int $x): string { return !$x ? "N" : "Y"; }
function andc(?int $x): string { return $x && true ? "T" : "F"; }
function elvis(?int $x): string { return $x ?: "D"; }
function cast(?int $x): string { return (bool) $x ? "T" : "F"; }
function whilec(?int $x): string { while ($x) { return "T"; } return "F"; }
function s(?string $x): string { return $x ? "T" : "F"; }
function fl(?float $x): string { return $x ? "T" : "F"; }
function bo(?bool $x): string { return $x ? "T" : "F"; }
function emp(?int $x): string { return empty($x) ? "E" : "N"; }
echo ifc(null), neg(null), neg(0), neg(-3), notc(null), andc(null), elvis(null), cast(null), whilec(null), s(null), fl(null), bo(null), "\n";
echo ifc(3), andc(3), cast(3), s("a"), fl(1.5), bo(true), "\n";
echo emp(null), emp(0), emp(5), "\n";
"#,
    );
    assert_eq!(out, "FFFTNFDFFFFF\nTTTTTT\nEEN\n");
}
