use crate::support::*;

// --- Anonymous functions (closures) and arrow functions ---

#[test]
fn test_closure_basic() {
    let out = compile_and_run(
        r#"<?php
$double = function($x) { return $x * 2; };
echo $double(5);
"#,
    );
    assert_eq!(out, "10");
}

#[test]
fn test_closure_multiple_params() {
    let out = compile_and_run(
        r#"<?php
$add = function($a, $b) { return $a + $b; };
echo $add(3, 7);
"#,
    );
    assert_eq!(out, "10");
}

#[test]
fn test_arrow_function_basic() {
    let out = compile_and_run(
        r#"<?php
$triple = fn($x) => $x * 3;
echo $triple(4);
"#,
    );
    assert_eq!(out, "12");
}

#[test]
fn test_arrow_function_expression() {
    let out = compile_and_run(
        r#"<?php
$calc = fn($x) => $x * $x + 1;
echo $calc(5);
"#,
    );
    assert_eq!(out, "26");
}

#[test]
fn test_closure_array_map() {
    let out = compile_and_run(
        r#"<?php
$result = array_map(function($x) { return $x * 10; }, [1, 2, 3]);
echo $result[0];
echo $result[1];
echo $result[2];
"#,
    );
    assert_eq!(out, "102030");
}

#[test]
fn test_arrow_function_array_map() {
    let out = compile_and_run(
        r#"<?php
$result = array_map(fn($x) => $x + 100, [1, 2, 3]);
echo $result[0];
echo $result[1];
echo $result[2];
"#,
    );
    assert_eq!(out, "101102103");
}

