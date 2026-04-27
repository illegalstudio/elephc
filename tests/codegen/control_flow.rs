use crate::support::*;

// --- if/else ---

#[test]
fn test_if_true() {
    let out = compile_and_run("<?php if (1 == 1) { echo \"yes\"; }");
    assert_eq!(out, "yes");
}

#[test]
fn test_if_false() {
    let out = compile_and_run("<?php if (1 == 2) { echo \"yes\"; }");
    assert_eq!(out, "");
}

#[test]
fn test_if_else() {
    let out = compile_and_run("<?php if (1 == 2) { echo \"a\"; } else { echo \"b\"; }");
    assert_eq!(out, "b");
}

#[test]
fn test_if_elseif_else() {
    let out = compile_and_run(
        "<?php $x = 2; if ($x == 1) { echo \"one\"; } elseif ($x == 2) { echo \"two\"; } else { echo \"other\"; }",
    );
    assert_eq!(out, "two");
}

#[test]
fn test_if_else_falls_through() {
    let out = compile_and_run(
        "<?php $x = 99; if ($x == 1) { echo \"a\"; } elseif ($x == 2) { echo \"b\"; } else { echo \"c\"; }",
    );
    assert_eq!(out, "c");
}

// --- while ---

#[test]
fn test_while_loop() {
    let out = compile_and_run("<?php $i = 0; while ($i < 5) { echo $i; $i = $i + 1; }");
    assert_eq!(out, "01234");
}

#[test]
fn test_while_zero_iterations() {
    let out = compile_and_run("<?php while (0) { echo \"no\"; }");
    assert_eq!(out, "");
}

#[test]
fn test_while_break() {
    let out = compile_and_run(
        "<?php $i = 0; while ($i < 10) { if ($i == 3) { break; } echo $i; $i = $i + 1; }",
    );
    assert_eq!(out, "012");
}

#[test]
fn test_while_continue() {
    let out = compile_and_run(
        "<?php $i = 0; while ($i < 5) { $i = $i + 1; if ($i == 3) { continue; } echo $i; }",
    );
    assert_eq!(out, "1245");
}

// --- for ---

#[test]
fn test_for_loop() {
    let out = compile_and_run("<?php for ($i = 0; $i < 5; $i = $i + 1) { echo $i; }");
    assert_eq!(out, "01234");
}

#[test]
fn test_for_break() {
    let out = compile_and_run(
        "<?php for ($i = 0; $i < 10; $i = $i + 1) { if ($i == 3) { break; } echo $i; }",
    );
    assert_eq!(out, "012");
}

// --- FizzBuzz ---

#[test]
fn test_fizzbuzz() {
    let source = r#"<?php
$i = 1;
while ($i <= 15) {
    if ($i % 15 == 0) {
        echo "FizzBuzz\n";
    } elseif ($i % 3 == 0) {
        echo "Fizz\n";
    } elseif ($i % 5 == 0) {
        echo "Buzz\n";
    } else {
        echo $i;
        echo "\n";
    }
    $i = $i + 1;
}
"#;
    let out = compile_and_run(source);
    assert_eq!(
        out,
        "1\n2\nFizz\n4\nBuzz\nFizz\n7\n8\nFizz\nBuzz\n11\nFizz\n13\n14\nFizzBuzz\n"
    );
}

// --- Increment/Decrement ---

#[test]
fn test_pre_increment() {
    let out = compile_and_run("<?php $i = 1; $k = ++$i; echo $i . \" \" . $k;");
    assert_eq!(out, "2 2");
}

#[test]
fn test_post_increment() {
    let out = compile_and_run("<?php $i = 1; $k = $i++; echo $i . \" \" . $k;");
    assert_eq!(out, "2 1");
}

#[test]
fn test_pre_decrement() {
    let out = compile_and_run("<?php $i = 5; $k = --$i; echo $i . \" \" . $k;");
    assert_eq!(out, "4 4");
}

#[test]
fn test_post_decrement() {
    let out = compile_and_run("<?php $i = 5; $k = $i--; echo $i . \" \" . $k;");
    assert_eq!(out, "4 5");
}

#[test]
fn test_standalone_increment() {
    let out = compile_and_run("<?php $x = 0; $x++; $x++; $x++; echo $x;");
    assert_eq!(out, "3");
}

