//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow, assignments compound and values, including pre increment, post increment, and pre decrement.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_pre_increment() {
    // Verifies pre-increment (`++$i`) increments before value capture.
    // Fixture: simple local `$i = 1` then `$k = ++$i`, expects `$i` and `$k` both 2.
    let out = compile_and_run("<?php $i = 1; $k = ++$i; echo $i . \" \" . $k;");
    assert_eq!(out, "2 2");
}

#[test]
fn test_post_increment() {
    // Verifies post-increment (`$i++`) captures value before increment.
    // Fixture: simple local `$i = 1` then `$k = $i++`, expects `$i` = 2 and `$k` = 1.
    let out = compile_and_run("<?php $i = 1; $k = $i++; echo $i . \" \" . $k;");
    assert_eq!(out, "2 1");
}

#[test]
fn test_pre_decrement() {
    // Verifies pre-decrement (`--$i`) decrements before value capture.
    // Fixture: simple local `$i = 5` then `$k = --$i`, expects `$i` and `$k` both 4.
    let out = compile_and_run("<?php $i = 5; $k = --$i; echo $i . \" \" . $k;");
    assert_eq!(out, "4 4");
}

#[test]
fn test_post_decrement() {
    // Verifies post-decrement (`$i--`) captures value before decrement.
    // Fixture: simple local `$i = 5` then `$k = $i--`, expects `$i` = 4 and `$k` = 5.
    let out = compile_and_run("<?php $i = 5; $k = $i--; echo $i . \" \" . $k;");
    assert_eq!(out, "4 5");
}

#[test]
fn test_plus_assign() {
    // Verifies `+=` compound addition on integer locals.
    // Fixture: `$x = 10; $x += 5;` expects output "15".
    let out = compile_and_run("<?php $x = 10; $x += 5; echo $x;");
    assert_eq!(out, "15");
}

#[test]
fn test_minus_assign() {
    // Verifies `-=` compound subtraction on integer locals.
    // Fixture: `$x = 10; $x -= 3;` expects output "7".
    let out = compile_and_run("<?php $x = 10; $x -= 3; echo $x;");
    assert_eq!(out, "7");
}

#[test]
fn test_star_assign() {
    // Verifies `*=` compound multiplication on integer locals.
    // Fixture: `$x = 6; $x *= 7;` expects output "42".
    let out = compile_and_run("<?php $x = 6; $x *= 7; echo $x;");
    assert_eq!(out, "42");
}

#[test]
fn test_slash_assign() {
    // Verifies `/=` compound division on integer locals.
    // Fixture: `$x = 84; $x /= 2;` expects output "42".
    let out = compile_and_run("<?php $x = 84; $x /= 2; echo $x;");
    assert_eq!(out, "42");
}

#[test]
fn test_percent_assign() {
    // Verifies `%=` compound modulo on integer locals.
    // Fixture: `$x = 10; $x %= 3;` expects output "1".
    let out = compile_and_run("<?php $x = 10; $x %= 3; echo $x;");
    assert_eq!(out, "1");
}

#[test]
fn test_dot_assign() {
    // Verifies `.=` compound string concatenation.
    // Fixture: `$s = "hello"; $s .= " world";` expects output "hello world".
    let out = compile_and_run("<?php $s = \"hello\"; $s .= \" world\"; echo $s;");
    assert_eq!(out, "hello world");
}

#[test]
fn test_pow_assign() {
    // Verifies `**=` compound exponentiation on integer locals.
    // Fixture: `$x = 2; $x **= 3;` expects output "8".
    let out = compile_and_run("<?php $x = 2; $x **= 3; echo $x;");
    assert_eq!(out, "8");
}

