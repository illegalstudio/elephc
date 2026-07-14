//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of callables state and variadics, including global read, global write, and global read write.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

// --- Global variables ---

/// Verifies that a `global $var` declaration inside a function reads the correct global value.
#[test]
fn test_global_read() {
    let out = compile_and_run(
        r#"<?php
$x = 10;
function test() {
    global $x;
    echo $x;
}
test();
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies that a `global $var` declaration inside a function can write to a global variable.
#[test]
fn test_global_write() {
    let out = compile_and_run(
        r#"<?php
$y = 5;
function modify() {
    global $y;
    $y = 99;
}
modify();
echo $y;
"#,
    );
    assert_eq!(out, "99");
}

/// Verifies that a `global $var` declaration allows both reading and writing the global variable.
#[test]
fn test_global_read_write() {
    let out = compile_and_run(
        r#"<?php
$x = 10;
function test() {
    global $x;
    echo $x;
    $x = 20;
}
test();
echo $x;
"#,
    );
    assert_eq!(out, "1020");
}

/// Verifies that multiple comma-separated global variables can be declared in one statement.
#[test]
fn test_global_multiple_vars() {
    let out = compile_and_run(
        r#"<?php
$a = 1;
$b = 2;
function sum() {
    global $a, $b;
    echo $a + $b;
}
sum();
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies that global variables persist and are correctly mutated across multiple function calls.
#[test]
fn test_global_increment() {
    let out = compile_and_run(
        r#"<?php
$counter = 0;
function inc() {
    global $counter;
    $counter++;
}
inc();
inc();
inc();
echo $counter;
"#,
    );
    assert_eq!(out, "3");
}

// --- Static variables ---

/// Verifies that a static variable inside a function increments across multiple invocations.
#[test]
fn test_static_counter() {
    let out = compile_and_run(
        r#"<?php
function counter() {
    static $n = 0;
    $n++;
    echo $n;
}
counter();
counter();
counter();
"#,
    );
    assert_eq!(out, "123");
}

/// Verifies that a static variable inside a closure links and persists across calls.
#[test]
fn test_closure_static_local_preserves_value_across_calls() {
    let out = compile_and_run(
        r#"<?php
$f = function () {
    static $x = 0;
    echo ++$x;
};
$f();
$f();
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies that a static variable's value is preserved and updated correctly across calls.
#[test]
fn test_static_preserves_value() {
    let out = compile_and_run(
        r#"<?php
function acc() {
    static $total = 0;
    $total = $total + 10;
    return $total;
}
echo acc();
echo acc();
echo acc();
"#,
    );
    assert_eq!(out, "102030");
}

/// Verifies that two functions can each declare a static variable with the same name without interference.
#[test]
fn test_static_separate_functions() {
    let out = compile_and_run(
        r#"<?php
function a() {
    static $x = 0;
    $x++;
    echo $x;
}
function b() {
    static $x = 0;
    $x = $x + 10;
    echo $x;
}
a();
b();
a();
b();
"#,
    );
    assert_eq!(out, "110220");
}

/// Regression test for null-initialized static persistence: `static $c = null;` guarded by
/// `if ($c === null) { $c = [...]; }` must build the array once and persist mutations across
/// calls. This pattern previously reset the static every call (`1 1 1`) because the null-sentinel
/// initializer sized the slot as void.
#[test]
fn test_static_null_guard_array_persists() {
    let out = compile_and_run(
        r#"<?php
function container() {
    static $c = null;
    if ($c === null) {
        $c = ['calls' => 0];
    }
    $c['calls']++;
    echo $c['calls'], " ";
}
container();
container();
container();
"#,
    );
    assert_eq!(out, "1 2 3 ");
}

/// Regression test for null-initialized static persistence: a `static $s = null;` string that is
/// initialized once then appended to must accumulate across calls rather than reset each time.
#[test]
fn test_static_null_guard_string_grows() {
    let out = compile_and_run(
        r#"<?php
function grow() {
    static $s = null;
    if ($s === null) {
        $s = "x";
    } else {
        $s = $s . "x";
    }
    echo $s, " ";
}
grow();
grow();
grow();
"#,
    );
    assert_eq!(out, "x xx xxx ");
}

/// Regression test for null-initialized static persistence: a `static $v = null;` int memoized
/// once then incremented must persist across calls. This pattern was previously a hard compile
/// error, so the test also guards against that regression.
#[test]
fn test_static_null_guard_int_memo() {
    let out = compile_and_run(
        r#"<?php
function memo() {
    static $v = null;
    if ($v === null) {
        $v = 41;
    }
    $v++;
    echo $v, " ";
}
memo();
memo();
memo();
"#,
    );
    assert_eq!(out, "42 43 44 ");
}

/// Regression test for null-initialized static persistence: a `static $f = null;` float seeded
/// once then accumulated with `+=` must persist its running total across calls.
#[test]
fn test_static_null_guard_float_accumulates() {
    let out = compile_and_run(
        r#"<?php
function fx() {
    static $f = null;
    if ($f === null) {
        $f = 1.5;
    }
    $f += 0.5;
    echo $f, " ";
}
fx();
fx();
fx();
"#,
    );
    assert_eq!(out, "2 2.5 3 ");
}

/// Regression test for null-initialized static persistence: a `static $o = null;` object created
/// once then mutated must persist the same instance (and its property state) across calls.
#[test]
fn test_static_null_guard_object_persists() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $n = 0;
}
function b() {
    static $o = null;
    if ($o === null) {
        $o = new Box();
    }
    $o->n++;
    echo $o->n, " ";
}
b();
b();
b();
"#,
    );
    assert_eq!(out, "1 2 3 ");
}

// --- Pass by reference ---

/// Verifies that a `&$var` parameter increments the caller's variable in place.
#[test]
fn test_ref_increment() {
    let out = compile_and_run(
        r#"<?php
function increment(&$val) {
    $val++;
}
$x = 5;
increment($x);
echo $x;
"#,
    );
    assert_eq!(out, "6");
}

/// Verifies that a `&$var` parameter can be assigned a new value and the caller sees the change.
#[test]
fn test_ref_assign() {
    let out = compile_and_run(
        r#"<?php
function set_value(&$v, $new_val) {
    $v = $new_val;
}
$x = 1;
set_value($x, 42);
echo $x;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies direct reference assignment aliases reads from the source variable.
#[test]
fn test_reference_assignment_alias_reads_source() {
    let out = compile_and_run(
        r#"<?php
$a = 1;
$b =& $a;
echo $b;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies writes through a directly aliased variable update the original source.
#[test]
fn test_reference_assignment_alias_writes_through() {
    let out = compile_and_run(
        r#"<?php
$a = 1;
$b =& $a;
$b = 42;
echo $a;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies writes to the original source remain visible through the alias.
#[test]
fn test_reference_assignment_source_write_visible_through_alias() {
    let out = compile_and_run(
        r#"<?php
$a = 1;
$b =& $a;
$a = 2;
echo $b;
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies that a two-argument `&$a, &$b` swap function correctly swaps the caller's values.
#[test]
fn test_ref_swap() {
    let out = compile_and_run(
        r#"<?php
function swap(&$a, &$b) {
    $tmp = $a;
    $a = $b;
    $b = $tmp;
}
$p = 1;
$q = 2;
swap($p, $q);
echo $p . $q;
"#,
    );
    assert_eq!(out, "21");
}

/// Verifies that a `&$target` parameter with a regular by-value parameter works correctly.
#[test]
fn test_ref_mixed_params() {
    let out = compile_and_run(
        r#"<?php
function add_to(&$target, $amount) {
    $target = $target + $amount;
}
$x = 10;
add_to($x, 5);
echo $x;
"#,
    );
    assert_eq!(out, "15");
}

/// Verifies by-reference variadic function and method element assignments mutate caller variables.
#[test]
fn test_by_ref_variadic_function_and_method_element_writeback() {
    let out = compile_and_run(
        r#"<?php
function f(&...$items) {
    $items[0] = $items[0] . "-f";
    $items[1] = $items[1] . "-g";
}
class C {
    public function m(&...$items) {
        $items[0] = $items[0] . "-m";
        $items[1] = $items[1] . "-n";
    }
}
$a = "A";
$b = "B";
f($a, $b);
echo $a . ":" . $b . "|";
$c = "C";
$d = "D";
(new C())->m($c, $d);
echo $c . ":" . $d;
"#,
    );
    assert_eq!(out, "A-f:B-g|C-m:D-n");
}

// --- Variadic functions ---

/// Verifies a variadic function collects exactly three positional arguments into the rest array.
#[test]
fn test_variadic_sum() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum(1, 2, 3);
"#,
    );
    assert_eq!(out, "6");
}

/// Verifies a variadic function collects exactly five positional arguments into the rest array.
#[test]
fn test_variadic_five_args() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum(1, 2, 3, 4, 5);
"#,
    );
    assert_eq!(out, "15");
}

/// Verifies that a variadic function can be called multiple times with different argument counts without interference.
#[test]
fn test_variadic_multiple_calls_same_function() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum(1, 2, 3);
echo ":";
echo sum(10, 20, 30, 40, 50);
"#,
    );
    assert_eq!(out, "6:150");
}

/// Verifies that a variadic function called with no arguments receives an empty rest array.
#[test]
fn test_variadic_empty() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum();
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies that a variadic parameter follows regular positional parameters and collects remaining arguments.
#[test]
fn test_variadic_with_regular_params() {
    let out = compile_and_run(
        r#"<?php
function greet($greeting, ...$names) {
    foreach ($names as $name) {
        echo $greeting . " " . $name . "\n";
    }
}
greet("Hello", "Alice", "Bob");
"#,
    );
    assert_eq!(out, "Hello Alice\nHello Bob\n");
}

/// Verifies that `count()` works correctly on a variadic rest array with four elements.
#[test]
fn test_variadic_count() {
    let out = compile_and_run(
        r#"<?php
function num_args(...$args) {
    return count($args);
}
echo num_args(10, 20, 30, 40);
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies that a variadic function returning the rest array allows accessing the single wrapped element.
#[test]
fn test_variadic_single_arg() {
    let out = compile_and_run(
        r#"<?php
function wrap(...$items) {
    return $items;
}
$arr = wrap(42);
echo $arr[0];
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that a nested array passed to a variadic function preserves its element tag through json_encode.
#[test]
fn test_variadic_array_arg_preserves_runtime_element_tag() {
    let out = compile_and_run(
        r#"<?php
function wrap(...$items) {
    echo json_encode($items);
}
wrap([1, 2]);
"#,
    );
    assert_eq!(out, "[[1,2]]");
}

// --- Spread operator ---

/// Verifies that an array spread `...$args` in a function call unpacks correctly into a variadic callee.
#[test]
fn test_spread_in_function_call() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
$args = [10, 20, 30];
echo sum(...$args);
"#,
    );
    assert_eq!(out, "60");
}

/// Verifies that an array spread into a function with regular and variadic params fills regular params first and collects the remainder into the rest array.
#[test]
fn test_spread_in_variadic_function_fills_regular_params_first() {
    let out = compile_and_run(
        r#"<?php
function show($head, ...$rest) {
    echo "head=" . $head . ";";
    foreach ($rest as $value) {
        echo $value . ";";
    }
}
show(...[1, 2, 3]);
"#,
    );
    assert_eq!(out, "head=1;2;3;");
}

/// Verifies that two spread arrays in an array literal `[...$a, ...$b]` produce a flattened array of four elements.
#[test]
fn test_spread_in_array_literal() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [3, 4];
$c = [...$a, ...$b];
echo count($c);
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies that two spread arrays in an array literal produce a flattened array whose elements iterate in correct order.
#[test]
fn test_spread_array_values() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [3, 4];
$c = [...$a, ...$b];
foreach ($c as $v) {
    echo $v;
}
"#,
    );
    assert_eq!(out, "1234");
}

/// Verifies that array spreads can be interleaved with literal elements in an array literal.
#[test]
fn test_spread_mixed_with_elements() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [5, 6];
$c = [...$a, 3, 4, ...$b];
echo count($c);
echo " ";
foreach ($c as $v) {
    echo $v;
}
"#,
    );
    assert_eq!(out, "6 123456");
}

/// Verifies that a single-array spread `[...$a]` produces an array equal in length to the source.
#[test]
fn test_spread_single_source() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
$c = [...$a];
echo count($c);
"#,
    );
    assert_eq!(out, "3");
}

/// Regression for #354: spread of an associative array into a new array literal flattens its
/// string-keyed entries instead of inserting the source as a single nested value.
#[test]
fn test_spread_assoc_array() {
    let out = compile_and_run(r#"<?php
$a = ['x' => 1];
$b = [...$a];
foreach ($b as $k => $v) { echo '[' . $k . ':' . $v . ']'; }
"#);
    assert_eq!(out, "[x:1]");
}

/// Regression for #354: spread reindexes integer-keyed source entries to fresh sequential keys
/// (matching PHP) while preserving string keys.
#[test]
fn test_spread_mixed_keys() {
    let out = compile_and_run(r#"<?php
$a = [10 => 'a', 'x' => 'b'];
$b = [...$a];
foreach ($b as $k => $v) { echo '[' . $k . ':' . $v . ']'; }
"#);
    assert_eq!(out, "[0:a][x:b]");
}

/// Regression for #354: later spread operands overwrite earlier ones on string-key collision.
#[test]
fn test_spread_overwrite() {
    let out = compile_and_run(r#"<?php
$a = ['x' => 1, 'y' => 2];
$b = ['y' => 3, 'z' => 4];
$c = [...$a, ...$b];
foreach ($c as $k => $v) { echo '[' . $k . ':' . $v . ']'; }
"#);
    assert_eq!(out, "[x:1][y:3][z:4]");
}

/// Regression for #354: spread of an indexed array into a new array literal stays on the indexed
/// storage path and preserves sequential integer keys.
#[test]
fn test_spread_indexed_array() {
    let out = compile_and_run(r#"<?php
$a = [1, 2, 3];
$b = [...$a];
foreach ($b as $k => $v) { echo '[' . $k . ':' . $v . ']'; }
"#);
    assert_eq!(out, "[0:1][1:2][2:3]");
}

/// Regression for #354: a literal element before and after an associative spread continues the
/// automatic integer key sequence across the reindexed spread entries.
#[test]
fn test_spread_literal_interleaved_with_assoc() {
    let out = compile_and_run(r#"<?php
$a = ['y' => 1];
$b = ['w', ...$a, 'x'];
foreach ($b as $k => $v) { echo '[' . $k . ':' . $v . ']'; }
"#);
    assert_eq!(out, "[0:w][y:1][1:x]");
}

/// Regression for #354: an indexed spread followed by an associative spread continues the
/// reindex counter across operands.
#[test]
fn test_spread_indexed_then_assoc() {
    let out = compile_and_run(r#"<?php
$a = [10, 20];
$b = ['x' => 1];
$c = [...$a, ...$b];
foreach ($c as $k => $v) { echo '[' . $k . ':' . $v . ']'; }
"#);
    assert_eq!(out, "[0:10][1:20][x:1]");
}

/// Verifies that a variadic function with a preceding regular parameter receives zero rest elements when called with exactly one argument.
#[test]
fn test_variadic_with_regular_and_no_extra() {
    let out = compile_and_run(
        r#"<?php
function prefix($pre, ...$items) {
    echo count($items);
}
prefix("x");
"#,
    );
    assert_eq!(out, "0");
}

// --- Typed variadics ---

/// Verifies a typed `int ...$nums` free-function variadic sums its arguments; the element type
/// is inferred from the passed integers.
#[test]
fn test_typed_variadic_int_sum() {
    let out = compile_and_run(
        r#"<?php
function sum(int ...$nums): int { return array_sum($nums); }
echo sum(1, 2, 3, 4);
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies a typed `string ...$parts` variadic joins its string arguments.
#[test]
fn test_typed_variadic_string_join() {
    let out = compile_and_run(
        r#"<?php
function join_em(string ...$parts): string { return implode("-", $parts); }
echo join_em("a", "b", "c");
"#,
    );
    assert_eq!(out, "a-b-c");
}

/// Verifies a typed variadic following a regular parameter collects only the trailing arguments.
#[test]
fn test_typed_variadic_after_regular_param() {
    let out = compile_and_run(
        r#"<?php
function tag(string $t, string ...$items): string { return $t . ":" . implode(",", $items); }
echo tag("x", "a", "b");
"#,
    );
    assert_eq!(out, "x:a,b");
}

/// Verifies a typed variadic accepts zero trailing arguments.
#[test]
fn test_typed_variadic_empty() {
    let out = compile_and_run(
        r#"<?php
function sum(int ...$nums): int { return array_sum($nums); }
echo sum();
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies a typed variadic on an instance method collects its arguments (counted to avoid the
/// pre-existing array_sum-over-mixed-array backend gap that affects all method/closure variadics).
#[test]
fn test_typed_variadic_method() {
    let out = compile_and_run(
        r#"<?php
class Calc {
    public function count_args(int ...$ns): int { return count($ns); }
}
echo (new Calc())->count_args(10, 20, 30);
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies a typed variadic on a closure collects its arguments.
#[test]
fn test_typed_variadic_closure() {
    let out = compile_and_run(
        r#"<?php
$f = function (int ...$xs): int { return count($xs); };
echo $f(5, 6, 7);
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies that array unpacking into a typed variadic works (`sum(...$a)`).
#[test]
fn test_typed_variadic_spread_argument() {
    let out = compile_and_run(
        r#"<?php
function sum(int ...$n): int { return array_sum($n); }
$a = [1, 2, 3];
echo sum(...$a);
"#,
    );
    assert_eq!(out, "6");
}

/// Verifies that a typed variadic collects correctly-typed positional arguments and runs
/// end-to-end, confirming the declared element type does not interfere with valid calls.
#[test]
fn test_typed_variadic_positional_arguments_run() {
    let out = compile_and_run(
        r#"<?php
function sum(int ...$n): int { return array_sum($n); }
echo sum(4, 5, 6);
"#,
    );
    assert_eq!(out, "15");
}

// --- First-class callables over registry builtins with variadic/optional signatures ---

/// Verifies `var_dump(...)` as a first-class callable exposes the registry's variadic
/// signature (`value, ...values`): the wrapper accepts multiple arguments and dumps
/// each independently in source order.
#[test]
fn test_first_class_callable_var_dump_variadic() {
    let out = compile_and_run(
        r#"<?php
$dump = var_dump(...);
$dump(1, "a");
"#,
    );
    assert_eq!(out, "int(1)\nstring(1) \"a\"\n");
}

/// Verifies `print_r(...)` as a first-class callable exposes the optional `$return`
/// flag from the registry signature. Through the wrapper the flag is a runtime
/// parameter, so the call takes the runtime-selected mode path and returns the
/// rendered string (boxed Mixed) without echoing.
#[test]
fn test_first_class_callable_print_r_return_flag() {
    let out = compile_and_run(
        r#"<?php
$render = print_r(...);
$r = $render("hi", true);
echo "|$r";
"#,
    );
    assert_eq!(out, "|hi");
}

/// Verifies `print_r(...)` as a first-class callable defaults the `$return` flag to
/// `false` when called with one argument: the value is echoed and the wrapper
/// returns true.
#[test]
fn test_first_class_callable_print_r_echo_default() {
    let out = compile_and_run(
        r#"<?php
$render = print_r(...);
$ok = $render(42);
echo "|";
echo $ok ? "yes" : "no";
"#,
    );
    assert_eq!(out, "42|yes");
}