#[test]
fn test_standalone_decrement() {
    let out = compile_and_run("<?php $x = 10; $x--; $x--; echo $x;");
    assert_eq!(out, "8");
}

#[test]
fn test_for_with_increment() {
    let out = compile_and_run("<?php for ($i = 0; $i < 5; $i++) { echo $i; }");
    assert_eq!(out, "01234");
}

#[test]
fn test_while_with_pre_increment() {
    let out = compile_and_run("<?php $i = 0; while ($i < 3) { ++$i; echo $i; }");
    assert_eq!(out, "123");
}

// --- Functions ---

#[test]
fn test_function_call_int() {
    let out = compile_and_run("<?php function add($a, $b) { return $a + $b; } echo add(10, 32);");
    assert_eq!(out, "42");
}

#[test]
fn test_function_call_string() {
    let out = compile_and_run(
        "<?php function greet($name) { return \"Hello, \" . $name; } echo greet(\"World\");",
    );
    assert_eq!(out, "Hello, World");
}

#[test]
fn test_function_void() {
    let out = compile_and_run("<?php function say() { echo \"hi\"; return; } say();");
    assert_eq!(out, "hi");
}

#[test]
fn test_function_local_scope() {
    let out = compile_and_run(
        "<?php $x = 1; function get_two() { $x = 2; return $x; } echo $x . \" \" . get_two();",
    );
    assert_eq!(out, "1 2");
}

#[test]
fn test_function_recursive() {
    let out = compile_and_run(
        "<?php function fact($n) { if ($n <= 1) { return 1; } return $n * fact($n - 1); } echo fact(5);",
    );
    assert_eq!(out, "120");
}

#[test]
fn test_function_multiple_calls() {
    let out = compile_and_run(
        "<?php function double($x) { return $x * 2; } echo double(3) . \" \" . double(7);",
    );
    assert_eq!(out, "6 14");
}

#[test]
fn test_function_as_argument() {
    let out = compile_and_run(
        "<?php function add($a, $b) { return $a + $b; } echo add(add(1, 2), add(3, 4));",
    );
    assert_eq!(out, "10");
}

#[test]
fn test_function_no_args() {
    let out = compile_and_run("<?php function answer() { return 42; } echo answer();");
    assert_eq!(out, "42");
}

// --- Logical operators ---

#[test]
fn test_and_true() {
    let out = compile_and_run("<?php echo 1 && 1;");
    assert_eq!(out, "1");
}

#[test]
fn test_and_false() {
    let out = compile_and_run("<?php echo 1 && 0;");
    assert_eq!(out, "");
}

#[test]
fn test_or_true() {
    let out = compile_and_run("<?php echo 0 || 1;");
    assert_eq!(out, "1");
}

#[test]
fn test_or_false() {
    let out = compile_and_run("<?php echo 0 || 0;");
    assert_eq!(out, "");
}

#[test]
fn test_not_zero() {
    let out = compile_and_run("<?php $x = 0; echo !$x;");
    assert_eq!(out, "1");
}

#[test]
fn test_not_nonzero() {
    let out = compile_and_run("<?php $x = 42; echo !$x;");
    assert_eq!(out, "");
}

#[test]
fn test_short_circuit_and() {
    let out = compile_and_run(
        r#"<?php
$count = 0;
function inc() { return 1; }
$r = 0 && inc();
echo $r;
"#,
    );
    assert_eq!(out, ""); // false prints nothing
}