#[test]
fn test_closure_array_filter() {
    let out = compile_and_run(
        r#"<?php
$evens = array_filter([1, 2, 3, 4, 5, 6], function($x) { return $x % 2 == 0; });
echo count($evens);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_arrow_function_array_filter() {
    let out = compile_and_run(
        r#"<?php
$big = array_filter([1, 5, 10, 15, 20], fn($x) => $x > 8);
echo count($big);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_closure_as_variable_then_call() {
    let out = compile_and_run(
        r#"<?php
$fn = function($x) { return $x + 1; };
$a = $fn(10);
$b = $fn(20);
echo $a;
echo $b;
"#,
    );
    assert_eq!(out, "1121");
}

#[test]
fn test_closure_no_params() {
    let out = compile_and_run(
        r#"<?php
$hello = function() { return 42; };
echo $hello();
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_arrow_no_params() {
    let out = compile_and_run(
        r#"<?php
$val = fn() => 99;
echo $val();
"#,
    );
    assert_eq!(out, "99");
}

#[test]
fn test_closure_array_reduce() {
    let out = compile_and_run(
        r#"<?php
$sum = array_reduce([1, 2, 3, 4], function($carry, $item) { return $carry + $item; }, 0);
echo $sum;
"#,
    );
    assert_eq!(out, "10");
}

// --- IIFE (Immediately Invoked Function Expression) ---

#[test]
fn test_iife_basic() {
    let out = compile_and_run(
        r#"<?php
echo (function() { return 42; })();
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_iife_with_args() {
    let out = compile_and_run(
        r#"<?php
echo (function($x) { return $x * 3; })(7);
"#,
    );
    assert_eq!(out, "21");
}

#[test]
fn test_iife_arrow() {
    let out = compile_and_run(
        r#"<?php
echo (fn($x) => $x + 100)(5);
"#,
    );
    assert_eq!(out, "105");
}

// --- Calling closures from array access ---

#[test]
fn test_closure_from_array_call() {
    let out = compile_and_run(
        r#"<?php
$fns = [function($x) { return $x * 10; }];
echo $fns[0](5);
"#,
    );
    assert_eq!(out, "50");
}

#[test]
fn test_closure_from_array_no_args() {
    let out = compile_and_run(
        r#"<?php
$fns = [function() { return 99; }];
echo $fns[0]();
"#,
    );
    assert_eq!(out, "99");
}

// --- Closure returning closure ---

#[test]
fn test_closure_returning_closure() {
    let out = compile_and_run(
        r#"<?php
$f = function() { return function() { return 99; }; };
$g = $f();
echo $g();
"#,
    );
    assert_eq!(out, "99");
}

#[test]
fn test_closure_returning_closure_with_args() {
    let out = compile_and_run(
        r#"<?php
$maker = function() { return function($x) { return $x * 3; }; };
$fn = $maker();
echo $fn(7);
"#,
    );
    assert_eq!(out, "21");
}

// ===== Feature 1: Default parameter values =====

#[test]
fn test_default_param_string() {
    let out = compile_and_run(
        r#"<?php
function greet($name = "world") {
    echo "Hello " . $name;
}
greet();
"#,
    );
    assert_eq!(out, "Hello world");
}

#[test]
fn test_default_param_override() {
    let out = compile_and_run(
        r#"<?php
function greet($name = "world") {
    echo "Hello " . $name;
}
greet("PHP");
"#,
    );
    assert_eq!(out, "Hello PHP");
}

#[test]
fn test_default_param_int() {
    let out = compile_and_run(
        r#"<?php
function add($a, $b = 0) {
    return $a + $b;
}
echo add(5);
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_default_param_int_override() {
    let out = compile_and_run(
        r#"<?php
function add($a, $b = 0) {
    return $a + $b;
}
echo add(5, 3);
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_default_param_multiple() {
    let out = compile_and_run(
        r#"<?php
function multi($a = 1, $b = 2, $c = 3) {
    echo $a + $b + $c;
}
multi();
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_default_param_partial() {
    let out = compile_and_run(
        r#"<?php
function multi($a = 1, $b = 2, $c = 3) {
    echo $a + $b + $c;
}
multi(10);
"#,
    );
    assert_eq!(out, "15");
}

// ===== Feature 2: Null coalescing operator ?? =====

#[test]
fn test_null_coalesce_null_value() {
    let out = compile_and_run(
        r#"<?php
$x = null;
echo $x ?? "default";
"#,
    );
    assert_eq!(out, "default");
}

#[test]
fn test_null_coalesce_non_null() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
echo $x ?? 0;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_null_coalesce_chained() {
    let out = compile_and_run(
        r#"<?php
$x = null;
$y = null;
echo $x ?? $y ?? "found";
"#,
    );
    assert_eq!(out, "found");
}

#[test]
fn test_null_coalesce_literal_null() {
    let out = compile_and_run(
        r#"<?php
echo null ?? "fallback";
"#,
    );
    assert_eq!(out, "fallback");
}

#[test]
fn test_null_coalesce_string() {
    let out = compile_and_run(
        r#"<?php
$name = "Alice";
echo $name ?? "default";
"#,
    );
    assert_eq!(out, "Alice");
}

#[test]
fn test_null_coalesce_null_to_string() {
    let out = compile_and_run(
        r#"<?php
$name = null;
echo $name ?? "default";
"#,
    );
    assert_eq!(out, "default");
}

#[test]
fn test_null_coalesce_empty_string() {
    let out = compile_and_run(
        r#"<?php
$val = "";
echo ($val ?? "fallback") . "|done";
"#,
    );
    assert_eq!(out, "|done");
}

#[test]
fn test_null_coalesce_int() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
echo $x ?? 0;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_null_coalesce_null_to_int() {
    let out = compile_and_run(
        r#"<?php
$x = null;
echo $x ?? 99;
"#,
    );
    assert_eq!(out, "99");
}

#[test]
fn test_null_coalesce_chain() {
    let out = compile_and_run(
        r#"<?php
$a = null;
$b = null;
$c = "found";
echo $a ?? $b ?? $c;
"#,
    );
    assert_eq!(out, "found");
}

#[test]
fn test_null_coalesce_float() {
    let out = compile_and_run(
        r#"<?php
$x = 3.14;
echo $x ?? 0.0;
"#,
    );
    assert_eq!(out, "3.14");
}

#[test]
fn test_null_coalesce_null_to_float() {
    let out = compile_and_run(
        r#"<?php
$x = null;
echo $x ?? 2.718;
"#,
    );
    assert_eq!(out, "2.718");
}

#[test]
fn test_null_coalesce_float_in_calc() {
    let out = compile_and_run(
        r#"<?php
$pi = null;
$val = $pi ?? 3.14159;
echo round($val * 2, 4);
"#,
    );
    assert_eq!(out, "6.2832");
}

#[test]
fn test_null_coalesce_result_survives_nested_function_calls_in_concat() {
    let out = compile_and_run(
        r#"<?php
function fallback_pi($x) {
    return $x ?? 3.14159;
}

echo round(fallback_pi(2), 1) . "|" . round(fallback_pi(null), 4);
"#,
    );
    assert_eq!(out, "2|3.1416");
}

// ===== Feature 3: Bitwise operators =====

#[test]
fn test_bitwise_and() {
    let out = compile_and_run("<?php echo 5 & 3;");
    assert_eq!(out, "1");
}

#[test]
fn test_bitwise_or() {
    let out = compile_and_run("<?php echo 5 | 3;");
    assert_eq!(out, "7");
}

#[test]
fn test_bitwise_xor() {
    let out = compile_and_run("<?php echo 5 ^ 3;");
    assert_eq!(out, "6");
}

#[test]
fn test_bitwise_not() {
    let out = compile_and_run("<?php echo ~0;");
    assert_eq!(out, "-1");
}

#[test]
fn test_shift_left() {
    let out = compile_and_run("<?php echo 1 << 4;");
    assert_eq!(out, "16");
}

#[test]
fn test_shift_right() {
    let out = compile_and_run("<?php echo 16 >> 2;");
    assert_eq!(out, "4");
}

#[test]
fn test_bitwise_combined() {
    let out = compile_and_run("<?php echo (255 & 15) | 48;");
    assert_eq!(out, "63");
}

#[test]
fn test_bitwise_not_positive() {
    let out = compile_and_run("<?php echo ~255;");
    assert_eq!(out, "-256");
}

#[test]
fn test_shift_left_multiply() {
    let out = compile_and_run("<?php echo 3 << 3;");
    assert_eq!(out, "24");
}

#[test]
fn test_shift_right_negative() {
    // Arithmetic shift preserves sign
    let out = compile_and_run("<?php echo -16 >> 2;");
    assert_eq!(out, "-4");
}

// ===== Feature 4: Spaceship operator <=> =====

#[test]
fn test_spaceship_less() {
    let out = compile_and_run("<?php echo 1 <=> 2;");
    assert_eq!(out, "-1");
}

#[test]
fn test_spaceship_equal() {
    let out = compile_and_run("<?php echo 2 <=> 2;");
    assert_eq!(out, "0");
}

#[test]
fn test_spaceship_greater() {
    let out = compile_and_run("<?php echo 3 <=> 2;");
    assert_eq!(out, "1");
}

#[test]
fn test_spaceship_negative() {
    let out = compile_and_run("<?php echo -5 <=> 5;");
    assert_eq!(out, "-1");
}

// ===== Feature 5: Heredoc / Nowdoc strings =====

#[test]
fn test_heredoc_basic() {
    let out = compile_and_run("<?php\necho <<<EOT\nHello World\nEOT;\n");
    assert_eq!(out, "Hello World");
}

#[test]
fn test_heredoc_multiline() {
    let out = compile_and_run("<?php\necho <<<EOT\nLine 1\nLine 2\nLine 3\nEOT;\n");
    assert_eq!(out, "Line 1\nLine 2\nLine 3");
}

#[test]
fn test_heredoc_escapes() {
    let out = compile_and_run("<?php\necho <<<EOT\nHello\\tWorld\\n\nEOT;\n");
    assert_eq!(out, "Hello\tWorld\n");
}

#[test]
fn test_nowdoc_basic() {
    let out = compile_and_run("<?php\necho <<<'EOT'\nHello World\nEOT;\n");
    assert_eq!(out, "Hello World");
}

#[test]
fn test_nowdoc_no_escapes() {
    let out = compile_and_run("<?php\necho <<<'EOT'\nHello\\tWorld\nEOT;\n");
    assert_eq!(out, "Hello\\tWorld");
}

#[test]
fn test_heredoc_interpolation() {
    let out =
        compile_and_run("<?php\n$name = \"World\";\n$s = <<<EOT\nHello $name\nEOT;\necho $s;\n");
    assert_eq!(out, "Hello World");
}

#[test]
fn test_heredoc_interpolation_multiple_vars() {
    let out = compile_and_run(
        "<?php\n$first = \"Hello\";\n$second = \"World\";\necho <<<EOT\n$first $second\nEOT;\n",
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_heredoc_interpolation_multiline() {
    let out = compile_and_run(
        "<?php\n$name = \"Alice\";\necho <<<EOT\nHello $name\nWelcome $name\nEOT;\n",
    );
    assert_eq!(out, "Hello Alice\nWelcome Alice");
}

#[test]
fn test_nowdoc_no_interpolation() {
    let out = compile_and_run("<?php\n$name = \"World\";\necho <<<'EOT'\nHello $name\nEOT;\n");
    assert_eq!(out, "Hello $name");
}

#[test]
fn test_heredoc_escaped_dollar() {
    let out = compile_and_run("<?php\necho <<<EOT\nPrice is \\$100\nEOT;\n");
    assert_eq!(out, "Price is $100");
}

// --- Constants (const / define) ---

#[test]
fn test_const_int() {
    let out = compile_and_run("<?php\nconst MAX = 100;\necho MAX;\n");
    assert_eq!(out, "100");
}

#[test]
fn test_const_string() {
    let out = compile_and_run("<?php\nconst GREETING = \"hello\";\necho GREETING;\n");
    assert_eq!(out, "hello");
}

#[test]
fn test_const_float() {
    let out = compile_and_run("<?php\nconst PI = 3.14;\necho PI;\n");
    assert_eq!(out, "3.14");
}

#[test]
fn test_const_bool() {
    let out = compile_and_run("<?php\nconst DEBUG = true;\necho DEBUG;\n");
    assert_eq!(out, "1");
}

#[test]
fn test_define_int() {
    let out = compile_and_run("<?php\ndefine(\"MAX_SIZE\", 256);\necho MAX_SIZE;\n");
    assert_eq!(out, "256");
}

#[test]
fn test_define_string() {
    let out = compile_and_run("<?php\ndefine(\"APP_NAME\", \"elephc\");\necho APP_NAME;\n");
    assert_eq!(out, "elephc");
}

#[test]
fn test_const_in_expression() {
    let out = compile_and_run("<?php\nconst X = 10;\nconst Y = 20;\necho X + Y;\n");
    assert_eq!(out, "30");
}

#[test]
fn test_const_in_function() {
    let out =
        compile_and_run("<?php\nconst LIMIT = 42;\nfunction test() { echo LIMIT; }\ntest();\n");
    assert_eq!(out, "42");
}

#[test]
fn test_define_in_function() {
    let out =
        compile_and_run("<?php\ndefine(\"RATE\", 100);\nfunction show() { echo RATE; }\nshow();\n");
    assert_eq!(out, "100");
}

#[test]
fn test_const_concat() {
    let out = compile_and_run("<?php\nconst PREFIX = \"hello\";\necho PREFIX . \" world\";\n");
    assert_eq!(out, "hello world");
}

// --- List unpacking ---

#[test]
fn test_list_unpack_int() {
    let out = compile_and_run(
        "<?php\n[$a, $b, $c] = [10, 20, 30];\necho $a . \" \" . $b . \" \" . $c;\n",
    );
    assert_eq!(out, "10 20 30");
}

#[test]
fn test_list_unpack_string() {
    let out = compile_and_run("<?php\n[$x, $y] = [\"hello\", \"world\"];\necho $x . \" \" . $y;\n");
    assert_eq!(out, "hello world");
}

#[test]
fn test_list_unpack_from_variable() {
    let out = compile_and_run(
        "<?php\n$arr = [1, 2, 3];\n[$a, $b, $c] = $arr;\necho $a . \" \" . $b . \" \" . $c;\n",
    );
    assert_eq!(out, "1 2 3");
}

#[test]
fn test_list_unpack_two_vars() {
    let out = compile_and_run("<?php\n[$first, $second] = [42, 99];\necho $first + $second;\n");
    assert_eq!(out, "141");
}

// --- call_user_func_array ---

#[test]
fn test_call_user_func_array_basic() {
    let out = compile_and_run("<?php\nfunction add($a, $b) { return $a + $b; }\necho call_user_func_array(\"add\", [3, 4]);\n");
    assert_eq!(out, "7");
}

#[test]
fn test_call_user_func_array_single_arg() {
    let out = compile_and_run("<?php\nfunction double($n) { return $n * 2; }\necho call_user_func_array(\"double\", [21]);\n");
    assert_eq!(out, "42");
}

#[test]
fn test_call_user_func_array_string_return() {
    let out = compile_and_run("<?php\nfunction greet($name) { return \"Hello \" . $name; }\necho call_user_func_array(\"greet\", [\"World\"]);\n");
    assert_eq!(out, "Hello World");
}

#[test]
fn test_call_user_func_array_variadic_callback() {
    let out = compile_and_run(
        "<?php
        function summarize($head = 1, ...$rest) {
            echo $head;
            echo \":\";
            echo count($rest);
        }
        call_user_func_array(summarize(...), [7, 8, 9]);
        ",
    );
    assert_eq!(out, "7:2");
}

// -- v0.8 constants --

#[test]
fn test_php_eol() {
    let out = compile_and_run("<?php echo \"a\" . PHP_EOL . \"b\";");
    assert_eq!(out, "a\nb");
}

#[test]
fn test_php_os() {
    let out = compile_and_run("<?php echo PHP_OS;");
    assert_eq!(out, "Darwin");
}

#[test]
fn test_directory_separator() {
    let out = compile_and_run("<?php echo DIRECTORY_SEPARATOR;");
    assert_eq!(out, "/");
}

// -- v0.8 time / microtime --

#[test]
fn test_time() {
    let out = compile_and_run("<?php $t = time(); if ($t > 1000000000) { echo \"ok\"; }");
    assert_eq!(out, "ok");
}

#[test]
fn test_microtime() {
    let out = compile_and_run("<?php $t = microtime(true); if ($t > 1000000000) { echo \"ok\"; }");
    assert_eq!(out, "ok");
}

// -- v0.8 sleep / usleep --

#[test]
fn test_sleep_zero() {
    let out = compile_and_run("<?php sleep(0); echo \"ok\";");
    assert_eq!(out, "ok");
}

#[test]
fn test_usleep_zero() {
    let out = compile_and_run("<?php usleep(0); echo \"ok\";");
    assert_eq!(out, "ok");
}

// -- v0.8 getenv --

#[test]
fn test_getenv_home() {
    let out =
        compile_and_run("<?php $home = getenv(\"HOME\"); if (strlen($home) > 0) { echo \"ok\"; }");
    assert_eq!(out, "ok");
}

#[test]
fn test_getenv_nonexistent() {
    let out = compile_and_run(
        "<?php $missing = getenv(\"ELEPHC_NONEXISTENT_VAR_XYZ\"); echo strlen($missing);",
    );
    assert_eq!(out, "0");
}

#[test]
fn test_putenv() {
    let out = compile_and_run(
        r#"<?php
putenv("ELEPHC_TEST_VAR=hello");
echo getenv("ELEPHC_TEST_VAR");
"#,
    );
    assert_eq!(out, "hello");
}

// -- v0.8 phpversion / php_uname --

#[test]
fn test_phpversion() {
    let out = compile_and_run("<?php echo phpversion();");
    assert_eq!(out, "0.7.1");
}

#[test]
fn test_php_uname() {
    let out = compile_and_run("<?php $os = php_uname(); if (strlen($os) > 0) { echo \"ok\"; }");
    assert_eq!(out, "ok");
}

// -- v0.8 exec / shell_exec / system / passthru --

#[test]
fn test_shell_exec() {
    let out = compile_and_run("<?php $out = shell_exec(\"echo hello\"); echo trim($out);");
    assert_eq!(out, "hello");
}

#[test]
fn test_exec() {
    let out = compile_and_run("<?php $out = exec(\"echo test\"); echo trim($out);");
    assert_eq!(out, "test");
}

#[test]
fn test_system() {
    let out = compile_and_run("<?php system(\"echo hi\");");
    assert_eq!(out, "hi\n");
}

#[test]
fn test_passthru() {
    let out = compile_and_run("<?php passthru(\"echo bye\");");
    assert_eq!(out, "bye\n");
}

// --- Global variables ---

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

// --- Pass by reference ---

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

// --- Variadic functions ---

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

// --- Spread operator ---

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

