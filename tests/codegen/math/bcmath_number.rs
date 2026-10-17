//! Purpose:
//! Exercises the complete immutable BCMath Number class through native EIR codegen.
//!
//! Called from:
//! - The codegen integration test binary's math group.
//!
//! Key details:
//! - Expected values are verified against PHP's BCMath implementation.

use crate::support::*;

/// Constructs a namespaced Number and checks its exact decimal methods and scale.
#[test]
fn test_bcmath_number_methods() {
    let out = compile_and_run(r#"<?php
use BcMath\Number;
$n = new Number('001.200');
echo $n->value, '|', $n->scale, '|', $n->add(2)->value, '|';
echo $n->sub('2.01')->value, '|', $n->mul('2.00')->value, '|';
echo $n->div(2)->value, '|', $n->mod('.5')->value, '|', $n->pow(2)->value, '|';
echo $n->compare('1.201'), '|', $n->ceil()->value, '|', $n->floor()->value, '|';
echo (new Number('4.00'))->sqrt()->value, '|';
echo (new Number(4))->powmod(13, 497)->value, '|';
$pair = $n->divmod('.2');
echo $pair[0]->value, '|', $pair[1]->value, '|', $n->round(1)->value;
"#);
    assert_eq!(out, "1.200|3|3.200|-0.810|2.40000|0.600|0.200|1.440000|-1|2|1|2.00|445|6|0.000|1.2");
}

/// Leaves the procedural scale independent of Number methods and exposes catchable errors.
#[test]
fn test_bcmath_number_scale_and_errors() {
    let out = compile_and_run(r#"<?php
use BcMath\Number;
bcscale(7);
$n = new Number('1.00');
echo $n->div(8)->value, '|', $n->div(8, 5)->value, '|', bcscale(), '|';
try { $n->div(0); } catch (DivisionByZeroError $e) { echo $e->getMessage(), '|'; }
try { new Number('1e2'); } catch (ValueError $e) { echo $e->getMessage(), '|'; }
try { $n->add(2, -1); } catch (ValueError $e) { echo $e->getMessage(); }
"#);
    assert_eq!(out, "0.125|0.12500|7|Division by zero|BcMath\\Number::__construct(): Argument #1 ($num) is not well-formed|BcMath\\Number::add(): Argument #2 ($scale) must be between 0 and 2147483647");
}

/// Dispatches every arithmetic operator and keeps value equality separate from identity.
#[test]
fn test_bcmath_number_operators() {
    let out = compile_and_run(r#"<?php
use BcMath\Number;
$n = new Number('1.20');
echo ($n + 2)->value, '|', (2 + $n)->value, '|', ($n - '.2')->value, '|';
echo (3 - $n)->value, '|', ($n * '2.0')->value, '|', ($n / 8)->value, '|';
echo ($n % '.5')->value, '|', ($n ** 2)->value, '|', (-$n)->value, '|', (+$n)->value, '|';
var_dump($n == new Number('1.200'), $n === new Number('1.20'), $n === $n, +$n === $n);
var_dump($n < 2, $n <= '1.2', 2 > $n, $n >= '1.20', $n != 2, $n <=> 2);
"#);
    assert_eq!(out, "3.20|3.20|1.00|1.80|2.400|0.15|0.20|1.4400|-1.20|1.20|bool(true)\nbool(false)\nbool(true)\nbool(false)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\nint(-1)\n");
}

/// Replaces immutable objects for compound assignment and prefix/postfix updates.
#[test]
fn test_bcmath_number_immutable_replacement() {
    let out = compile_and_run(r#"<?php
use BcMath\Number;
$n = new Number('1.20');
$alias = $n;
$old = $n++;
echo $alias->value, '|', $old->value, '|', $n->value, '|', (++$n)->value, '|';
echo ($n--)->value, '|', (--$n)->value, '|';
$n += 2;
$n *= '2.0';
$n /= 4;
$n %= 1;
$n **= 2;
echo $n->value, '|', $alias->value;
"#);
    assert_eq!(out, "1.20|1.20|2.20|3.20|3.20|1.20|0.360000|1.20");
}

/// Shares Number construction and operators with eval and preserves zero truthiness.
#[test]
fn test_bcmath_number_eval_and_truthiness() {
    let out = compile_and_run(r#"<?php
use BcMath\Number;
$zero = new Number('-0.00');
$value = new Number('1.20');
var_dump((bool)$zero, !$zero, empty($zero), (bool)$value);
$source = $argc > 1 ? 'echo "unexpected";' : '$sum = $value + new \\BcMath\\Number("2.30"); echo $sum->value, "|"; var_dump($sum > 3, (bool)$zero, !$zero, empty($zero)); $sum++; echo $sum->value;';
eval($source);
"#);
    assert_eq!(out, "bool(false)\nbool(true)\nbool(true)\nbool(true)\n3.50|bool(true)\nbool(false)\nbool(true)\nbool(true)\n4.50");
}

/// Round-trips immutable storage and refuses reinitialization and property mutation.
#[test]
fn test_bcmath_number_serialization_and_readonly() {
    let out = compile_and_run(r#"<?php
use BcMath\Number;
$n = new Number('1.200');
echo serialize($n), '|';
$restored = unserialize(serialize($n));
echo $restored->value, '|', $restored->scale, '|';
echo (clone $n)->value, '|', json_encode($n), '|';
try { $n->value = '2'; } catch (Error $e) { echo $e->getMessage(), '|'; }
try { $n->__construct('2'); } catch (Error $e) { echo $e->getMessage(); }
"#);
    assert_eq!(out, "O:13:\"BcMath\\Number\":1:{s:5:\"value\";s:5:\"1.200\";}|1.200|3|1.200|{\"value\":\"1.200\",\"scale\":3}|Cannot modify readonly property BcMath\\Number::$value|Cannot modify readonly property BcMath\\Number::$value");
}

/// Uses all eight PHP enum modes without exposing their internal integer representation.
#[test]
fn test_bcmath_number_rounding_mode_enum() {
    let out = compile_and_run(r#"<?php
$n = new BcMath\Number('-2.5');
foreach (RoundingMode::cases() as $mode) {
    echo $mode->name, ':', $n->round(mode: $mode)->value, '|';
}
"#);
    assert_eq!(out, "HalfAwayFromZero:-3|HalfTowardsZero:-2|HalfEven:-2|HalfOdd:-3|TowardsZero:-2|AwayFromZero:-3|NegativeInfinity:-3|PositiveInfinity:-2|");
}

/// Handles object values hidden in mixed storage while preserving scalar fallback semantics.
#[test]
fn test_bcmath_number_mixed_operators() {
    let out = compile_and_run(r#"<?php
function decimal_add(mixed $left, mixed $right): mixed { return $left + $right; }
function decimal_div(mixed $left, mixed $right): mixed { return $left / $right; }
function decimal_compare(mixed $left, mixed $right): int { return $left <=> $right; }
$number = new BcMath\Number('1.20');
echo decimal_add($number, 2)->value, '|', decimal_div($number, 8)->value, '|';
echo decimal_compare($number, '1.200'), '|';
var_dump(decimal_add(2, 3), decimal_div(8, 2));
"#);
    assert_eq!(out, "3.20|0.15|0|int(5)\nfloat(4)\n");
}

/// Unordered values never satisfy relational operators, while booleans use Number truthiness.
#[test]
fn test_bcmath_number_comparison_domains() {
    let out = compile_and_run(r#"<?php
function compare_number(mixed $a, mixed $b): void {
    echo (int)($a == $b), (int)($a != $b), (int)($a < $b), (int)($a <= $b), (int)($a > $b), (int)($a >= $b), '|', $a <=> $b, '|', $b <=> $a, ';';
}
$n = new BcMath\Number('0.00');
compare_number($n, 'bad');
compare_number($n, []);
compare_number($n, false);
compare_number($n, true);
compare_number($n, null);
$nonzero = new BcMath\Number('2.50');
$truth = $argc > 0;
compare_number($nonzero, $truth);
try { $n + 'bad'; } catch (ValueError $e) { echo $e->getMessage(), '|'; }
function add_number(mixed $a, mixed $b): mixed { return $a + $b; }
try { add_number($n, []); } catch (TypeError $e) { echo $e->getMessage(); }
class OtherNumberDomain {}
var_dump($n == new OtherNumberDomain(), $n < new OtherNumberDomain(), $n <=> new OtherNumberDomain());
"#);
    assert_eq!(out, "010000|1|1;010000|1|1;100101|0|0;011100|-1|1;010011|1|-1;100101|0|0;Right string operand cannot be converted to BcMath\\Number|Unsupported operand types: BcMath\\Number + arraybool(false)\nbool(false)\nint(1)\n");
}

/// Switch labels and loose membership share the Number comparison handler in either position.
#[test]
fn test_bcmath_number_switch_and_membership() {
    let out = compile_and_run(r#"<?php
use BcMath\Number;
function mixed_switch(mixed $n): string {
    switch ($n) {
        case new Number('9.00'): return 'wrong';
        case '5.0': return 'five';
        default: return 'miss';
    }
}
$n = new Number('1.20');
switch ($n) { case '1.200': echo 'S'; break; default: echo 'wrong'; }
switch ($n) { case new Number('1.200'): echo 'N'; break; default: echo 'wrong'; }
switch (new Number('5')) { case new Number('9.00'): echo 'wrong'; break; case 5: echo 'I'; }
switch (new Number('2.50')) { case true: echo 'B'; }
switch (5) { case new Number('5.0'): echo 'R'; }
echo '|', mixed_switch(new Number('5')), '|', mixed_switch(new Number('8')), '|';
var_dump(in_array(new Number('5'), ['5.0']), in_array($n, [new Number('1.200')]));
var_dump(in_array('5.0', [new Number('5')]), in_array($n, [new Number('1.200')], true));
var_dump(in_array(new Number('0'), ['bad', null, []]), in_array(new Number('0.00'), [false]));
"#);
    assert_eq!(out, "SNIBR|five|miss|bool(true)\nbool(true)\nbool(true)\nbool(false)\nbool(false)\nbool(true)\n");
}

/// Internal string-or-integer parameters truncate floats before decimal parsing, including Mixed.
#[test]
fn test_bcmath_number_parameter_coercion() {
    let out = compile_and_run(r#"<?php
use BcMath\Number;
function mixed_parameter(mixed $v): void {
    $n = new Number('2.50');
    echo (new Number($v))->value, '|', $n->add($v)->value, '|', $n->compare($v), ';';
}
$n = new Number('2.50');
$fraction = $argc + 0.5;
echo (new Number(1.5))->value, '|', $n->add($fraction)->value, '|', $n->compare(2.9), ';';
mixed_parameter($fraction);
mixed_parameter(true);
mixed_parameter(false);
mixed_parameter(null);
echo $n->sub(1.5)->value, '|', $n->mul(1.5)->value, '|', $n->div(1.5)->value, '|';
echo $n->mod(1.5)->value, '|', $n->pow(1.5)->value, '|';
$pair = $n->divmod(1.5);
echo $pair[0]->value, '|', $pair[1]->value, '|', (new Number(2))->powmod(3.9, 5.9)->value;
foreach ([INF, -INF, 1e20, 9223372036854775808.0] as $bad) {
    try { new Number($bad); } catch (ValueError $e) { echo '|new'; }
    try { $n->add($bad); } catch (ValueError $e) { echo '|add'; }
    try { $n->compare($bad); } catch (ValueError $e) { echo '|compare'; }
}
"#);
    assert_eq!(out, "1|3.50|1;1|3.50|1;1|3.50|1;0|2.50|1;0|2.50|1;1.50|2.50|2.50|0.50|2.50|2|0.50|3|new|add|compare|new|add|compare|new|add|compare|new|add|compare");
}

/// Invalid floating operator operands throw rather than silently becoming zero.
#[test]
fn test_bcmath_number_operator_float_range() {
    let out = compile_and_run(r#"<?php
use BcMath\Number;
function mixed_add(mixed $a, mixed $b): mixed { return $a + $b; }
$n = new Number('2.50');
echo ($n + 1.5)->value, '|', ($n + -9223372036854775808.0)->value, '|';
foreach ([INF, -INF, NAN, 1e20, -1e20, 9223372036854775808.0] as $bad) {
    try { $n + $bad; } catch (TypeError $e) { echo $e->getMessage(), ';'; }
    try { mixed_add($bad, $n); } catch (TypeError $e) { echo $e->getMessage(), ';'; }
}
"#);
    let errors = "Unsupported operand types: BcMath\\Number + float;Unsupported operand types: float + BcMath\\Number;";
    assert_eq!(out, format!("3.50|-9223372036854775805.50|{}", errors.repeat(6)));
}

/// Interface-typed objects dispatch boolean handlers using their actual runtime class.
#[test]
fn test_bcmath_number_stringable_truthiness() {
    let out = compile_and_run(r#"<?php
class OrdinaryStringable implements Stringable {
    public function __toString(): string { return '0'; }
}
function show_truthiness(Stringable $n): void { var_dump((bool)$n, !$n, empty($n)); }
show_truthiness(new BcMath\Number('0.00'));
show_truthiness(new OrdinaryStringable());
"#);
    assert_eq!(out, "bool(false)\nbool(true)\nbool(true)\nbool(true)\nbool(false)\nbool(false)\n");
}