#[test]
fn test_short_circuit_or() {
    // With ||, if left is true the right side should not be evaluated.
    let out = compile_and_run(
        r#"<?php
function inc() { return 1; }
$r = 1 || inc();
echo $r;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_word_and_short_circuit() {
    let out = compile_and_run(
        r#"<?php
function mark() { echo "rhs"; return true; }
$r = (false and mark());
echo $r ? "T" : "F";
"#,
    );
    assert_eq!(out, "F");
}

#[test]
fn test_word_or_short_circuit() {
    let out = compile_and_run(
        r#"<?php
function mark() { echo "rhs"; return false; }
$r = (true or mark());
echo $r ? "T" : "F";
"#,
    );
    assert_eq!(out, "T");
}

#[test]
fn test_word_xor_evaluates_rhs() {
    let out = compile_and_run(
        r#"<?php
function mark() { echo "rhs"; return false; }
$r = (true xor mark());
echo $r ? "T" : "F";
"#,
    );
    assert_eq!(out, "rhsT");
}

#[test]
fn test_word_logical_operators_are_case_insensitive_codegen() {
    let out = compile_and_run(
        r#"<?php
echo (true AND false) ? "T" : "F";
echo (false Or true) ? "T" : "F";
echo (true xOr false) ? "T" : "F";
"#,
    );
    assert_eq!(out, "FTT");
}

#[test]
fn test_word_logical_precedence_against_symbolic_logical() {
    let out = compile_and_run(
        r#"<?php
$a = (true || false and false);
echo $a ? "T" : "F";
$b = (false && true or true);
echo $b ? "T" : "F";
$c = (true xor true and false);
echo $c ? "T" : "F";
"#,
    );
    assert_eq!(out, "FTT");
}

#[test]
fn test_boolean_true() {
    let out = compile_and_run("<?php echo true;");
    assert_eq!(out, "1");
}

#[test]
fn test_boolean_false() {
    let out = compile_and_run("<?php echo false;");
    assert_eq!(out, "");
}

#[test]
fn test_boolean_in_condition() {
    let out = compile_and_run("<?php if (true) { echo \"yes\"; } if (false) { echo \"no\"; }");
    assert_eq!(out, "yes");
}

// --- Assignment operators ---

#[test]
fn test_plus_assign() {
    let out = compile_and_run("<?php $x = 10; $x += 5; echo $x;");
    assert_eq!(out, "15");
}

#[test]
fn test_minus_assign() {
    let out = compile_and_run("<?php $x = 10; $x -= 3; echo $x;");
    assert_eq!(out, "7");
}

#[test]
fn test_star_assign() {
    let out = compile_and_run("<?php $x = 6; $x *= 7; echo $x;");
    assert_eq!(out, "42");
}

#[test]
fn test_slash_assign() {
    let out = compile_and_run("<?php $x = 84; $x /= 2; echo $x;");
    assert_eq!(out, "42");
}

#[test]
fn test_percent_assign() {
    let out = compile_and_run("<?php $x = 10; $x %= 3; echo $x;");
    assert_eq!(out, "1");
}

#[test]
fn test_dot_assign() {
    let out = compile_and_run("<?php $s = \"hello\"; $s .= \" world\"; echo $s;");
    assert_eq!(out, "hello world");
}

#[test]
fn test_pow_assign() {
    let out = compile_and_run("<?php $x = 2; $x **= 3; echo $x;");
    assert_eq!(out, "8");
}

#[test]
fn test_bitwise_compound_assignments() {
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
fn test_logical_with_comparison() {
    let out = compile_and_run("<?php $x = 5; echo ($x > 3 && $x < 10);");
    assert_eq!(out, "1");
}

// --- Logical operators with null ---

#[test]
fn test_null_and_true() {
    // null && true → false (null coerces to false)
    let out = compile_and_run("<?php echo null && true;");
    assert_eq!(out, "");
}

#[test]
fn test_true_and_null() {
    let out = compile_and_run("<?php echo true && null;");
    assert_eq!(out, "");
}

#[test]
fn test_null_or_false() {
    // null || false → false
    let out = compile_and_run("<?php echo null || false;");
    assert_eq!(out, "");
}

#[test]
fn test_false_or_null() {
    let out = compile_and_run("<?php echo false || null;");
    assert_eq!(out, "");
}

#[test]
fn test_null_or_true() {
    // null || true → true
    let out = compile_and_run("<?php echo null || true;");
    assert_eq!(out, "1");
}

#[test]
fn test_null_and_false() {
    let out = compile_and_run("<?php echo null && false;");
    assert_eq!(out, "");
}

#[test]
fn test_null_var_and() {
    let out = compile_and_run("<?php $x = null; echo $x && true;");
    assert_eq!(out, "");
}

#[test]
fn test_null_var_or() {
    let out = compile_and_run("<?php $x = null; echo $x || false;");
    assert_eq!(out, "");
}

#[test]
fn test_not_null_is_true() {
    // !null → true
    let out = compile_and_run("<?php $x = null; echo !$x;");
    assert_eq!(out, "1");
}

#[test]
fn test_if_null_is_falsy() {
    let out = compile_and_run(
        r#"<?php
$x = null;
if ($x) {
    echo "true";
} else {
    echo "false";
}
"#,
    );
    assert_eq!(out, "false");
}

#[test]
fn test_ternary_null_is_falsy() {
    let out = compile_and_run("<?php $x = null; echo $x ? \"yes\" : \"no\";");
    assert_eq!(out, "no");
}

#[test]
fn test_while_null_no_loop() {
    let out = compile_and_run("<?php $x = null; while ($x) { echo \"bad\"; } echo \"ok\";");
    assert_eq!(out, "ok");
}

// --- Ternary operator ---

#[test]
fn test_ternary_true() {
    let out = compile_and_run("<?php echo 1 == 1 ? \"yes\" : \"no\";");
    assert_eq!(out, "yes");
}

#[test]
fn test_ternary_false() {
    let out = compile_and_run("<?php echo 1 == 2 ? \"yes\" : \"no\";");
    assert_eq!(out, "no");
}

#[test]
fn test_ternary_int() {
    let out = compile_and_run("<?php $x = 3; $y = 7; echo $x > $y ? $x : $y;");
    assert_eq!(out, "7");
}

#[test]
fn test_ternary_in_assignment() {
    let out = compile_and_run("<?php $a = 10; $b = 20; $max = $a > $b ? $a : $b; echo $max;");
    assert_eq!(out, "20");
}

#[test]
fn test_ternary_mixed_types_str_vs_int() {
    let out = compile_and_run(
        "<?php $a = [1]; array_pop($a); $v = array_pop($a); echo is_null($v) ? \"null\" : \"has value\";",
    );
    assert_eq!(out, "null");
}

#[test]
fn test_ternary_mixed_types_then_branch_str() {
    let out = compile_and_run("<?php $x = 0; echo $x ? \"yes\" : \"no\";");
    assert_eq!(out, "no");
}

#[test]
fn test_ternary_int_string() {
    let out = compile_and_run(
        r#"<?php
$x = true;
echo $x ? 42 : "none";
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_ternary_string_int() {
    let out = compile_and_run(
        r#"<?php
$x = false;
echo $x ? "yes" : 0;
"#,
    );
    assert_eq!(out, "0");
}

#[test]
fn test_ternary_string_string() {
    let out = compile_and_run(
        r#"<?php
$x = true;
echo $x ? "hello" : "world";
"#,
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_ternary_int_int() {
    let out = compile_and_run(
        r#"<?php
$x = true;
echo $x ? 1 : 0;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_ternary_mixed_in_concat() {
    let out = compile_and_run(
        r#"<?php
$count = 5;
echo "Items: " . ($count > 0 ? $count : "none");
"#,
    );
    assert_eq!(out, "Items: 5");
}

#[test]
fn test_ternary_float_string() {
    let out = compile_and_run(
        r#"<?php
$x = false;
echo $x ? 3.14 : "zero";
"#,
    );
    assert_eq!(out, "zero");
}

#[test]
fn test_ternary_nested_mixed() {
    let out = compile_and_run(
        r#"<?php
$a = 0;
echo $a ? "yes" : ($a === 0 ? "zero" : "no");
"#,
    );
    assert_eq!(out, "zero");
}

#[test]
fn test_ternary_variable_string() {
    let out = compile_and_run(
        r#"<?php
$name = "Alice";
$greeting = true ? $name : "nobody";
echo $greeting;
"#,
    );
    assert_eq!(out, "Alice");
}

#[test]
fn test_ternary_function_result() {
    let out = compile_and_run(
        r#"<?php
function get_name() { return "Bob"; }
echo true ? get_name() : "default";
"#,
    );
    assert_eq!(out, "Bob");
}

#[test]
fn test_ternary_variable_int_vs_string() {
    let out = compile_and_run(
        r#"<?php
$count = 5;
$label = "none";
echo ($count > 0) ? $count : $label;
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_ternary_method_call_result() {
    let out = compile_and_run(
        r#"<?php
class Box { public $val;
    public function __construct($v) { $this->val = $v; }
    public function get() { return $this->val; }
}
$b = new Box("hello");
echo true ? $b->get() : "fallback";
"#,
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_chained_closure_call() {
    let out = compile_and_run(
        "<?php $f = function() { return function() { return 99; }; }; echo $f()();",
    );
    assert_eq!(out, "99");
}

// --- do...while ---