#[test]
fn test_bitwise_compound_assignments() {
    // Verifies `&=`, `|=`, `^=`, `<<=`, `>>=` bitwise compound assignments on integer locals.
    // Fixture: sequential bitwise ops with comma-separated output "2,5,4,32,8".
    let out = compile_and_run(
        r#"<?php
$x = 6;
$x &= 3;
echo $x . ",";
$x = 4;
$x |= 1;
echo $x . ",";
$x = 7;
$x ^= 3;
echo $x . ",";
$x = 1;
$x <<= 5;
echo $x . ",";
$x = 64;
$x >>= 3;
echo $x;
"#,
    );
    assert_eq!(out, "2,5,4,32,8");
}

#[test]
fn test_assignment_expression_returns_assigned_value() {
    // Verifies simple assignment `$x = 5` is an expression returning the assigned value.
    // Fixture: `echo ($x = 5); echo ':'; echo $x;` expects "5:5".
    let out = compile_and_run("<?php echo ($x = 5); echo ':'; echo $x;");
    assert_eq!(out, "5:5");
}

#[test]
fn test_string_assignment_expression_returns_assigned_value() {
    // Verifies string assignment `$s = "hi"` is an expression returning the assigned value.
    // Fixture: `echo ($s = "hi"); echo ":"; echo $s;` expects "hi:hi".
    let out = compile_and_run(r#"<?php echo ($s = "hi"); echo ":"; echo $s;"#);
    assert_eq!(out, "hi:hi");
}

#[test]
fn test_assignment_expression_word_and_uses_php_precedence() {
    // Verifies assignment with `and` respects PHP operator precedence (lowest precedence).
    // Fixture: `$x = true and false;` parses as `$x = (true and false)`, so `$x` is `false`; echo "T"/"F" expects "T".
    let out = compile_and_run(
        r#"<?php
$x = true and false;
echo $x ? "T" : "F";
"#,
    );
    assert_eq!(out, "T");
}

#[test]
fn test_assignment_expression_in_condition_updates_local() {
    // Verifies assignment expression inside `if` condition updates the local variable.
    // Fixture: `if ($x = 3) { echo $x; }` expects output "3".
    let out = compile_and_run(
        r#"<?php
if ($x = 3) {
    echo $x;
}
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_compound_assignment_expression_returns_new_value() {
    // Verifies compound assignment `+=` returns the new value (not the original).
    // Fixture: `$x = 4; echo ($x += 3); echo ':'; echo $x;` expects "7:7".
    let out = compile_and_run("<?php $x = 4; echo ($x += 3); echo ':'; echo $x;");
    assert_eq!(out, "7:7");
}

#[test]
fn test_null_coalesce_assignment_expression_returns_existing_mixed_value() {
    // Verifies `??=` returns the existing non-null value when the variable already has one.
    // Fixture: `maybe(true)` returns 7 (non-null), assigned to `$x`; `($x ??= 5)` returns 7 without assigning.
    let out = compile_and_run(
        r#"<?php
function maybe(bool $flag): mixed {
    return $flag ? 7 : null;
}
$x = maybe(true);
echo ($x ??= 5);
echo ":";
echo $x;
"#,
    );
    assert_eq!(out, "7:7");
}

#[test]
fn test_null_coalesce_assignment_expression_returns_default_for_mixed_null() {
    // Verifies `??=` returns and assigns the default when the variable is null.
    // Fixture: `maybe(false)` returns null, `$x` is set to that null; `($x ??= 5)` assigns 5 and returns 5.
    let out = compile_and_run(
        r#"<?php
function maybe(bool $flag): mixed {
    return $flag ? 7 : null;
}
$x = maybe(false);
echo ($x ??= 5);
echo ":";
echo $x;
"#,
    );
    assert_eq!(out, "5:5");
}

#[test]
fn test_array_assignment_expression_returns_assigned_value() {
    // Verifies array-element assignment is an expression returning the assigned value.
    // Fixture: `$items = [1, 2]; echo ($items[1] = 9);` expects both echo and `$items[1]` to be 9.
    let out = compile_and_run("<?php $items = [1, 2]; echo ($items[1] = 9); echo ':' . $items[1];");
    assert_eq!(out, "9:9");
}

#[test]
fn test_array_assignment_expression_snapshots_rhs_container_before_write() {
    // Verifies array assignment evaluates the RHS container snapshot before writing to the destination.
    // Fixture: `$items = []; $result = ($items[0] = $items);` — both result and `$items[0]` receive the empty array (snapshot taken before write).
    let out = compile_and_run(
        r#"<?php
$items = [];
$result = ($items[0] = $items);
echo count($result) . ":" . count($items[0]);
"#,
    );
    assert_eq!(out, "0:0");
}

#[test]
fn test_array_assignment_expression_variable_index_returns_assigned_value() {
    // Verifies array-element assignment with a variable index returns the assigned value.
    // Fixture: `$items = [1, 2]; $i = 1; echo ($items[$i] = 9);` expects echo and `$items[1]` both 9.
    let out = compile_and_run("<?php $items = [1, 2]; $i = 1; echo ($items[$i] = 9); echo ':' . $items[1];");
    assert_eq!(out, "9:9");
}

#[test]
fn test_array_compound_assignment_expression_returns_new_value() {
    // Verifies compound assignment on an array element returns the new value.
    // Fixture: `$items = [3]; echo ($items[0] += 4); echo ':' . $items[0];` expects "7:7".
    let out = compile_and_run("<?php $items = [3]; echo ($items[0] += 4); echo ':' . $items[0];");
    assert_eq!(out, "7:7");
}

#[test]
fn test_assoc_array_assignment_expression_returns_assigned_value() {
    // Verifies associative array element assignment with compound `+=` returns the new value.
    // Fixture: `$items = ["count" => 2]; echo ($items["count"] += 5);` expects echo "7" and `$items["count"]` = 7.
    let out = compile_and_run(
        r#"<?php
$items = ["count" => 2];
echo ($items["count"] += 5);
echo ":" . $items["count"];
"#,
    );
    assert_eq!(out, "7:7");
}

#[test]
fn test_array_null_coalesce_assignment_expression_returns_slot_value() {
    // Verifies `??=` on a populated array element returns the existing value without reassigning.
    // Fixture: `$items = [5, 8];` — `($items[0] ??= 5)` returns 5, `($items[1] ??= 6)` returns 8, final state is unchanged.
    let out = compile_and_run(
        r#"<?php
$items = [5, 8];
echo ($items[0] ??= 5);
echo ":";
echo ($items[1] ??= 6);
echo ":" . $items[0] . ":" . $items[1];
"#,
    );
    assert_eq!(out, "5:8:5:8");
}

#[test]
fn test_array_null_coalesce_assignment_expression_snapshots_rhs_container_before_write() {
    // Verifies `??=` on an empty array element evaluates the RHS snapshot before write when the slot is absent.
    // Fixture: `$items = []; $result = ($items[0] ??= $items);` — both receive the empty array snapshot.
    let out = compile_and_run(
        r#"<?php
$items = [];
$result = ($items[0] ??= $items);
echo count($result) . ":" . count($items[0]);
"#,
    );
    assert_eq!(out, "0:0");
}

#[test]
fn test_property_assignment_expression_returns_assigned_value() {
    // Verifies object property assignment with compound `+=` returns the new value.
    // Fixture: `Box` with `$value = 1`; `$box->value += 4` returns and stores 5.
    let out = compile_and_run(
        r#"<?php
class Box {
    public $value = 1;
}
$box = new Box();
echo ($box->value += 4);
echo ":" . $box->value;
"#,
    );
    assert_eq!(out, "5:5");
}

#[test]
fn test_property_array_assignment_expression_returns_assigned_value() {
    // Verifies compound assignment on an object property array element returns the new value.
    // Fixture: `Box` with `$items = [2, 4]`; `$box->items[1] *= 3` returns and stores 12.
    let out = compile_and_run(
        r#"<?php
class Box {
    public $items = [2, 4];
}
$box = new Box();
echo ($box->items[1] *= 3);
echo ":" . $box->items[1];
"#,
    );
    assert_eq!(out, "12:12");
}

#[test]
fn test_property_array_assignment_expression_snapshots_rhs_container_before_write() {
    // Verifies object property array element assignment snapshots the RHS container before writing.
    // Fixture: `$box->items = []; $result = ($box->items[0] = $box->items);` — both get the empty array snapshot.
    let out = compile_and_run(
        r#"<?php
class Box {
    public $items = [];
}
$box = new Box();
$result = ($box->items[0] = $box->items);
echo count($result) . ":" . count($box->items[0]);
"#,
    );
    assert_eq!(out, "0:0");
}

#[test]
fn test_static_property_assignment_expression_returns_assigned_value() {
    // Verifies static property assignment with compound `+=` returns the new value.
    // Fixture: `Registry::$count = 10;` then `Registry::$count += 5` returns and stores 15.
    let out = compile_and_run(
        r#"<?php
class Registry {
    public static $count = 10;
}
echo (Registry::$count += 5);
echo ":" . Registry::$count;
"#,
    );
    assert_eq!(out, "15:15");
}

#[test]
fn test_static_property_array_assignment_expression_returns_assigned_value() {
    // Verifies compound assignment on a static property array element returns the new value.
    // Fixture: `Registry::$items = [3, 5];` then `Registry::$items[0] += 3` returns and stores 6.
    let out = compile_and_run(
        r#"<?php
class Registry {
    public static $items = [3, 5];
}
echo (Registry::$items[0] += 3);
echo ":" . Registry::$items[0];
"#,
    );
    assert_eq!(out, "6:6");
}

#[test]
fn test_static_property_array_assignment_expression_snapshots_rhs_container_before_write() {
    // Verifies static property array element assignment snapshots the RHS container before writing.
    // Fixture: `Registry::$items = []; $result = (Registry::$items[0] = Registry::$items);` — both get empty array snapshot.
    let out = compile_and_run(
        r#"<?php
class Registry {
    public static $items = [];
}
$result = (Registry::$items[0] = Registry::$items);
echo count($result) . ":" . count(Registry::$items[0]);
"#,
    );
    assert_eq!(out, "0:0");
}

#[test]
fn test_static_property_null_coalesce_assignment_expression_returns_value() {
    // Verifies `??=` on a null static property returns and assigns the default value.
    // Fixture: `Registry::$value = null;` then `Registry::$value ??= 6` assigns and returns 6.
    let out = compile_and_run(
        r#"<?php
class Registry {
    public static ?int $value = null;
}
echo (Registry::$value ??= 6);
echo ":" . Registry::$value;
"#,
    );
    assert_eq!(out, "6:6");
}

#[test]
fn test_chained_three_level_local_assignment() {
    // Verifies three-level chained local assignment `$a = $b = $c = 5` assigns to all three variables.
    // Fixture: `$a = $b = $c = 5; echo $a + $b + $c;` expects "15".
    let out = compile_and_run("<?php $a = $b = $c = 5; echo $a + $b + $c;");
    assert_eq!(out, "15");
}

#[test]
fn test_chained_string_local_assignment() {
    // Verifies three-level chained local assignment with strings.
    // Fixture: `$a = $b = "hi"; echo $a . $b;` expects "hihi".
    let out = compile_and_run(r#"<?php $a = $b = "hi"; echo $a . $b;"#);
    assert_eq!(out, "hihi");
}

#[test]
fn test_chained_static_property_and_local_assignment() {
    // Verifies chained assignment to a static property and a local variable in a single expression.
    // Regression test: static property assignment must not consume the right-hand-side local incorrectly.
    // Fixture: `self::$x = $local = 42` inside a static method, result is `self::$x + $local` = 42 + 42 = 84.
    let out = compile_and_run(
        r#"<?php
class C {
    public static int $x = 0;
    public static function init(): int {
        self::$x = $local = 42;
        return self::$x + $local;
    }
}
echo C::init();
"#,
    );
    assert_eq!(out, "84");
}
