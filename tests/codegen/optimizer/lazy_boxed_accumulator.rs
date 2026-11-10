//! Purpose:
//! Checks PHP-exact results for accumulators that the lazy boxing pass keeps as raw integers.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_lazy_accumulator`.
//!
//! Key details:
//! - Runtime `$argc` keeps the arithmetic out of the AST constant folder.
//! - Every fixture runs with heap checks: a stale box left in a slot must still be freed once.
//! - Expected output comes from PHP 8.5, including float promotion on overflow and the
//!   coercion of float, string, null, and bool operands.

use super::*;

/// Assembly comment emitted only by a `cond_br` fused with `IOverflows`.
const FUSED_OVERFLOW_BRANCH: &str = "branch on signed overflow flags";

/// Runs one fixture with heap checks and returns stdout.
fn run(source: &str) -> String {
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}", output.stderr);
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        output.stderr
    );
    output.stdout
}

/// Array reads, calls, branches, and nested loops keep the accumulator a raw integer.
#[test]
fn test_lazy_accumulator_general_loop_bodies() {
    let source = r#"<?php
function total(array $a): mixed {
    $sum = 0;
    foreach ($a as $v) {
        $sum += $v;
    }
    return $sum;
}
function indexed(array $a, int $n): mixed {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) {
        $sum += $a[$i];
    }
    return $sum;
}
function count_letter(string $s): mixed {
    $c = 0;
    $n = strlen($s);
    for ($i = 0; $i < $n; $i++) {
        if ($s[$i] === 'a') {
            $c++;
        }
    }
    return $c;
}
function lengths(int $n): mixed {
    $t = 0;
    for ($i = 0; $i < $n; $i++) {
        $t += strlen(str_repeat("x", $i));
    }
    return $t;
}
function grid(int $n): mixed {
    $t = 0;
    for ($i = 0; $i < $n; $i++) {
        for ($j = 0; $j < $n; $j++) {
            if (($i + $j) % 2 == 0) {
                $t += $i * $j;
            } else {
                $t -= 1;
            }
        }
    }
    return $t;
}
var_dump(total([1, 2, 3, $argc]), indexed([5, 6, 7], 2 + $argc), count_letter("banana"), lengths(4 + $argc), grid(4 + $argc));
"#;
    assert_eq!(run(source), "int(7)\nint(18)\nint(3)\nint(10)\nint(40)\n");
}

/// Six accumulators in one loop are all rewritten, with exact results.
#[test]
fn test_lazy_accumulator_six_accumulators_in_one_loop() {
    let source = r#"<?php
function many(array $a): mixed {
    $s1 = 0;
    $s2 = 0;
    $s3 = 0;
    $s4 = 0;
    $s5 = 0;
    $s6 = 1;
    foreach ($a as $v) {
        $s1 += $v;
        $s2 -= $v;
        $s3 += 2;
        $s4++;
        $s5--;
        $s6 *= 2;
    }
    return [$s1, $s2, $s3, $s4, $s5, $s6];
}
var_dump(many([1, 2, 3, $argc]));
"#;
    assert_eq!(run(source), "array(6) {\n  [0]=>\n  int(7)\n  [1]=>\n  int(-7)\n  [2]=>\n  int(8)\n  [3]=>\n  int(4)\n  [4]=>\n  int(-4)\n  [5]=>\n  int(16)\n}\n");
}

