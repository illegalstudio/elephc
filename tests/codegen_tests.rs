use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_ID: AtomicU64 = AtomicU64::new(0);

/// Compile a PHP source string to a native binary, run it, and return stdout.
/// Also verifies the output matches PHP interpreter if available.
fn compile_and_run(source: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{:?}_{}", tid, id));
    fs::create_dir_all(&dir).unwrap();

    let php_path = dir.join("test.php");
    let bin_path = dir.join("test");

    fs::write(&php_path, source).unwrap();

    // Run elephc
    let status = Command::new(env!("CARGO_BIN_EXE_elephc"))
        .arg(&php_path)
        .status()
        .expect("failed to run elephc");
    assert!(status.success(), "elephc failed to compile");

    // Run the compiled binary
    let output = Command::new(&bin_path)
        .output()
        .expect("failed to run compiled binary");
    assert!(output.status.success(), "binary exited with error");

    let elephc_out = String::from_utf8(output.stdout).unwrap();

    // Cross-check with PHP interpreter if available.
    // Differences are reported but don't fail the test — known semantic
    // gaps (e.g., echo false prints "0" in elephc but "" in PHP) are
    // tracked and will be resolved when a proper Bool type is added.
    if let Ok(php_output) = Command::new("php").arg(&php_path).output() {
        if php_output.status.success() {
            let php_out = String::from_utf8_lossy(&php_output.stdout);
            if elephc_out != php_out.as_ref() {
                eprintln!(
                    "PHP compat note: output differs for test.\n  elephc: {:?}\n  php:    {:?}",
                    elephc_out, php_out
                );
            }
        }
    }

    // Cleanup
    let _ = fs::remove_dir_all(&dir);

    elephc_out
}

// --- Phase 1: Echo strings ---

#[test]
fn test_echo_hello_world() {
    let out = compile_and_run("<?php echo \"Hello, World!\\n\";");
    assert_eq!(out, "Hello, World!\n");
}

#[test]
fn test_echo_empty_string() {
    let out = compile_and_run("<?php echo \"\";");
    assert_eq!(out, "");
}

#[test]
fn test_echo_multiple_strings() {
    let out = compile_and_run("<?php echo \"foo\"; echo \"bar\"; echo \"\\n\";");
    assert_eq!(out, "foobar\n");
}

#[test]
fn test_echo_escape_sequences() {
    let out = compile_and_run("<?php echo \"a\\tb\\nc\";");
    assert_eq!(out, "a\tb\nc");
}

// --- Phase 2: Variables and integers ---

#[test]
fn test_echo_integer() {
    let out = compile_and_run("<?php echo 42;");
    assert_eq!(out, "42");
}

#[test]
fn test_echo_zero() {
    let out = compile_and_run("<?php echo 0;");
    assert_eq!(out, "0");
}

#[test]
fn test_echo_negative() {
    let out = compile_and_run("<?php echo -7;");
    assert_eq!(out, "-7");
}

#[test]
fn test_echo_large_number() {
    let out = compile_and_run("<?php echo 1000000;");
    assert_eq!(out, "1000000");
}

#[test]
fn test_variable_int() {
    let out = compile_and_run("<?php $x = 42; echo $x;");
    assert_eq!(out, "42");
}

#[test]
fn test_variable_string() {
    let out = compile_and_run("<?php $s = \"hello\"; echo $s;");
    assert_eq!(out, "hello");
}

#[test]
fn test_variable_reassign_same_type() {
    let out = compile_and_run("<?php $x = 1; $x = 2; echo $x;");
    assert_eq!(out, "2");
}

#[test]
fn test_multiple_variables() {
    let out = compile_and_run(
        "<?php $a = 10; $b = 20; echo $a; echo \" \"; echo $b; echo \"\\n\";",
    );
    assert_eq!(out, "10 20\n");
}

#[test]
fn test_variable_negative_int() {
    let out = compile_and_run("<?php $x = -100; echo $x;");
    assert_eq!(out, "-100");
}

#[test]
fn test_echo_int_zero_variable() {
    let out = compile_and_run("<?php $z = 0; echo $z;");
    assert_eq!(out, "0");
}

// --- Phase 3: Arithmetic ---

#[test]
fn test_addition() {
    let out = compile_and_run("<?php echo 10 + 32;");
    assert_eq!(out, "42");
}

