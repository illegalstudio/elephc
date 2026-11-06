//! Purpose:
//! Checks ownership of immutable Number operators, replacements, and exception operands.
//!
//! Called from:
//! - The codegen runtime GC integration group under heap-debug instrumentation.
//!
//! Key details:
//! - Every owned Number/string/cell must be released on normal and caught-error paths.

use crate::support::*;

/// Preserved switches dispatch boxed Number labels and keep ordinary scalar comparisons intact.
#[test]
fn test_bcmath_number_switch_mixed_numeric() {
    let out = compile_and_run_with_heap_debug(r#"<?php
use BcMath\Number;
function switch_numeric(mixed $n): void {
    switch ($n) { case 9: echo '-'; break; case 5: echo 'I'; break; default: echo '-'; }
    switch ($n) { case 9.0: echo '-'; break; case 5.0: echo 'F'; break; default: echo '-'; }
    switch (5) { case $n: echo 'R'; break; default: echo '-'; }
    switch (5.0) { case $n: echo 'S'; break; default: echo '-'; }
    echo '|';
}
function switch_boolean(bool $flag, mixed $n): void {
    switch ($flag) { case $n: echo 'B'; break; case false: echo 'F'; break; default: echo '-'; }
}
foreach ([new Number('5.00'), new Number('6.00'), 5, 6, 5.0, 6.0, '5.0', 'bad', true, false, null, []] as $value) {
    switch_numeric($value);
}
switch_boolean(false, new Number('0.00'));
switch_boolean(true, new Number('2.50'));
switch_boolean(true, new Number('0.00'));
"#);
    assert_eq!(out.stdout, "IFRS|----|IFRS|----|IFRS|----|IFRS|----|--RS|----|----|----|BB-", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Runtime equality and internal scalar adapters retire both successful and exceptional owners.
#[test]
fn test_bcmath_number_review_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function number_switch(mixed $n): bool {
    switch ($n) {
        case new BcMath\Number('9.00'): return false;
        case new BcMath\Number('1.200'): return true;
        default: return false;
    }
}
for ($i = 0; $i < 8; $i++) {
    $matched = number_switch(new BcMath\Number('1.20'));
    $member = in_array(new BcMath\Number('1.20'), [new BcMath\Number('1.200')]);
    switch (new BcMath\Number('5')) {
        case new BcMath\Number('5.00'): $matched = true; break;
        default: $matched = false;
    }
    switch (new BcMath\Number('8')) {
        case new BcMath\Number('9'): $member = false; break;
        default: break;
    }
    try {
        switch (new BcMath\Number('5')) { case new BcMath\Number('bad'): break; }
    } catch (ValueError $e) { unset($e); }
    $coerced = (new BcMath\Number(1.5))->add(1.5);
    try { (new BcMath\Number('2.50'))->add(INF); } catch (ValueError $e) { unset($e); }
    try { new BcMath\Number('2.50') + INF; } catch (TypeError $e) { unset($e); }
    unset($coerced);
}
echo (int)$matched, '|', (int)$member;
"#);
    assert_eq!(out.stdout, "1|1", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Scalar numeric conversions retire captured strings while preserving their original locals.
#[test]
fn test_bcmath_number_scalar_string_owners() {
    let body = r#"
$text = str_repeat('1', 2);
for ($i = 0; $i < 16; $i++) {
    $mask = ((int)$text) & 3;
    $remainder = $text % 3;
    $right = ($argc * 23) % str_repeat('2', 2);
}
echo $mask, '|', $remainder, '|', $right, '|', $text;
unset($text);
"#;
    for (setup, cleanup) in [
        ("<?php ", ""),
        ("<?php $number = new BcMath\\Number('1.20'); echo $number->value, '|'; ", "unset($number);"),
    ] {
        let out = compile_and_run_with_heap_debug(&[setup, body, cleanup].concat());
        let prefix = if cleanup.is_empty() { "" } else { "1.20|" };
        assert_eq!(out.stdout, format!("{prefix}3|2|1|11"), "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    }
}

/// Keeping Number available must not strand checked scalar boxes consumed by bitwise operators.
#[test]
fn test_bcmath_number_scalar_checked_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$number = new BcMath\Number('1.20');
for ($i = 0; $i < 16; $i++) {
    $p = ($i + 1) & 1;
    $q = ($i - 1) & 1;
    $r = ($i * 2) & 3;
}
echo $p, '|', $q, '|', $r;
unset($number);
"#);
    assert_eq!(out.stdout, "0|0|2", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Mixed unary operators return independent Numbers and retire temporary operands exactly once.
#[test]
fn test_bcmath_number_mixed_unary_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function negate_decimal(mixed $n): mixed { return -$n; }
function positive_decimal(mixed $n): mixed { return +$n; }
$n = new BcMath\Number('1.20');
$negative = negate_decimal($n);
$positive = positive_decimal($n);
echo $negative->value, '|', $positive->value, '|';
var_dump($negative === $n, $positive === $n);
var_dump(negate_decimal(3), negate_decimal(1.5), negate_decimal('2.5'));
for ($i = 0; $i < 8; $i++) {
    $temporary = negate_decimal(new BcMath\Number('0.00'));
    unset($temporary);
}
unset($n, $negative, $positive);
"#);
    assert_eq!(out.stdout, "-1.20|1.20|bool(false)\nbool(false)\nint(-3)\nfloat(-1.5)\nfloat(-2.5)\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Repeatedly constructs and replaces native and boxed Numbers without retaining temporaries.
#[test]
fn test_bcmath_number_heap_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function calculate(mixed $n): mixed { return ($n + '0.25') * 2; }
for ($i = 0; $i < 16; $i++) {
    $n = new BcMath\Number('1.20');
    $alias = $n;
    $old = $n++;
    $n += new BcMath\Number('.30');
    $result = calculate($n);
    $pair = $result->divmod(2);
    $text = (string)($result / 4);
    unset($n, $alias, $old, $result, $pair, $text);
}
echo 'done';
"#);
    assert_eq!(out.stdout, "done", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A right operand can replace the left variable without changing the captured operator value.
#[test]
fn test_bcmath_number_operand_order_and_capture() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function replace(BcMath\Number &$n): BcMath\Number {
    echo 'R';
    $n = new BcMath\Number(10);
    return new BcMath\Number(2);
}
$n = new BcMath\Number('1.20');
echo ($n + replace($n))->value, '|', $n->value;
unset($n);
"#);
    assert_eq!(out.stdout, "R3.20|10", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Failed operations retire fresh temporary operands before entering a same-frame catch.
#[test]
fn test_bcmath_number_exception_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function add_unknown_number(mixed $a, mixed $b): mixed { return $a + $b; }
for ($i = 0; $i < 8; $i++) {
    try { new BcMath\Number('1.20') / new BcMath\Number(0); }
    catch (DivisionByZeroError $e) { unset($e); }
    try { new BcMath\Number('1.20') + 'invalid'; }
    catch (ValueError $e) { unset($e); }
    try { add_unknown_number(new BcMath\Number('1.20'), []); }
    catch (TypeError $e) { unset($e); }
}
echo 'caught';
"#);
    assert_eq!(out.stdout, "caught", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Dynamic eval releases operator results and compares null through the Number contract.
#[test]
fn test_bcmath_number_eval_heap_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$n = new BcMath\Number('1.20');
$source = $argc > 1 ? 'echo "unexpected";' : '
for ($i = 0; $i < 8; $i++) {
    $result = ($n + "0.30") * 2;
    $result++;
    try { $result / new \\BcMath\\Number(0); } catch (DivisionByZeroError $e) { unset($e); }
    unset($result);
}
$zero = new \\BcMath\\Number("0.00");
echo (int)($zero == null), (int)(null == $zero), (int)($zero != null), (int)(null != $zero);
echo (int)($zero < null), (int)(null < $zero), "|", $zero <=> null, "|", null <=> $zero, ";";
echo (int)($n == null), (int)(null == $n), (int)($n != null), (int)(null != $n);
echo (int)($n < null), (int)(null < $n), "|", $n <=> null, "|", null <=> $n;
unset($zero);
echo "eval";';
eval($source);
unset($n, $source);
"#);
    assert_eq!(out.stdout, "001101|1|-1;001101|1|-1eval", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Silent empty probes share Number truthiness for properties, array items, and magic getters.
#[test]
fn test_bcmath_number_empty_places() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NumberBox {
    public BcMath\Number $number;
    public static BcMath\Number $shared;
}
class MagicNumberBox {
    public function __isset(string $name): bool { return true; }
    public function __get(string $name): BcMath\Number { return new BcMath\Number('0.00'); }
}
$box = new NumberBox();
var_dump(empty($box->number), empty(NumberBox::$shared));
$box->number = new BcMath\Number('0.00');
NumberBox::$shared = new BcMath\Number('0.00');
$items = [new BcMath\Number('0.00')];
var_dump(empty($box->number), empty(NumberBox::$shared), empty($items[0]), empty((new MagicNumberBox())->missing), empty($undefined));
unset($box, $items);
"#);
    assert_eq!(out.stdout, "bool(true)\n".repeat(7), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