/// Overflow, floats, numeric strings, null, and bool operands take the mixed path with PHP results.
#[test]
fn test_lazy_accumulator_overflow_and_non_integer_operands() {
    let source = r#"<?php
function total(array $a, mixed $seed): mixed {
    $sum = $seed;
    foreach ($a as $v) {
        $sum += $v;
    }
    return $sum;
}
function product(array $a): mixed {
    $p = 1;
    foreach ($a as $v) {
        $p *= $v;
    }
    return $p;
}
function down(array $a): mixed {
    $s = PHP_INT_MIN + 2;
    foreach ($a as $v) {
        $s -= $v;
    }
    return $s;
}
function reversed(array $a): mixed {
    $s = 100;
    foreach ($a as $v) {
        $s = $v - $s;
    }
    return $s;
}
function then_float(array $a): mixed {
    $sum = PHP_INT_MAX - 4;
    $n = 0;
    foreach ($a as $v) {
        $sum += $v;
        $n++;
    }
    $sum -= 1.5;
    return [$sum, $n];
}
var_dump(
    total([1, 2, 3], PHP_INT_MAX - 4 - $argc),
    total([1, 2, 3, 4, 5], PHP_INT_MAX - 4),
    total([1, 2.5, 3], 0),
    total([1, "2", "3.5"], 0),
    total([1, null, true, 4], 0),
    total([1, 2], 0.5),
    product([3037000500, 3037000500, 2]),
    product([7, 8, $argc]),
    down([1, 2, 3, 4]),
    reversed([1, 2, 3]),
    then_float([1, 2, 3, 4, 5])
);
"#;
    assert_eq!(run(source), "float(9.223372036854776E+18)\nfloat(9.223372036854776E+18)\nfloat(6.5)\nfloat(6.5)\nint(6)\nfloat(3.5)\nfloat(1.84467440740005E+19)\nint(56)\nfloat(-9.223372036854776E+18)\nint(-98)\narray(2) {\n  [0]=>\n  float(9.223372036854776E+18)\n  [1]=>\n  int(5)\n}\n");
}

/// Reads, plain stores, by-reference calls, and early returns see the current value.
#[test]
fn test_lazy_accumulator_reads_stores_and_references() {
    let source = r#"<?php
function bump(&$x): void {
    $x += 1;
}
function id(mixed $v): mixed {
    return $v;
}
function rare_read(array $a): mixed {
    $sum = 0;
    $out = "";
    foreach ($a as $v) {
        $sum += $v;
        if ($v == 3) {
            $out .= "[$sum]";
        }
    }
    return $out . $sum;
}
function every_read(array $a): mixed {
    $sum = 0;
    $out = "";
    foreach ($a as $v) {
        $sum += $v;
        $out .= $sum . ",";
    }
    return $out;
}
function by_reference(array $a): mixed {
    $sum = 0;
    foreach ($a as $v) {
        $sum += $v;
        if ($v == 2) {
            bump($sum);
        }
    }
    bump($sum);
    return $sum;
}
function reassigned(array $a): mixed {
    $sum = 0;
    foreach ($a as $v) {
        $sum += $v;
        $sum = id($sum);
    }
    return $sum;
}
function best_run(array $a): mixed {
    $best = 0;
    $run = 0;
    foreach ($a as $v) {
        if ($v < 0) {
            $run = 0;
        } else {
            $run += $v;
        }
        if ($run > $best) {
            $best = $run;
        }
    }
    return $best;
}
function early(array $a): mixed {
    $sum = 0;
    foreach ($a as $v) {
        $sum += $v;
        if ($sum > 5) {
            return $sum;
        }
    }
    return -$sum;
}
var_dump(
    rare_read([1, 2, 3, 4]),
    every_read([1, 2, 3]),
    by_reference([1, 2, 3]),
    reassigned([1, 2, 3]),
    best_run([1, 2, -1, 5, 6, -2, 3]),
    early([1, 2, 3, 4]),
    early([1, $argc])
);
"#;
    assert_eq!(run(source), "string(5) \"[6]10\"\nstring(6) \"1,3,6,\"\nint(8)\nint(6)\nint(11)\nint(6)\nint(-2)\n");
}

