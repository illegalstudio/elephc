use crate::support::*;

// -- Issue #25: \0 null byte in strings --
#[test]
fn test_null_byte_in_string() {
    let out = compile_and_run(r#"<?php echo strlen("ab\0cd");"#);
    assert_eq!(out, "5");
}

// -- Issue #26: empty string should be falsy --
#[test]
fn test_not_empty_string_is_true() {
    let out = compile_and_run(r#"<?php echo !!"";"#);
    assert_eq!(out, "");
}

#[test]
fn test_not_nonempty_string_is_false() {
    let out = compile_and_run(r#"<?php echo !!"hello";"#);
    assert_eq!(out, "1");
}

// -- Issue #27: is_numeric() should work for numeric strings --
#[test]
fn test_is_numeric_string_digits() {
    let out = compile_and_run(r#"<?php if (is_numeric("42")) { echo "yes"; } else { echo "no"; }"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_is_numeric_string_float() {
    let out =
        compile_and_run(r#"<?php if (is_numeric("3.14")) { echo "yes"; } else { echo "no"; }"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_is_numeric_string_negative() {
    let out = compile_and_run(r#"<?php if (is_numeric("-5")) { echo "yes"; } else { echo "no"; }"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_is_numeric_string_not_numeric() {
    let out =
        compile_and_run(r#"<?php if (is_numeric("abc")) { echo "yes"; } else { echo "no"; }"#);
    assert_eq!(out, "no");
}

// -- Issue #29: function_exists() should recognize builtins --
#[test]
fn test_function_exists_builtin() {
    let out = compile_and_run(r#"<?php echo function_exists("strlen") ? "yes" : "no";"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_function_exists_builtin_array_push() {
    let out = compile_and_run(r#"<?php echo function_exists("array_push") ? "yes" : "no";"#);
    assert_eq!(out, "yes");
}

// --- Issue #12: preg_split with \s shorthand ---

#[test]
fn test_preg_split_backslash_s() {
    let out = compile_and_run(
        r#"<?php
$parts = preg_split("/\s+/", "hello  world");
echo $parts[1];
"#,
    );
    assert_eq!(out, "world");
}

#[test]
fn test_preg_split_backslash_d() {
    let out = compile_and_run(
        r#"<?php
$parts = preg_split("/\d+/", "abc123def456ghi");
echo count($parts) . "|" . $parts[0] . "|" . $parts[1] . "|" . $parts[2];
"#,
    );
    assert_eq!(out, "3|abc|def|ghi");
}

#[test]
fn test_preg_match_backslash_s() {
    let out = compile_and_run(r#"<?php echo preg_match("/\s/", "hello world");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_preg_match_backslash_d() {
    let out = compile_and_run(r#"<?php echo preg_match("/\d+/", "abc123");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_preg_match_backslash_w() {
    let out = compile_and_run(r#"<?php echo preg_match("/^\w+$/", "hello_world");"#);
    assert_eq!(out, "1");
}

// --- Issue #14: hex integer literals ---

#[test]
fn test_hex_literal_0xff() {
    let out = compile_and_run("<?php echo 0xFF;");
    assert_eq!(out, "255");
}

#[test]
fn test_hex_literal_0x1a() {
    let out = compile_and_run("<?php echo 0x1A;");
    assert_eq!(out, "26");
}

#[test]
fn test_hex_literal_0x0() {
    let out = compile_and_run("<?php echo 0x0;");
    assert_eq!(out, "0");
}

#[test]
fn test_hex_literal_uppercase_prefix() {
    let out = compile_and_run("<?php echo 0XFF;");
    assert_eq!(out, "255");
}

#[test]
fn test_hex_literal_arithmetic() {
    let out = compile_and_run("<?php echo 0xFF + 1;");
    assert_eq!(out, "256");
}

// --- Issue #23: modulo by zero ---

#[test]
fn test_modulo_normal() {
    let out = compile_and_run("<?php echo 5 % 1;");
    assert_eq!(out, "0");
}

#[test]
fn test_modulo_by_zero() {
    let out = compile_and_run("<?php echo 5 % 0;");
    assert_eq!(out, "0");
}

#[test]
fn test_modulo_normal_remainder() {
    let out = compile_and_run("<?php echo 7 % 3;");
    assert_eq!(out, "1");
}

// --- Issue #24: negative array index ---

#[test]
fn test_negative_array_index_returns_null() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$v = $a[-1];
if (is_null($v)) { echo "null"; } else { echo "not null"; }
"#,
    );
    assert_eq!(out, "null");
}

#[test]
fn test_array_out_of_bounds_returns_null() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$v = $a[5];
if (is_null($v)) { echo "null"; } else { echo "not null"; }
"#,
    );
    assert_eq!(out, "null");
}

#[test]
fn test_array_valid_index_still_works() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo $a[0] . "|" . $a[1] . "|" . $a[2];
"#,
    );
    assert_eq!(out, "10|20|30");
}

// -- Issue #20: assoc array missing key should return null, not garbage --

#[test]
fn test_assoc_array_missing_key_returns_null() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => 1];
echo $m["missing"];
"#,
    );
    assert_eq!(out, "");
}

// -- Issue #28: array_map should handle string return values from callbacks --

#[test]
fn test_array_map_str_callback() {
    let out = compile_and_run(
        r#"<?php
$r = array_map(fn($x) => "v" . $x, [1, 2, 3]);
echo $r[0];
"#,
    );
    assert_eq!(out, "v1");
}

#[test]
fn test_array_map_str_callback_all_elements() {
    let out = compile_and_run(
        r#"<?php
$r = array_map(fn($x) => "item" . $x, [1, 2, 3]);
echo $r[0] . "|" . $r[1] . "|" . $r[2];
"#,
    );
    assert_eq!(out, "item1|item2|item3");
}

// -- Issue #13: empty array literal should be accepted by type checker --

#[test]
fn test_empty_array_literal() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a[] = 1;
echo count($a);
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_empty_array_json_encode() {
    let out = compile_and_run(
        r#"<?php
echo json_encode([]);
"#,
    );
    assert_eq!(out, "[]");
}

// -- Issue #16: Spread operator unpacking into named parameters --

#[test]
fn test_spread_into_named_params() {
    let out = compile_and_run(
        r#"<?php
function add($a, $b) { return $a + $b; }
$args = [3, 4];
echo add(...$args);
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_spread_into_named_params_three() {
    let out = compile_and_run(
        r#"<?php
function sum3($a, $b, $c) { return $a + $b + $c; }
$args = [10, 20, 30];
echo sum3(...$args);
"#,
    );
    assert_eq!(out, "60");
}

#[test]
fn test_spread_mixed_with_regular_args() {
    let out = compile_and_run(
        r#"<?php
function add3($a, $b, $c) { return $a + $b + $c; }
$rest = [20, 30];
echo add3(10, ...$rest);
"#,
    );
    assert_eq!(out, "60");
}

// -- Issue #17: Braceless single-statement bodies --

#[test]
fn test_braceless_if() {
    let out = compile_and_run(
        r#"<?php
if (true) echo "yes";
"#,
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_braceless_if_else() {
    let out = compile_and_run(
        r#"<?php
$x = 5;
if ($x > 10) echo "big";
else echo "small";
"#,
    );
    assert_eq!(out, "small");
}

#[test]
fn test_braceless_for() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 3; $i++) echo $i;
"#,
    );
    assert_eq!(out, "012");
}

#[test]
fn test_braceless_while() {
    let out = compile_and_run(
        r#"<?php
$i = 0;
while ($i < 3) echo $i++;
"#,
    );
    assert_eq!(out, "012");
}

#[test]
fn test_braceless_foreach() {
    let out = compile_and_run(
        r#"<?php
$arr = [1, 2, 3];
foreach ($arr as $v) echo $v;
"#,
    );
    assert_eq!(out, "123");
}

#[test]
fn test_braceless_else_if() {
    let out = compile_and_run(
        r#"<?php
$x = 5;
if ($x > 10) echo "big";
else if ($x > 3) echo "medium";
else echo "small";
"#,
    );
    assert_eq!(out, "medium");
}

// --- Bug regression tests ---

#[test]
fn test_closure_default_param() {
    let out = compile_and_run(
        r#"<?php
$fn = function($x, $y = 10) { return $x + $y; };
echo $fn(5);
"#,
    );
    assert_eq!(out, "15");
}

#[test]
fn test_closure_default_param_overridden() {
    let out = compile_and_run(
        r#"<?php
$fn = function($x, $y = 10) { return $x + $y; };
echo $fn(5, 20);
"#,
    );
    assert_eq!(out, "25");
}

#[test]
fn test_implode_int_array() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
echo implode(", ", $a);
"#,
    );
    assert_eq!(out, "1, 2, 3");
}

#[test]
fn test_implode_chained_array_builtins() {
    let out = compile_and_run(
        r#"<?php
echo implode(",", array_reverse([3, 1, 2]));
"#,
    );
    assert_eq!(out, "2,1,3");
}

#[test]
fn test_str_replace_in_foreach_assoc_function() {
    let out = compile_and_run(
        r#"<?php
function transform($map, $text) {
    foreach ($map as $key => $value) {
        $text = str_replace($key, $value, $text);
    }
    return $text;
}
$map = ["hello" => "world", "foo" => "bar"];
echo transform($map, "hello foo");
"#,
    );
    assert_eq!(out, "world bar");
}

// --- Bug fix: fmod sign (frintm → frintz) ---

#[test]
fn test_fmod_negative_dividend() {
    let out = compile_and_run("<?php echo fmod(-10, 3);");
    assert_eq!(out, "-1");
}

#[test]
fn test_float_modulo_negative() {
    let out = compile_and_run("<?php echo -10.0 % 3;");
    assert_eq!(out, "-1");
}

// --- Bug fix: string "0" is falsy ---

#[test]
fn test_string_zero_falsy_if() {
    let out = compile_and_run(
        r#"<?php
if ("0") { echo "bad"; } else { echo "good"; }
"#,
    );
    assert_eq!(out, "good");
}

#[test]
fn test_string_zero_falsy_ternary() {
    let out = compile_and_run(r#"<?php echo "0" ? "truthy" : "falsy";"#);
    assert_eq!(out, "falsy");
}

#[test]
fn test_string_zero_falsy_not() {
    let out = compile_and_run(r#"<?php echo !"0" ? "yes" : "no";"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_string_nonempty_truthy() {
    let out = compile_and_run(r#"<?php echo "hello" ? "yes" : "no";"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_string_empty_falsy() {
    let out = compile_and_run(r#"<?php echo "" ? "yes" : "no";"#);
    assert_eq!(out, "no");
}

// --- Bug fix: compound assignment in for-loop update ---

#[test]
fn test_for_compound_subtract() {
    let out = compile_and_run(
        r#"<?php
for ($i = 10; $i > 0; $i -= 3) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "10 7 4 1 ");
}

#[test]
fn test_for_compound_add() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 10; $i += 3) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "0 3 6 9 ");
}

#[test]
fn test_for_compound_multiply() {
    let out = compile_and_run(
        r#"<?php
for ($i = 1; $i < 100; $i *= 2) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "1 2 4 8 16 32 64 ");
}

#[test]
fn test_for_compound_shift_left() {
    let out = compile_and_run(
        r#"<?php
for ($i = 1; $i < 20; $i <<= 1) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "1 2 4 8 16 ");
}

// --- Bug fix: array push with concat expression ---

#[test]
fn test_array_push_string_to_empty() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a[] = "hello";
echo $a[0];
"#,
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_array_push_concat_expr() {
    let out = compile_and_run(
        r#"<?php
$tokens = [];
$word = "42";
$tokens[] = "NUM:" . $word;
echo $tokens[0];
"#,
    );
    assert_eq!(out, "NUM:42");
}

#[test]
fn test_many_local_vars() {
    // Issue #22: stur/ldur offset overflow with >32 local variables
    let mut php = String::from("<?php\nfunction f() {\n");
    for i in 0..50 {
        php.push_str(&format!("$v{} = {};\n", i, i));
    }
    // Sum some vars to ensure they're stored/loaded correctly
    php.push_str("echo $v0 + $v49;\n");
    php.push_str("}\nf();\n");
    let out = compile_and_run(&php);
    assert_eq!(out, "49");
}

#[test]
fn test_ref_array_assign() {
    // Issue #32: pass-by-reference array mutation via index assignment
    let out = compile_and_run(
        r#"<?php
function swap(&$a) {
    $t = $a[0];
    $a[0] = $a[1];
    $a[1] = $t;
}
$x = [1, 2];
swap($x);
echo $x[0];
echo $x[1];
"#,
    );
    assert_eq!(out, "21");
}

#[test]
fn test_ref_array_push() {
    // Issue #32: pass-by-reference array mutation via push
    let out = compile_and_run(
        r#"<?php
function append(&$arr, $val) {
    $arr[] = $val;
}
$x = [10, 20];
append($x, 30);
echo count($x);
echo $x[2];
"#,
    );
    assert_eq!(out, "330");
}

#[test]
fn test_ref_array_multi_index_write() {
    // Writing to two different computed indices of a by-ref array must not corrupt values
    let out = compile_and_run(
        r#"<?php
function write_two(&$arr, int $base, int $val1, int $val2): void {
    $arr[$base] = $val1;
    int $idx = $base + 1;
    $arr[$idx] = $val2;
}

$data = [0, 0, 0, 0, 0, 0];
write_two($data, 0, 42, 99);
echo $data[0] . "\n";
echo $data[1] . "\n";
write_two($data, 3, 77, 88);
echo $data[3] . "\n";
echo $data[4] . "\n";
"#,
    );
    assert_eq!(out, "42\n99\n77\n88\n");
}

#[test]
fn test_ref_array_stride_loop_multi_write() {
    // Reproduces DOOM showcase bug: loop over stride-3 packed array with read+write
    let out = compile_and_run(
        r#"<?php
function process(&$data, int $width): void {
    int $col = 0;
    while ($col < $width) {
        int $base = $col * 3;
        int $depthVal = $data[$base];
        if ($depthVal > 100) {
            $data[$base] = 50;
            int $idx1 = $base + 1;
            $data[$idx1] = 999;
        }
        $col += 1;
    }
}

$data = [];
int $i = 0;
while ($i < 4) {
    $data[] = 2147483647;
    $data[] = 0;
    $data[] = 599;
    $i += 1;
}
process($data, 4);
echo $data[0] . "\n";
echo $data[1] . "\n";
echo $data[2] . "\n";
echo $data[3] . "\n";
echo $data[4] . "\n";
echo $data[5] . "\n";
"#,
    );
    assert_eq!(out, "50\n999\n599\n50\n999\n599\n");
}

#[test]
fn test_ref_array_large_offset_multi_write() {
    // Regression: load_at_offset used x9 as scratch at grow_ready, clobbering the
    // array index register when the by-ref param lived at stack offset > 255.
    let out = compile_and_run(
        r#"<?php
function big(
    int $p1, int $p2, int $p3, int $p4, int $p5,
    int $p6, int $p7, int $p8, int $p9, int $p10,
    int $p11, int $p12, int $p13, int $p14, int $p15,
    int $p16, int $p17, int $p18, int $p19, int $p20,
    int $p21, int $p22, int $p23, int $p24, int $p25,
    int $p26, int $p27, int $p28, int $p29, int $p30,
    int $p31, int $p32,
    &$arr
): void {
    int $base = $p1 * 3;
    $arr[$base] = 50;
    int $idx = $base + 1;
    $arr[$idx] = 999;
    echo $p2 + $p3 + $p4 + $p5 + $p6 + $p7 + $p8 + $p9 + $p10;
    echo $p11 + $p12 + $p13 + $p14 + $p15 + $p16 + $p17 + $p18 + $p19 + $p20;
    echo $p21 + $p22 + $p23 + $p24 + $p25 + $p26 + $p27 + $p28 + $p29 + $p30;
    echo $p31 + $p32;
}
$data = [0, 0, 0, 0, 0, 0];
big(0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,$data);
echo "\n" . $data[0] . "\n" . $data[1] . "\n";
"#,
    );
    assert_eq!(out, "0000\n50\n999\n");
}

#[test]
fn test_array_column_string_implode() {
    // Issue #33: array_column on arrays of assoc arrays with string values + implode
    let out = compile_and_run(
        r#"<?php
$s = [["n" => "Alice"], ["n" => "Bob"]];
$names = array_column($s, "n");
echo implode(",", $names);
"#,
    );
    assert_eq!(out, "Alice,Bob");
}

#[test]
fn test_round_precision_1() {
    let out = compile_and_run("<?php echo round(1.55, 1);");
    assert_eq!(out, "1.6");
}

#[test]
fn test_round_precision_2() {
    let out = compile_and_run("<?php echo round(3.14159, 2);");
    assert_eq!(out, "3.14");
}

#[test]
fn test_rtrim_mask() {
    let out = compile_and_run(r#"<?php echo rtrim("hello...", ".");"#);
    assert_eq!(out, "hello");
}

#[test]
fn test_ltrim_mask() {
    let out = compile_and_run(r#"<?php echo ltrim("000123", "0");"#);
    assert_eq!(out, "123");
}

#[test]
fn test_trim_mask() {
    let out = compile_and_run(r#"<?php echo trim("**hello**", "*");"#);
    assert_eq!(out, "hello");
}

#[test]
fn test_min_three_args() {
    let out = compile_and_run("<?php echo min(3, 1, 2);");
    assert_eq!(out, "1");
}

#[test]
fn test_max_three_args() {
    let out = compile_and_run("<?php echo max(1, 3, 2);");
    assert_eq!(out, "3");
}

#[test]
fn test_min_five_args() {
    let out = compile_and_run("<?php echo min(5, 4, 3, 2, 1);");
    assert_eq!(out, "1");
}

#[test]
fn test_closure_use_int() {
    let out = compile_and_run(
        r#"<?php
$factor = 3;
$mul = function($x) use ($factor) { return $x * $factor; };
echo $mul(5);
"#,
    );
    assert_eq!(out, "15");
}

#[test]
fn test_closure_use_string() {
    let out = compile_and_run(
        r#"<?php
$greeting = "Hello";
$greet = function($name) use ($greeting) { return $greeting . " " . $name; };
echo $greet("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_closure_use_multiple() {
    let out = compile_and_run(
        r#"<?php
$a = 10;
$b = 20;
$sum = function() use ($a, $b) { return $a + $b; };
echo $sum();
"#,
    );
    assert_eq!(out, "30");
}

#[test]
fn test_closure_use_no_params() {
    let out = compile_and_run(
        r#"<?php
$name = "World";
$greet = function() use ($name) {
    echo "Hello " . $name;
};
$greet();
"#,
    );
    assert_eq!(out, "Hello World");
}

// === Memory management regression tests ===

#[test]
fn test_concat_loop_1000() {
    // Regression test for issue #21: concat buffer overflow after ~362 iterations
    let out = compile_and_run(
        r#"<?php
$s = "";
for ($i = 0; $i < 1000; $i++) {
    $s .= "x";
}
echo strlen($s);
"#,
    );
    assert_eq!(out, "1000");
}

#[test]
fn test_concat_assignment_loop_5000() {
    // Regression for x86_64 local-string cleanup: `$s = $s . "x"` must release old heap strings.
    let out = compile_and_run(
        r#"<?php
$s = "";
for ($i = 0; $i < 5000; $i++) {
    $s = $s . "x";
}
echo strlen($s);
"#,
    );
    assert_eq!(out, "5000");
}

#[test]
fn test_string_function_in_loop() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 500; $i++) {
    $x = strtolower("HELLO WORLD");
}
echo $x;
"#,
    );
    assert_eq!(out, "hello world");
}

#[test]
fn test_hash_table_computed_keys_loop() {
    // Tests that hash keys survive concat_buf reset (persisted to heap)
    let out = compile_and_run(
        r#"<?php
$h = ["init" => 0];
for ($i = 0; $i < 10; $i++) {
    $h["k" . $i] = $i;
}
echo $h["k9"];
"#,
    );
    assert_eq!(out, "9");
}

#[test]
fn test_string_reassignment_loop() {
    // Tests that old string values are freed on reassignment (free-list reuse)
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 2000; $i++) {
    $s = str_repeat("a", 100);
}
echo strlen($s);
"#,
    );
    assert_eq!(out, "100");
}

#[test]
fn test_string_variables_survive_statements() {
    // Tests that string persist works: values survive across statement boundaries
    let out = compile_and_run(
        r#"<?php
$a = "foo" . "bar";
$b = "baz" . "qux";
echo $a . $b;
"#,
    );
    assert_eq!(out, "foobarbazqux");
}

#[test]
fn test_unset_frees_string() {
    let out = compile_and_run(
        r#"<?php
$x = "hello" . " world";
echo strlen($x);
unset($x);
echo is_null($x) ? "1" : "0";
"#,
    );
    assert_eq!(out, "111");
}

#[test]
fn test_multiple_string_vars_independent() {
    // Ensure multiple string variables don't interfere after concat_buf reset
    let out = compile_and_run(
        r#"<?php
$a = "hello";
$b = "world";
$c = $a . " " . $b;
$d = strtoupper($a);
echo $c . "|" . $d;
"#,
    );
    assert_eq!(out, "hello world|HELLO");
}

#[test]
fn test_str_replace_in_loop() {
    let out = compile_and_run(
        r#"<?php
$result = "";
for ($i = 0; $i < 100; $i++) {
    $result = str_replace("x", "y", "xox");
}
echo $result;
"#,
    );
    assert_eq!(out, "yoy");
}

#[test]
fn test_array_dynamic_growth_int() {
    // Array grows beyond initial capacity via reallocation
    let out = compile_and_run(
        r#"<?php
$arr = [1, 2, 3];
for ($i = 4; $i <= 100; $i++) {
    $arr[] = $i;
}
echo count($arr) . "|" . $arr[0] . "|" . $arr[99];
"#,
    );
    assert_eq!(out, "100|1|100");
}

#[test]
fn test_array_dynamic_growth_str() {
    // String array grows beyond initial capacity
    let out = compile_and_run(
        r#"<?php
$arr = ["first"];
for ($i = 0; $i < 50; $i++) {
    $arr[] = "item" . $i;
}
echo count($arr) . "|" . $arr[0] . "|" . $arr[50];
"#,
    );
    assert_eq!(out, "51|first|item49");
}

#[test]
fn test_array_push_function_growth() {
    // array_push() triggers growth
    let out = compile_and_run(
        r#"<?php
$arr = [10];
for ($i = 0; $i < 20; $i++) {
    array_push($arr, $i * 10);
}
echo count($arr) . "|" . $arr[20];
"#,
    );
    assert_eq!(out, "21|190");
}

#[test]
fn test_array_reassign_after_function_growth() {
    let out = compile_and_run(
        r#"<?php
function grow($arr) {
    for ($i = 0; $i < 32; $i++) {
        array_push($arr, $i);
    }
    return $arr;
}

$arr = [100];
for ($j = 0; $j < 20; $j++) {
    $arr = grow($arr);
}
echo count($arr) > 100 ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_array_push_float() {
    let out = compile_and_run(
        r#"<?php
$arr = [1.1];
array_push($arr, 2.2);
echo count($arr) . "|" . $arr[1];
"#,
    );
    assert_eq!(out, "2|2.2");
}

#[test]
fn test_array_push_bool() {
    let out = compile_and_run(
        r#"<?php
$arr = [true];
array_push($arr, false);
echo count($arr);
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_array_push_object() {
    let out = compile_and_run(
        r#"<?php
class Item { public $name;
    public function __construct($n) { $this->name = $n; }
}
$items = [new Item("a")];
array_push($items, new Item("b"));
echo count($items) . "|" . $items[1]->name;
"#,
    );
    assert_eq!(out, "2|b");
}

#[test]
fn test_array_push_syntax_float() {
    // $arr[] = float syntax
    let out = compile_and_run(
        r#"<?php
$arr = [1.0];
$arr[] = 2.5;
$arr[] = 3.7;
echo count($arr) . "|" . $arr[2];
"#,
    );
    assert_eq!(out, "3|3.7");
}
