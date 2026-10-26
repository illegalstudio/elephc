//! Purpose:
//! Executable regressions for direct short-circuit condition lowering.
//!
//! Called from:
//! - `tests/codegen/optimizer.rs` through the codegen test harness.
//!
//! Key details:
//! - Runs with EIR optimization enabled and disabled.
//! - Runtime arguments and observable calls keep the tested conditions alive.

use super::*;

/// Asserts a fixture prints the same expected result with either EIR optimizer mode.
fn assert_both_modes(source: &str, expected: &str) {
    assert_eq!(compile_and_run(source), expected);
    without_ir_opt(|| assert_eq!(compile_and_run(source), expected));
}

/// Nested logical conditions preserve every truth-table row and source-order side effect.
#[test]
fn test_branch_conditions_nested_source_order() {
    assert_both_modes(r#"<?php
function mark(string $label, bool $value): bool { echo $label; return $value; }
function decision(bool $a, bool $b, bool $c): void {
    if ((mark("a", $a) || mark("b", $b)) && !mark("c", $c)) { echo "T"; }
    elseif (!(mark("d", $a) && (mark("e", $b) || !mark("f", $c)))) { echo "F"; }
    else { echo "E"; }
    echo ":", (mark("g", $a) and mark("h", $b) or mark("i", $c)) ? "Y" : "N", "\n";
}
for ($i = 0; $i < 8; $i++) { decision(($i & 4) !== 0, ($i & 2) !== 0, ($i & 1) !== 0); }
"#, "abdF:giN\nabdF:giY\nabcT:giN\nabcdF:giY\nacT:ghiN\nacdefF:ghiY\nacT:ghY\nacdeE:ghY\n");
}

/// Loop headers, negation, continue targets, and value-producing logical operators agree.
#[test]
fn test_branch_conditions_loop_headers_and_boolean_values() {
    assert_both_modes(r#"<?php
function tick(int $value): int { echo "t", $value, ";"; return $value; }
function run(int $start): void {
    $i = $start;
    while ($i < 4 && tick($i)) { $i++; if ($i === 3) continue; echo "w", $i, ";"; }
    do { $i--; if ($i === 3) continue; echo "d", $i, ";"; } while (!($i < 2 || tick($i) === 0));
    for ($j = 0; $j < 4 && (tick($j) || $i < 2); $j++) {
        if ($j === 1) continue;
        echo "f", $j, ";";
    }
    $value = $i > 0 || tick(9);
    var_dump($value, !($i > 0 && $j > 0), ($i > 0 xor tick(7)));
}
run($argc);
"#, "t1;w2;t2;t3;w4;t3;d2;t2;d1;t0;f0;t1;t2;f2;t3;f3;t7;bool(true)\nbool(false)\nbool(false)\n");
}

/// A skipped operand's array mutation cannot change storage layout on just one branch.
#[test]
fn test_branch_conditions_conditional_array_widening() {
    assert_both_modes(r#"<?php
function conditional_write(int $c): void {
    $values = [$c, 2, 3];
    if ($c === 1 && (($values[0] = "s") === "s")) { echo "T"; }
    else { echo "F"; }
    echo $values[1], "|", $values[0], "\n";
}
conditional_write($argc - 1);
conditional_write($argc);
"#, "F2|0\nT2|s\n");
}

/// Branches retain PHP truthiness for null, zero strings, arrays, objects, and reference calls.
#[test]
fn test_branch_conditions_truthiness_and_reference_calls() {
    assert_both_modes(r#"<?php
class Flag {}
function identity(mixed $value): mixed { return $value; }
function truth(mixed $value): void {
    if (identity($value) || identity(0)) { echo "T"; } else { echo "F"; }
    echo (!identity($value) && !identity(false)) ? "f" : "t";
    echo ":";
}
function &flag_ref(): string { $value = "live"; return $value; }
truth(null); truth(false); truth($argc - 1); truth((float)($argc - 1));
truth(""); truth("0"); truth("00"); truth([]); truth([0]); truth(new Flag());
if (flag_ref() && !identity(0) && flag_ref()) { echo "R"; }
"#, "Ff:Ff:Ff:Ff:Ff:Ff:Tt:Ff:Tt:Tt:R");
}

/// Direct and negated float branches keep unordered NaN and signed-zero behavior.
#[test]
fn test_branch_conditions_float_unordered_and_negation() {
    assert_both_modes(r#"<?php
function classify(float $x, float $y): void {
    echo !($x < $y || $x >= $y) ? "U" : "O";
    echo ($x <= $y && !($x > $y)) ? "L" : "-";
    echo ($x > $y || !($x <= $y)) ? "G" : "-";
    echo "\n";
}
classify(NAN, (float)$argc); classify((float)$argc, NAN);
classify(-1.0, (float)$argc); classify((float)$argc, -1.0);
classify((float)$argc, 1.0); classify(-0.0, 0.0);
"#, "U-G\nU-G\nOL-\nO-G\nOL-\nOL-\n");
}

/// A packed collision-grid chain and separate rejection statements find identical pairs.
#[test]
fn test_branch_conditions_collision_grid() {
    assert_both_modes(r#"<?php
packed class Box { public float $minX; public float $maxX; public float $minY; public float $maxY; }
function chained(buffer<Box> $boxes, int $n): int {
    $found = 0;
    for ($i = 0; $i < $n; $i++) {
        for ($j = 0; $j < $n; $j++) {
            if ($boxes[$i]->minX >= $boxes[$j]->maxX
                || $boxes[$j]->minX >= $boxes[$i]->maxX
                || $boxes[$i]->minY >= $boxes[$j]->maxY
                || $boxes[$j]->minY >= $boxes[$i]->maxY) { continue; }
            $found++;
        }
    }
    return $found;
}
function separate(buffer<Box> $boxes, int $n): int {
    $found = 0;
    for ($i = 0; $i < $n; $i++) {
        for ($j = 0; $j < $n; $j++) {
            if ($boxes[$i]->minX >= $boxes[$j]->maxX) continue;
            if ($boxes[$j]->minX >= $boxes[$i]->maxX) continue;
            if ($boxes[$i]->minY >= $boxes[$j]->maxY) continue;
            if ($boxes[$j]->minY >= $boxes[$i]->maxY) continue;
            $found++;
        }
    }
    return $found;
}
$n = $argc + 3;
$boxes = buffer_new<Box>($n);
for ($i = 0; $i < $n; $i++) {
    $boxes[$i]->minX = (float)$i;
    $boxes[$i]->maxX = (float)$i + 1.5;
    $boxes[$i]->minY = 0.0;
    $boxes[$i]->maxY = 1.0;
}
echo chained($boxes, $n), ":", separate($boxes, $n);
buffer_free($boxes);
"#, "10:10");
}