/// A compare of the accumulator is an integer compare while both sides are ints.
#[test]
fn test_lazy_accumulator_compares_inside_the_loop() {
    let source = r#"<?php
function capped(array $a): mixed {
    $sum = 0;
    foreach ($a as $v) {
        $sum += $v;
        if ($sum > 100) {
            break;
        }
    }
    return $sum;
}
function inclusive(array $a, int $n): mixed {
    $t = 0;
    for ($i = 1; $i <= $n; $i++) {
        $t += $a[$i - 1];
    }
    return [$t, $i];
}
function equalities(array $a): mixed {
    $s = 0;
    $hits = 0;
    foreach ($a as $v) {
        $s += $v;
        if ($s == 6) {
            $hits++;
        }
        if ($s != 3) {
            $hits += 10;
        }
    }
    return $hits;
}
function past_max(array $a): mixed {
    $s = PHP_INT_MAX - 2;
    $n = 0;
    foreach ($a as $v) {
        $s += $v;
        if ($s > PHP_INT_MAX) {
            $n++;
        }
        if ($s >= 1.5) {
            $n += 100;
        }
    }
    return [$s, $n];
}
function mixed_bound(array $a): mixed {
    $s = 0;
    $n = 0;
    foreach ($a as $v) {
        $s += 1;
        if ($s < $v) {
            $n++;
        }
    }
    return $n;
}
function left_side(array $a): mixed {
    $s = 0;
    $n = 0;
    foreach ($a as $v) {
        $s += $v;
        if (10 > $s) {
            $n++;
        }
        if ($v <= $s) {
            $n += 100;
        }
    }
    return $n;
}
var_dump(
    capped([60, 50, 40]),
    capped([1, $argc]),
    inclusive([4, 5, 6], 2 + $argc),
    equalities([1, 2, 3, 4]),
    past_max([1, 1, 1, 1]),
    mixed_bound([5, "7", 2.5, null]),
    left_side([3, 4, 5])
);
"#;
    assert_eq!(run(source), "int(110)\nint(2)\narray(2) {\n  [0]=>\n  int(15)\n  [1]=>\n  int(4)\n}\nint(31)\narray(2) {\n  [0]=>\n  float(9.223372036854776E+18)\n  [1]=>\n  int(400)\n}\nint(2)\nint(302)\n");
}

/// Top-level accumulators in loops with calls and nested arrays are rewritten too.
#[test]
fn test_lazy_accumulator_top_level_code() {
    let source = r#"<?php
$tot = 0;
$words = ["a", "bb", "ccc"];
foreach ($words as $w) {
    $tot += strlen($w);
}
var_dump($tot);
$grid = [[1, 2], [3, 4 + $argc]];
$g = PHP_INT_MAX - 10;
foreach ($grid as $row) {
    foreach ($row as $cell) {
        $g += $cell;
    }
}
var_dump($g);
"#;
    assert_eq!(run(source), "int(6)\nfloat(9.223372036854776E+18)\n");
}