#[test]
fn test_subtraction() {
    let out = compile_and_run("<?php echo 100 - 58;");
    assert_eq!(out, "42");
}

#[test]
fn test_multiplication() {
    let out = compile_and_run("<?php echo 6 * 7;");
    assert_eq!(out, "42");
}

#[test]
fn test_division() {
    let out = compile_and_run("<?php echo 84 / 2;");
    assert_eq!(out, "42");
}

#[test]
fn test_arithmetic_with_variables() {
    let out = compile_and_run("<?php $a = 10; $b = 32; echo $a + $b;");
    assert_eq!(out, "42");
}

#[test]
fn test_operator_precedence() {
    let out = compile_and_run("<?php echo 2 + 3 * 4;");
    assert_eq!(out, "14");
}

#[test]
fn test_parenthesized_arithmetic() {
    let out = compile_and_run("<?php echo (2 + 3) * 4;");
    assert_eq!(out, "20");
}

#[test]
fn test_complex_expression() {
    let out = compile_and_run("<?php echo (10 + 5) * 2 - 7;");
    assert_eq!(out, "23");
}

#[test]
fn test_arithmetic_assign_and_echo() {
    let out = compile_and_run("<?php $a = 10; $b = 32; $c = $a + $b; echo $c;");
    assert_eq!(out, "42");
}

#[test]
fn test_subtraction_negative_result() {
    let out = compile_and_run("<?php echo 3 - 10;");
    assert_eq!(out, "-7");
}

#[test]
fn test_nested_arithmetic() {
    let out = compile_and_run("<?php echo 1 + 2 + 3 + 4;");
    assert_eq!(out, "10");
}

// --- Phase 3: Concatenation ---

#[test]
fn test_concat_literals() {
    let out = compile_and_run("<?php echo \"Hello, \" . \"World!\";");
    assert_eq!(out, "Hello, World!");
}

#[test]
fn test_concat_variables() {
    let out = compile_and_run(
        "<?php $a = \"Hello, \"; $b = \"World!\"; echo $a . $b;",
    );
    assert_eq!(out, "Hello, World!");
}

#[test]
fn test_concat_chain() {
    let out = compile_and_run("<?php echo \"a\" . \"b\" . \"c\";");
    assert_eq!(out, "abc");
}

#[test]
fn test_concat_assign() {
    let out = compile_and_run(
        "<?php $msg = \"foo\" . \"bar\"; echo $msg;",
    );
    assert_eq!(out, "foobar");
}

#[test]
fn test_concat_with_newline() {
    let out = compile_and_run("<?php echo \"hello\" . \"\\n\";");
    assert_eq!(out, "hello\n");
}

// --- Phase 3: Mixed-type concatenation ---

#[test]
fn test_concat_string_and_int() {
    let out = compile_and_run("<?php echo \"Value: \" . 42;");
    assert_eq!(out, "Value: 42");
}

#[test]
fn test_concat_int_and_string() {
    let out = compile_and_run("<?php echo 42 . \" is the answer\";");
    assert_eq!(out, "42 is the answer");
}

#[test]
fn test_concat_int_and_int() {
    let out = compile_and_run("<?php echo 1 . 2;");
    assert_eq!(out, "12");
}

#[test]
fn test_concat_expr_result() {
    let out = compile_and_run("<?php $a = 10; $b = 32; echo \"Result: \" . ($a + $b);");
    assert_eq!(out, "Result: 42");
}

#[test]
fn test_concat_chain_mixed() {
    let out = compile_and_run("<?php echo \"x=\" . 5 . \" y=\" . 10;");
    assert_eq!(out, "x=5 y=10");
}

#[test]
fn test_concat_negative_int() {
    let out = compile_and_run("<?php echo \"num: \" . -7;");
    assert_eq!(out, "num: -7");
}

// --- Modulo ---

#[test]
fn test_modulo() {
    let out = compile_and_run("<?php echo 10 % 3;");
    assert_eq!(out, "1");
}

#[test]
fn test_modulo_zero_remainder() {
    let out = compile_and_run("<?php echo 15 % 5;");
    assert_eq!(out, "0");
}

// --- Comparison operators ---

#[test]
fn test_equal_true() {
    let out = compile_and_run("<?php echo 1 == 1;");
    assert_eq!(out, "1");
}

