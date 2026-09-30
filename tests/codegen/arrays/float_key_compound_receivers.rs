//! Purpose:
//! Regression tests for float-key deprecations on compound updates whose receiver is a
//! property, a static property, or a boxed `mixed` local array.
//!
//! Called from:
//! - `cargo test` through the codegen integration harness.
//!
//! Key details:
//! - Expected output and deprecation counts come from PHP 8.5: a compound update (`+=`, `.=`,
//!   `++`, `--`) converts its key once, while an inserting `??=` converts it in the probe and
//!   again in the insert.

use crate::support::*;

/// Counts the float-key deprecations PHP reports for `value` in a program's stderr.
fn float_key_deprecations(stderr: &str, value: &str) -> usize {
    stderr
        .matches(&format!("Implicit conversion from float {value} to int loses precision"))
        .count()
}

/// Compound assignments on a declared `array` property report each float key once.
#[test]
fn test_float_key_property_compound_assignments_warn_once_each() {
    let out = compile_and_run_capture(r#"<?php
class Box { public array $items = [1 => 10, 2 => 20, 3 => 30]; }
$box = new Box();
$box->items[1.9] += 5;
$box->items[2.5] .= "x";
echo ($box->items[3.7] *= 2), ':';
echo $box->items[1], ':', $box->items[2], ':', $box->items[3];
"#);
    assert_eq!(out.stdout, "60:15:20x:60");
    for value in ["1.9", "2.5", "3.7"] {
        assert_eq!(float_key_deprecations(&out.diagnostics, value), 1, "{value}: {}", out.diagnostics);
    }
}

/// Property increments and decrements, statement or expression, report each float key once.
#[test]
fn test_float_key_property_increment_and_decrement_warn_once_each() {
    let out = compile_and_run_capture(r#"<?php
class Box { public array $items = [1 => 10, 2 => 20, 3 => 30, 4 => 40]; }
$box = new Box();
$box->items[1.9]++;
$box->items[2.5]--;
echo $box->items[3.7]++, ':', ++$box->items[4.2], ':';
echo $box->items[1], ':', $box->items[2], ':', $box->items[3], ':', $box->items[4];
"#);
    assert_eq!(out.stdout, "30:41:11:19:31:41");
    for value in ["1.9", "2.5", "3.7", "4.2"] {
        assert_eq!(float_key_deprecations(&out.diagnostics, value), 1, "{value}: {}", out.diagnostics);
    }
}

/// Untyped hash and list properties reuse their read's key conversion on the write.
#[test]
fn test_float_key_untyped_property_arrays_warn_once_each() {
    let out = compile_and_run_capture(r#"<?php
class Box { public $map = [1 => 10]; public $list = [10, 20, 30]; }
$box = new Box();
$box->map[1.9] += 1;
$box->list[1.5]++;
echo $box->map[1], ':', $box->list[1];
"#);
    assert_eq!(out.stdout, "11:21");
    for value in ["1.9", "1.5"] {
        assert_eq!(float_key_deprecations(&out.diagnostics, value), 1, "{value}: {}", out.diagnostics);
    }
}

/// A property `??=` probes a present element once and converts an inserted key twice.
#[test]
fn test_float_key_property_null_coalesce_assignment_matches_php() {
    let out = compile_and_run_capture(r#"<?php
class Box { public array $items = [1 => 10]; }
$box = new Box();
$box->items[1.9] ??= 7;
$box->items[3.5] ??= 8;
echo $box->items[1], ':', $box->items[3];
"#);
    assert_eq!(out.stdout, "10:8");
    assert_eq!(float_key_deprecations(&out.diagnostics, "1.9"), 1, "{}", out.diagnostics);
    assert_eq!(float_key_deprecations(&out.diagnostics, "3.5"), 2, "{}", out.diagnostics);
}

/// Static-property compound updates, including `++`/`--` statements, report each key once.
#[test]
fn test_float_key_static_property_compound_updates_warn_once_each() {
    let out = compile_and_run_capture(r#"<?php
class Box {
    public static array $items = [1 => 10, 2 => 20, 3 => 30, 4 => 40];
    public static $list = [10, 20, 30];
}
Box::$items[1.9] += 5;
Box::$items[2.5]++;
Box::$items[3.7]--;
echo Box::$items[4.2]++, ':', (Box::$list[1.5] += 1), ':';
echo Box::$items[1], ':', Box::$items[2], ':', Box::$items[3], ':', Box::$items[4], ':', Box::$list[1];
"#);
    assert_eq!(out.stdout, "40:21:15:21:29:41:21");
    for value in ["1.9", "2.5", "3.7", "4.2", "1.5"] {
        assert_eq!(float_key_deprecations(&out.diagnostics, value), 1, "{value}: {}", out.diagnostics);
    }
}

/// A static-property `??=` probes a present element once and converts an inserted key twice.
#[test]
fn test_float_key_static_property_null_coalesce_assignment_matches_php() {
    let out = compile_and_run_capture(r#"<?php
class Box { public static array $items = [1 => 10]; }
Box::$items[1.9] ??= 7;
Box::$items[3.5] ??= 8;
echo Box::$items[1], ':', Box::$items[3];
"#);
    assert_eq!(out.stdout, "10:8");
    assert_eq!(float_key_deprecations(&out.diagnostics, "1.9"), 1, "{}", out.diagnostics);
    assert_eq!(float_key_deprecations(&out.diagnostics, "3.5"), 2, "{}", out.diagnostics);
}

/// Compound updates on a boxed `mixed` local array report each float key once.
#[test]
fn test_float_key_mixed_local_compound_updates_warn_once_each() {
    let out = compile_and_run_capture(r#"<?php
function rows(): mixed { return [1 => 10, 2 => 20, 3 => 30, 4 => 40]; }
$rows = rows();
$rows[1.9] += 5;
$rows[2.5]++;
echo $rows[3.7]--, ':', ($rows[4.2] .= "x"), ':';
echo $rows[1], ':', $rows[2], ':', $rows[3], ':', $rows[4];
"#);
    assert_eq!(out.stdout, "30:40x:15:21:29:40x");
    for value in ["1.9", "2.5", "3.7", "4.2"] {
        assert_eq!(float_key_deprecations(&out.diagnostics, value), 1, "{value}: {}", out.diagnostics);
    }
}

/// A `mixed` local `??=` probes a present element once and converts an inserted key twice.
#[test]
fn test_float_key_mixed_local_null_coalesce_assignment_matches_php() {
    let out = compile_and_run_capture(r#"<?php
function rows(): mixed { return [1 => 10]; }
$rows = rows();
$rows[1.9] ??= 7;
$rows[3.5] ??= 8;
echo $rows[1], ':', $rows[3];
"#);
    assert_eq!(out.stdout, "10:8");
    assert_eq!(float_key_deprecations(&out.diagnostics, "1.9"), 1, "{}", out.diagnostics);
    assert_eq!(float_key_deprecations(&out.diagnostics, "3.5"), 2, "{}", out.diagnostics);
}

/// A handler that changes the index variable cannot redirect a property or static update.
#[test]
fn test_float_key_property_update_handler_keeps_original_dimension() {
    let out = compile_and_run_capture(r#"<?php
class Box {
    public array $items = [1 => 10, 2 => 20];
    public static array $shared = [1 => 10, 2 => 20];
}
$box = new Box();
$k = 1.9;
set_error_handler(function($level, $message) use (&$k) {
    echo $level, ':', $message, '|';
    $k = 2.9;
});
$box->items[$k] += 1;
$k = 1.9;
Box::$shared[$k]++;
echo $box->items[1], ':', $box->items[2], ':', Box::$shared[1], ':', Box::$shared[2];
"#);
    assert_eq!(
        out.stdout,
        concat!(
            "8192:Implicit conversion from float 1.9 to int loses precision|",
            "8192:Implicit conversion from float 1.9 to int loses precision|",
            "11:20:11:20",
        )
    );
    assert_eq!(out.stderr, "");
}