/// Two rewritten accumulators compared or added to each other read current values.
#[test]
fn test_lazy_accumulator_two_accumulators_read_each_other() {
    let source = r#"<?php
function same_pace(array $a): mixed {
    $x = 0;
    $y = 0;
    foreach ($a as $v) {
        $x += $v;
        $y += $v;
    }
    if ($x == $y) {
        return "eq " . $x;
    }
    return "ne " . $x . " " . $y;
}
function negatives(int $n): mixed {
    $x = 5;
    $y = 1;
    $hits = 0;
    for ($i = 0; $i < $n; $i++) {
        $x -= 3;
        $y -= 1;
        if ($x < $y) {
            $hits += 1;
        }
        if ($x > $y) {
            $hits += 10;
        }
    }
    return [$x, $y, $hits];
}
function after_loop(array $a): mixed {
    $x = 0;
    $y = 0;
    foreach ($a as $v) {
        $x += $v;
        $y += 3;
    }
    $mask = 0;
    if ($x < $y) {
        $mask += 1;
    }
    if ($x == $y) {
        $mask += 2;
    }
    if ($x > $y) {
        $mask += 4;
    }
    if ($x <= $y) {
        $mask += 8;
    }
    if ($x >= $y) {
        $mask += 16;
    }
    if ($x != $y) {
        $mask += 32;
    }
    return [$x, $y, $mask];
}
function inside_loop(array $a): mixed {
    $x = 0;
    $y = 0;
    $mask = 0;
    foreach ($a as $v) {
        $x += $v;
        $y += 2;
        if ($x < $y) {
            $mask += 1;
        }
        if ($x == $y) {
            $mask += 10;
        }
        if ($x > $y) {
            $mask += 100;
        }
        if ($x <= $y) {
            $mask += 1000;
        }
        if ($x >= $y) {
            $mask += 10000;
        }
        if ($x != $y) {
            $mask += 100000;
        }
    }
    return [$x, $y, $mask];
}
function summed(array $a): mixed {
    $x = 0;
    $y = 0;
    foreach ($a as $v) {
        $x += $v;
        $y += $x;
        $x += $y;
    }
    return [$x, $y];
}
var_dump(
    same_pace([1, 2, 3, $argc]),
    negatives(3 + $argc),
    after_loop([1, 2, 3, $argc]),
    inside_loop([1, 2, 3, $argc]),
    summed([1, 2, 3, $argc])
);
"#;
    assert_eq!(run(source), "string(4) \"eq 7\"\narray(3) {\n  [0]=>\n  int(-7)\n  [1]=>\n  int(-3)\n  [2]=>\n  int(12)\n}\narray(3) {\n  [0]=>\n  int(7)\n  [1]=>\n  int(12)\n  [2]=>\n  int(41)\n}\narray(3) {\n  [0]=>\n  int(7)\n  [1]=>\n  int(8)\n  [2]=>\n  int(314013)\n}\narray(2) {\n  [0]=>\n  int(77)\n  [1]=>\n  int(47)\n}\n");
}

/// Literal `eval`, `get_defined_vars`, statics, `isset`, and `unset` see the current value.
#[test]
fn test_lazy_accumulator_locals_read_by_name_stay_current() {
    let source = r#"<?php
function inside(int $n): void {
    $s = 0;
    for ($i = 0; $i < $n; $i++) {
        $s += 10;
        if ($i + 1 == $n) {
            eval('echo $s, "\n";');
        }
    }
    echo $s, "\n";
}
function after(int $n): void {
    $s = 0;
    for ($i = 0; $i < $n; $i++) {
        $s += 10;
        if ($i == 0) {
            echo "";
        }
    }
    eval('echo $s, "\n";');
}
function vars(array $a): mixed {
    $s = 0;
    foreach ($a as $v) {
        $s += $v;
        if ($v == 2) {
            echo get_defined_vars()['s'], "\n";
        }
    }
    return get_defined_vars()['s'];
}
function keep_sum(array $a): mixed {
    static $s = 0;
    foreach ($a as $v) {
        $s += $v;
    }
    return $s;
}
function has_sum(array $a): mixed {
    $s = 0;
    $n = 0;
    foreach ($a as $v) {
        $s += $v;
        if (isset($s)) { $n++; }
        if ($v == 2) { unset($s); $s = 50; }
    }
    return [$s, $n];
}
inside(2 + $argc);
after(2 + $argc);
var_dump(vars([1, 2, 3]));
var_dump(keep_sum([1, 2]));
var_dump(keep_sum([3]));
var_dump(has_sum([1, 2, 3]));
"#;
    assert_eq!(run(source), "30\n30\n30\n3\nint(6)\nint(3)\nint(6)\narray(2) {\n  [0]=>\n  int(53)\n  [1]=>\n  int(3)\n}\n");
}

/// The `foreach` sum tests overflow with a fused branch instead of boxing every iteration.
#[test]
fn test_lazy_accumulator_foreach_sum_branches_on_overflow() {
    let source = r#"<?php
function total(array $a): mixed {
    $sum = 0;
    foreach ($a as $v) {
        $sum += $v;
    }
    return $sum;
}
echo total([1, 2, 3, $argc]);
"#;
    let (output, asm) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(output.success, "{}", output.stderr);
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        output.stderr
    );
    assert_eq!(output.stdout, "7");
    assert!(
        asm.contains(FUSED_OVERFLOW_BRANCH),
        "expected a fused signed overflow branch in the loop:\n{asm}"
    );
}