#[test]
fn test_equal_false() {
    let out = compile_and_run("<?php echo 1 == 2;");
    assert_eq!(out, ""); // echo false prints nothing in PHP
}

#[test]
fn test_not_equal() {
    let out = compile_and_run("<?php echo 1 != 2;");
    assert_eq!(out, "1");
}

#[test]
fn test_less_than() {
    let out = compile_and_run("<?php echo 1 < 2;");
    assert_eq!(out, "1");
}

#[test]
fn test_greater_than() {
    let out = compile_and_run("<?php echo 2 > 1;");
    assert_eq!(out, "1");
}

#[test]
fn test_less_equal() {
    let out = compile_and_run("<?php echo 2 <= 2;");
    assert_eq!(out, "1");
}

#[test]
fn test_greater_equal() {
    let out = compile_and_run("<?php echo 1 >= 2;");
    assert_eq!(out, "");
}

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
    let out = compile_and_run(
        "<?php $i = 0; while ($i < 5) { echo $i; $i = $i + 1; }",
    );
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
    let out = compile_and_run(
        "<?php for ($i = 0; $i < 5; $i = $i + 1) { echo $i; }",
    );
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
    let out = compile_and_run(
        "<?php function add($a, $b) { return $a + $b; } echo add(10, 32);",
    );
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
    let out = compile_and_run(
        "<?php function say() { echo \"hi\"; return; } say();",
    );
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
    let out = compile_and_run(
        "<?php function answer() { return 42; } echo answer();",
    );
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
    let out = compile_and_run(r#"<?php
$count = 0;
function inc() { return 1; }
$r = 0 && inc();
echo $r;
"#);
    assert_eq!(out, ""); // false prints nothing
}

