//! Purpose:
//! Regression tests for PHP diagnostics when float values become array keys.
//!
//! Called from:
//! - `cargo test` through the codegen integration harness.
//!
//! Key details:
//! - Covers typed and boxed keys, exact floats, and diagnostic dispatch.

use crate::support::*;

/// Packed reads and writes each diagnose a fractional float key once.
#[test]
fn test_packed_float_array_key_read_and_write_warn_once_each() {
    let out = compile_and_run_capture("<?php $a = ['a', 'b']; echo $a[1.9]; $a[1.9] = 'x'; echo $a[1];");
    assert_eq!(out.stdout, "bx");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 2, "{}", out.stderr);
}

/// A compound packed write diagnoses its shared read and write key once.
#[test]
fn test_packed_float_array_key_compound_add_warns_once() {
    let out = compile_and_run_capture("<?php $a = [10, 20]; $a[1.9] += 1; echo $a[1];");
    assert_eq!(out.stdout, "21");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}

/// Packed post-increment diagnoses a fractional key once.
#[test]
fn test_packed_float_array_key_post_increment_warns_once() {
    let out = compile_and_run_capture("<?php $a = [10, 20]; $a[1.9]++; echo $a[1];");
    assert_eq!(out.stdout, "21");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}

/// Packed existence and coalescing probes still diagnose a fractional key.
#[test]
fn test_packed_float_array_key_probes_warn_once_each() {
    let out = compile_and_run_capture("<?php $a = [10, 20]; echo (int)isset($a[1.9]), ':', (int)empty($a[1.9]), ':', ($a[1.9] ?? 0);");
    assert_eq!(out.stdout, "1:0:20");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 3, "{}", out.stderr);
}

/// Null-coalesce assignment on packed storage diagnoses its float key once.
#[test]
fn test_packed_float_array_key_null_coalesce_assignment_warns_once() {
    let out = compile_and_run_capture("<?php $a = [10, 20]; echo ($a[1.9] ??= 7), ':', $a[1];");
    assert_eq!(out.stdout, "20:20");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}

/// Packed array writes through object and static properties diagnose float keys.
#[test]
fn test_packed_float_array_key_property_writes_warn_once_each() {
    let out = compile_and_run_capture(r#"<?php
class Box {
    public $items = [10, 20];
    public static $staticItems = [10, 20];
}
$box = new Box();
$box->items[1.9] = 21;
Box::$staticItems[1.9] = 22;
echo $box->items[1], ':', Box::$staticItems[1];
"#);
    assert_eq!(out.stdout, "21:22");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 2, "{}", out.stderr);
}

/// An integral float key remains silent on indexed storage.
#[test]
fn test_packed_integral_float_array_key_is_silent() {
    let out = compile_and_run_capture("<?php $a = ['a', 'b']; echo $a[1.0]; $a[1.0] = 'x'; echo $a[1];");
    assert_eq!(out.stdout, "bx");
    assert_eq!(out.stderr, "");
}

/// An undefined packed read diagnoses the float before warning about the integer key.
#[test]
fn test_packed_float_array_key_missing_read_warns_once() {
    let out = compile_and_run_capture("<?php $a = ['a', 'b']; var_dump($a[9.9]);");
    assert_eq!(out.stderr.matches("Implicit conversion from float 9.9 to int loses precision").count(), 1, "{}", out.stderr);
    assert_eq!(out.stderr.matches("Undefined array key 9").count(), 1, "{}", out.stderr);
}