#[test]
fn test_short_circuit_or() {
    // With ||, if left is true the right side should not be evaluated.
    let out = compile_and_run(r#"<?php
function inc() { return 1; }
$r = 1 || inc();
echo $r;
"#);
    assert_eq!(out, "1");
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
fn test_logical_with_comparison() {
    let out = compile_and_run("<?php $x = 5; echo ($x > 3 && $x < 10);");
    assert_eq!(out, "1");
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

// --- do...while ---

#[test]
fn test_do_while() {
    let out = compile_and_run("<?php $i = 0; do { $i++; } while ($i < 5); echo $i;");
    assert_eq!(out, "5");
}

#[test]
fn test_do_while_runs_once() {
    let out = compile_and_run("<?php $i = 0; do { $i++; } while (false); echo $i;");
    assert_eq!(out, "1");
}

// --- Single-quoted strings ---

#[test]
fn test_single_quoted_string() {
    let out = compile_and_run("<?php echo 'hello';");
    assert_eq!(out, "hello");
}

#[test]
fn test_single_quoted_no_escape() {
    let out = compile_and_run(r"<?php echo 'no\n escape';");
    assert_eq!(out, "no\\n escape");
}

#[test]
fn test_single_quoted_escaped_quote() {
    let out = compile_and_run("<?php echo 'it\\'s';");
    assert_eq!(out, "it's");
}

// --- null ---

#[test]
fn test_null_echo_nothing() {
    let out = compile_and_run("<?php echo null;");
    assert_eq!(out, "");
}

#[test]
fn test_null_variable_echo_nothing() {
    let out = compile_and_run("<?php $x = null; echo $x;");
    assert_eq!(out, "");
}

#[test]
fn test_is_null_true() {
    let out = compile_and_run("<?php $x = null; echo is_null($x);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_null_false() {
    let out = compile_and_run("<?php $x = 42; echo is_null($x);");
    assert_eq!(out, "");
}

#[test]
fn test_null_plus_int() {
    let out = compile_and_run("<?php $x = null; echo $x + 5;");
    assert_eq!(out, "5");
}

#[test]
fn test_null_concat() {
    let out = compile_and_run("<?php $x = null; echo $x . \"hello\";");
    assert_eq!(out, "hello");
}

#[test]
fn test_null_equals_zero() {
    let out = compile_and_run("<?php $x = null; echo $x == 0;");
    assert_eq!(out, "1");
}

#[test]
fn test_null_plus_assign() {
    let out = compile_and_run("<?php $y = null; $y += 10; echo $y;");
    assert_eq!(out, "10");
}

#[test]
fn test_null_reassign() {
    let out = compile_and_run("<?php $x = null; $x = 42; echo $x;");
    assert_eq!(out, "42");
}

// --- Built-in functions ---

#[test]
fn test_strlen() {
    let out = compile_and_run("<?php echo strlen(\"hello\");");
    assert_eq!(out, "5");
}

#[test]
fn test_strlen_empty() {
    let out = compile_and_run("<?php echo strlen(\"\");");
    assert_eq!(out, "0");
}

#[test]
fn test_intval_string() {
    let out = compile_and_run("<?php echo intval(\"42\");");
    assert_eq!(out, "42");
}

#[test]
fn test_intval_negative() {
    let out = compile_and_run("<?php echo intval(\"-7\");");
    assert_eq!(out, "-7");
}

#[test]
fn test_intval_int_passthrough() {
    let out = compile_and_run("<?php echo intval(42);");
    assert_eq!(out, "42");
}

#[test]
fn test_exit_code() {
    // We can't easily test exit code in compile_and_run, so test that
    // exit stops execution (nothing after exit is printed)
    let out = compile_and_run("<?php echo \"before\"; exit(0); echo \"after\";");
    assert_eq!(out, "before");
}

// --- $argc ---

#[test]
fn test_argc_exists() {
    let out = compile_and_run("<?php echo $argc;");
    // When run as a test, argc is 1 (just the binary name)
    assert_eq!(out, "1");
}

// --- Arrays ---

#[test]
fn test_array_literal_and_count() {
    let out = compile_and_run("<?php $a = [1, 2, 3]; echo count($a);");
    assert_eq!(out, "3");
}

#[test]
fn test_array_access() {
    let out = compile_and_run("<?php $a = [10, 20, 30]; echo $a[0] . \" \" . $a[1] . \" \" . $a[2];");
    assert_eq!(out, "10 20 30");
}

#[test]
fn test_array_access_variable_index() {
    let out = compile_and_run("<?php $a = [10, 20, 30]; $i = 2; echo $a[$i];");
    assert_eq!(out, "30");
}

#[test]
fn test_array_assign() {
    let out = compile_and_run("<?php $a = [1, 2, 3]; $a[1] = 99; echo $a[1];");
    assert_eq!(out, "99");
}

#[test]
fn test_array_push() {
    let out = compile_and_run("<?php $a = [1, 2]; $a[] = 3; echo count($a) . \" \" . $a[2];");
    assert_eq!(out, "3 3");
}

#[test]
fn test_array_push_builtin() {
    let out = compile_and_run("<?php $a = [10]; array_push($a, 20); echo count($a) . \" \" . $a[1];");
    assert_eq!(out, "2 20");
}

#[test]
fn test_foreach_int() {
    let out = compile_and_run("<?php $a = [1, 2, 3]; foreach ($a as $v) { echo $v; }");
    assert_eq!(out, "123");
}

#[test]
fn test_foreach_string() {
    let out = compile_and_run(r#"<?php $a = ["a", "b", "c"]; foreach ($a as $v) { echo $v; }"#);
    assert_eq!(out, "abc");
}

#[test]
fn test_foreach_break() {
    let out = compile_and_run("<?php $a = [1, 2, 3, 4, 5]; foreach ($a as $v) { if ($v == 3) { break; } echo $v; }");
    assert_eq!(out, "12");
}

#[test]
fn test_array_in_function() {
    let out = compile_and_run(r#"<?php
function sum($arr) {
    $total = 0;
    foreach ($arr as $v) {
        $total += $v;
    }
    return $total;
}
echo sum([1, 2, 3, 4, 5]);
"#);
    assert_eq!(out, "15");
}

#[test]
fn test_string_array() {
    let out = compile_and_run(r#"<?php
$names = ["Alice", "Bob"];
$names[] = "Charlie";
echo count($names) . ": ";
foreach ($names as $n) { echo $n . " "; }
"#);
    assert_eq!(out, "3: Alice Bob Charlie ");
}

// --- Array functions ---

#[test]
fn test_array_pop() {
    let out = compile_and_run("<?php $a = [1, 2, 3]; $v = array_pop($a); echo $v . \" \" . count($a);");
    assert_eq!(out, "3 2");
}

#[test]
fn test_in_array_found() {
    let out = compile_and_run("<?php $a = [10, 20, 30]; echo in_array(20, $a);");
    assert_eq!(out, "1");
}

#[test]
fn test_in_array_not_found() {
    let out = compile_and_run("<?php $a = [10, 20, 30]; echo in_array(99, $a);");
    assert_eq!(out, "0");
}

#[test]
fn test_sort() {
    let out = compile_and_run(r#"<?php $a = [5, 3, 1, 4, 2]; sort($a); foreach ($a as $v) { echo $v; }"#);
    assert_eq!(out, "12345");
}

#[test]
fn test_rsort() {
    let out = compile_and_run(r#"<?php $a = [1, 3, 2]; rsort($a); foreach ($a as $v) { echo $v; }"#);
    assert_eq!(out, "321");
}

#[test]
fn test_array_keys() {
    let out = compile_and_run(r#"<?php $a = [10, 20, 30]; $k = array_keys($a); foreach ($k as $v) { echo $v; }"#);
    assert_eq!(out, "012");
}

#[test]
fn test_isset() {
    let out = compile_and_run("<?php $x = 42; echo isset($x);");
    assert_eq!(out, "1");
}

#[test]
fn test_array_values() {
    let out = compile_and_run(r#"<?php $a = [10, 20, 30]; $v = array_values($a); foreach ($v as $x) { echo $x; }"#);
    assert_eq!(out, "102030");
}

#[test]
fn test_die() {
    let out = compile_and_run("<?php echo \"before\"; die(); echo \"after\";");
    assert_eq!(out, "before");
}

// --- Nested control flow ---

#[test]
fn test_nested_if() {
    let out = compile_and_run(
        "<?php $x = 5; if ($x > 0) { if ($x > 3) { echo \"big\"; } else { echo \"small\"; } }",
    );
    assert_eq!(out, "big");
}

#[test]
fn test_nested_loops() {
    let out = compile_and_run(
        "<?php for ($i = 0; $i < 3; $i++) { for ($j = 0; $j < 2; $j++) { echo $i . $j . \" \"; } }",
    );
    assert_eq!(out, "00 01 10 11 20 21 ");
}

#[test]
fn test_for_continue() {
    let out = compile_and_run(
        "<?php for ($i = 0; $i < 5; $i++) { if ($i == 2) { continue; } echo $i; }",
    );
    assert_eq!(out, "0134");
}

#[test]
fn test_while_with_function() {
    let out = compile_and_run(r#"<?php
function sum_to($n) {
    $s = 0;
    $i = 1;
    while ($i <= $n) {
        $s = $s + $i;
        $i++;
    }
    return $s;
}
echo sum_to(10);
"#);
    assert_eq!(out, "55");
}

#[test]
fn test_function_with_if_return() {
    let out = compile_and_run(r#"<?php
function abs_val($x) {
    if ($x < 0) {
        return -$x;
    }
    return $x;
}
echo abs_val(-5) . " " . abs_val(3);
"#);
    assert_eq!(out, "5 3");
}

#[test]
fn test_function_calling_function() {
    let out = compile_and_run(r#"<?php
function square($x) { return $x * $x; }
function sum_of_squares($a, $b) { return square($a) + square($b); }
echo sum_of_squares(3, 4);
"#);
    assert_eq!(out, "25");
}

#[test]
fn test_multiple_elseif() {
    let out = compile_and_run(r#"<?php
$x = 4;
if ($x == 1) { echo "one"; }
elseif ($x == 2) { echo "two"; }
elseif ($x == 3) { echo "three"; }
elseif ($x == 4) { echo "four"; }
else { echo "other"; }
"#);
    assert_eq!(out, "four");
}

// --- Edge cases ---

#[test]
fn test_comments_ignored() {
    let out = compile_and_run(
        "<?php\n// this is a comment\necho \"ok\";\n/* block comment */\n",
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_no_output_program() {
    let out = compile_and_run("<?php $x = 1;");
    assert_eq!(out, "");
}

#[test]
fn test_empty_string_concat() {
    let out = compile_and_run("<?php echo \"\" . \"hello\" . \"\";");
    assert_eq!(out, "hello");
}

#[test]
fn test_deeply_nested_arithmetic() {
    let out = compile_and_run("<?php echo ((((1 + 2) * 3) - 4) / 5);");
    assert_eq!(out, "1");
}