/// A packed compound update keeps the diagnosed key even if its handler changes the variable.
#[test]
fn test_packed_float_array_key_compound_handler_keeps_original_dimension() {
    let out = compile_and_run_capture(r#"<?php
$a = [10, 20, 30];
$k = 1.9;
set_error_handler(function($level, $message) use (&$k) {
    echo $level, ':', $message, '|';
    $k = 2.9;
});
$a[$k] += 1;
echo $a[1], ':', $a[2];
"#);
    assert_eq!(out.stdout, "8192:Implicit conversion from float 1.9 to int loses precision|21:30");
    assert_eq!(out.stderr, "");
}

/// A boxed packed post-increment diagnoses its shared key once.
#[test]
fn test_packed_float_array_key_increment_handler_keeps_original_dimension() {
    let out = compile_and_run_capture(r#"<?php
$a = [10, 20, 30];
$k = 1.9;
set_error_handler(function($level, $message) use (&$k) {
    echo $level, ':', $message, '|';
    $k = 2.9;
});
$a[$k]++;
echo $a[1], ':', $a[2];
"#);
    assert_eq!(out.stdout, "8192:Implicit conversion from float 1.9 to int loses precision|21:30");
    assert_eq!(out.stderr, "");
}

/// A runtime-promoted packed array also avoids repeating its boxed float-key diagnosis.
#[test]
fn test_packed_promoted_float_array_key_compound_warns_once() {
    let out = compile_and_run_capture(r#"<?php
$a = [10, 20, 30];
$name = $argc > 0 ? 'tag' : 0;
$a[$name] = 7;
$k = 1.9;
set_error_handler(function($level, $message) use (&$k) {
    echo $level, ':', $message, '|';
    $k = 2.9;
});
$a[$k] += 1;
echo $a[1], ':', $a[2], ':', $a['tag'];
"#);
    assert_eq!(out.stdout, "8192:Implicit conversion from float 1.9 to int loses precision|21:30:7");
    assert_eq!(out.stderr, "");
}

/// Checks fractional reads and writes warn with the original float value.
#[test]
fn test_float_array_key_fractional_read_and_write_warn() {
    let out = compile_and_run_capture("<?php $a = [1 => 'x']; echo $a[1.9]; $a[-0.9] = 'y'; echo $a[0];");
    assert_eq!(out.stdout, "xy");
    assert!(out.stderr.contains("Implicit conversion from float 1.9 to int loses precision"), "{}", out.stderr);
    assert!(out.stderr.contains("Implicit conversion from float -0.9 to int loses precision"), "{}", out.stderr);
}

/// A compound hash assignment diagnoses its float key once across the read and write.
#[test]
fn test_float_array_key_compound_add_warns_once() {
    let out = compile_and_run_capture("<?php $a = [1 => 10]; $a[1.9] += 1; echo $a[1];");
    assert_eq!(out.stdout, "11");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}

/// Post-increment diagnoses a float hash key once across its read and write.
#[test]
fn test_float_array_key_post_increment_warns_once() {
    let out = compile_and_run_capture("<?php $a = [1 => 10]; $a[1.9]++; echo $a[1];");
    assert_eq!(out.stdout, "11");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}

/// Compound assignment used as an expression diagnoses the shared key once.
#[test]
fn test_float_array_key_compound_expression_warns_once() {
    let out = compile_and_run_capture("<?php $a = [1 => 10]; echo ($a[1.9] += 1), ':', $a[1];");
    assert_eq!(out.stdout, "11:11");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}

/// Post-increment used as an expression returns the old value with one key diagnostic.
#[test]
fn test_float_array_key_post_increment_expression_warns_once() {
    let out = compile_and_run_capture("<?php $a = [1 => 10]; echo $a[1.9]++, ':', $a[1];");
    assert_eq!(out.stdout, "10:11");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}

/// Prefix decrement returns the new value and diagnoses its shared key once.
#[test]
fn test_float_array_key_pre_decrement_expression_warns_once() {
    let out = compile_and_run_capture("<?php $a = [1 => 10]; echo --$a[1.9], ':', $a[1];");
    assert_eq!(out.stdout, "9:9");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}

/// Null-coalesce assignment shares its float-key diagnosis between lookup and insertion.
#[test]
fn test_float_array_key_null_coalesce_assignment_warns_once() {
    let out = compile_and_run_capture("<?php $a = ['other' => 1]; echo ($a[1.9] ??= 5), ':', $a[1];");
    assert_eq!(out.stdout, "5:5");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}

/// An error handler may change the source variable, but increment keeps its original key.
#[test]
fn test_float_array_key_increment_handler_keeps_original_dimension() {
    let out = compile_and_run_capture(r#"<?php
$a = [1 => 10, 2 => 20];
$k = 1.9;
set_error_handler(function($level, $message) use (&$k) {
    echo $level, ':', $message, '|';
    $k = 2.9;
});
$a[$k]++;
echo $a[1], ':', $a[2];
"#);
    assert_eq!(out.stdout, "8192:Implicit conversion from float 1.9 to int loses precision|11:20");
    assert_eq!(out.stderr, "");
}

/// Compound assignment also keeps the key read before its diagnostic handler ran.
#[test]
fn test_float_array_key_compound_handler_keeps_original_dimension() {
    let out = compile_and_run_capture(r#"<?php
$a = [1 => 10, 2 => 20];
$k = 1.9;
set_error_handler(function($level, $message) use (&$k) {
    echo $level, ':', $message, '|';
    $k = 2.9;
});
$a[$k] += 1;
echo $a[1], ':', $a[2];
"#);
    assert_eq!(out.stdout, "8192:Implicit conversion from float 1.9 to int loses precision|11:20");
    assert_eq!(out.stderr, "");
}

/// An expression compound assignment returns the updated value at its original key.
#[test]
fn test_float_array_key_compound_expression_handler_keeps_original_dimension() {
    let out = compile_and_run_capture(r#"<?php
$a = [1 => 10, 2 => 20];
$k = 1.9;
set_error_handler(function($level, $message) use (&$k) {
    echo $level, ':', $message, '|';
    $k = 2.9;
});
echo ($a[$k] += 1), '|', $a[1], ':', $a[2];
"#);
    assert_eq!(out.stdout, "8192:Implicit conversion from float 1.9 to int loses precision|11|11:20");
    assert_eq!(out.stderr, "");
}

/// An expression increment returns the old value while writing to its original key.
#[test]
fn test_float_array_key_increment_expression_handler_keeps_original_dimension() {
    let out = compile_and_run_capture(r#"<?php
$a = [1 => 10, 2 => 20];
$k = 1.9;
set_error_handler(function($level, $message) use (&$k) {
    echo $level, ':', $message, '|';
    $k = 2.9;
});
echo $a[$k]++, '|', $a[1], ':', $a[2];
"#);
    assert_eq!(out.stdout, "8192:Implicit conversion from float 1.9 to int loses precision|10|11:20");
    assert_eq!(out.stderr, "");
}

/// Separate source-level read and write operations each diagnose the same float key.
#[test]
fn test_float_array_key_separate_accesses_warn_twice() {
    let out = compile_and_run_capture("<?php $a = [1 => 10]; echo $a[1.9]; $a[1.9] = 12; echo $a[1];");
    assert_eq!(out.stdout, "1012");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 2, "{}", out.stderr);
}

/// A missing hash read reports one float-key deprecation before the undefined-key warning.
#[test]
fn test_float_array_key_missing_hash_read_warns_once() {
    let out = compile_and_run_capture("<?php $a = ['name' => 'x']; var_dump($a[1.9]);");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
    assert_eq!(out.stderr.matches("Undefined array key 1").count(), 1, "{}", out.stderr);
}

/// A boxed float key on a missing hash read emits one deprecation and one warning.
#[test]
fn test_float_array_key_mixed_missing_hash_read_warns_once() {
    let out = compile_and_run_capture("<?php $key = $argc > 0 ? 1.9 : 'one'; $a = ['name' => 'x']; var_dump($a[$key]);");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
    assert_eq!(out.stderr.matches("Undefined array key 1").count(), 1, "{}", out.stderr);
}

/// A missing hash entry inserted through a reference normalizes its float key once.
#[test]
fn test_float_array_key_missing_reference_warns_once() {
    let out = compile_and_run_capture("<?php $a = ['name' => 'x']; $r =& $a[1.9]; $r = 'y'; echo $a[1];");
    assert_eq!(out.stdout, "y");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}

/// A nested write does not rediagnose the key while promoting its hash entry.
#[test]
fn test_float_array_key_nested_write_warns_once() {
    let out = compile_and_run_capture("<?php $a = [1 => ['name' => 1], 'other' => 's']; $a[1.9]['x'] = 2; echo $a[1]['x'];");
    assert_eq!(out.stdout, "2");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}

/// Checks array-literal insertion reports the float key before later reads.
#[test]
fn test_float_array_literal_key_warns() {
    let out = compile_and_run_capture("<?php $a = [1.9 => 'x']; echo $a[1];");
    assert_eq!(out.stdout, "x");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}

/// Checks integral float keys and suppressed reads remain silent.
#[test]
fn test_float_array_key_exact_and_suppressed_are_silent() {
    let out = compile_and_run_capture("<?php $a = [2 => 'x']; echo $a[2.0]; echo @$a[2.9];");
    assert_eq!(out.stdout, "xx");
    assert_eq!(out.stderr, "");
}

/// Checks a dynamically typed float key emits one complete deprecation.
#[test]
fn test_float_array_key_mixed_handler_receives_complete_message() {
    let out = compile_and_run_capture(r#"<?php
set_error_handler(function($level, $message) { echo $level, ':', $message, '|'; });
$key = $argc > 0 ? 1.9 : 'one';
$a = [1 => 'x'];
echo $a[$key];
"#);
    assert_eq!(out.stdout, "8192:Implicit conversion from float 1.9 to int loses precision|x");
    assert_eq!(out.stderr, "");
}

/// Checks dynamically typed writes diagnose the key before indexing the array.
#[test]
fn test_float_array_key_mixed_write_warns_once() {
    let out = compile_and_run_capture(r#"<?php
$key = $argc > 0 ? 1.9 : 'one';
$a = [];
$a[$key] = 'x';
echo $a[1];
"#);
    assert_eq!(out.stdout, "x");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}

/// Checks both list and hash existence probes apply PHP's float-key conversion.
#[test]
fn test_float_array_key_exists_warns() {
    let out = compile_and_run_capture(r#"<?php
$list = ['a', 'b'];
$hash = ['name' => 'n', 1 => 'x'];
echo array_key_exists(1.9, $list) ? 'L' : '?';
echo array_key_exists(1.9, $hash) ? 'H' : '?';
"#);
    assert_eq!(out.stdout, "LH");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 2, "{}", out.stderr);
}

/// Checks a boxed float key follows the same existence-probe diagnostics.
#[test]
fn test_float_array_key_exists_mixed_warns() {
    let out = compile_and_run_capture(r#"<?php
$key = $argc > 0 ? 1.9 : 'one';
$a = [1 => 'x'];
echo array_key_exists($key, $a) ? 'yes' : 'no';
"#);
    assert_eq!(out.stdout, "yes");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}

/// Checks NaN and infinity receive PHP's range warning and NaN's extra deprecation.
#[test]
fn test_float_array_key_nan_and_infinity_diagnostics() {
    let out = compile_and_run_capture("<?php $a = [0 => 'z']; echo $a[NAN], $a[INF];");
    assert_eq!(out.stdout, "zz");
    assert_eq!(out.stderr.matches("The float NAN is not representable as an int, cast occurred").count(), 1, "{}", out.stderr);
    assert_eq!(out.stderr.matches("Implicit conversion from float NAN to int loses precision").count(), 1, "{}", out.stderr);
    assert_eq!(out.stderr.matches("The float INF is not representable as an int, cast occurred").count(), 1, "{}", out.stderr);
}

/// Checks range diagnostics retain PHP's full float representation on the active profile.
#[test]
fn test_float_array_key_range_warning_text() {
    let out = compile_and_run_capture(
        "<?php $a = [0 => 'z']; echo $a[1e20] ?? '?'; echo $a[-INF]; echo $a[9223372036854775808.0] ?? '?';",
    );
    assert_eq!(out.stdout, "?z?");
    assert_eq!(out.stderr.matches("The float 1.0E+20 is not representable as an int, cast occurred").count(), 1, "{}", out.stderr);
    assert_eq!(out.stderr.matches("The float -INF is not representable as an int, cast occurred").count(), 1, "{}", out.stderr);
    assert_eq!(out.stderr.matches("The float 9.223372036854776E+18 is not representable as an int, cast occurred").count(), 1, "{}", out.stderr);
}

/// Checks an existence probe diagnoses a float key even without a value read.
#[test]
fn test_float_array_key_isset_warns_once() {
    let out = compile_and_run_capture("<?php $a = [1 => 'x']; echo isset($a[1.9]) ? 'yes' : 'no';");
    assert_eq!(out.stdout, "yes");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}

/// Checks reference insertion converts its source float key only once.
#[test]
fn test_float_array_key_reference_write_warns_once() {
    let out = compile_and_run_capture("<?php $a = [1 => 'x']; $r =& $a[1.9]; $r = 'y'; echo $a[1];");
    assert_eq!(out.stdout, "y");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}

/// Checks iterator-to-array key normalization reports fractional generator keys.
#[test]
fn test_float_array_key_from_iterator_warns() {
    let out = compile_and_run_capture("<?php function g() { yield 1.9 => 'x'; } $a = iterator_to_array(g()); echo $a[1];");
    assert_eq!(out.stdout, "x");
    assert_eq!(out.stderr.matches("Implicit conversion from float 1.9 to int loses precision").count(), 1, "{}", out.stderr);
}
