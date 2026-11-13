//! Purpose:
//! Checks growable print_r capture ownership across literal and runtime return flags.
//!
//! Called from:
//! - Focused runtime GC codegen CI on Linux and macOS.
//!
//! Key details:
//! - Large captures exceed the former static buffer while stdout stays bounded by ob_*.

use crate::support::*;

/// Every print_r mode preserves more than 64 KiB and releases its raw buffer and final string.
#[test]
fn test_print_r_growable_capture_heap_debug() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$large = str_repeat('x', 70000);
$returned = print_r($large, true);
echo strlen($returned), ':', (int)($returned === $large), '|';
ob_start(); $result = print_r($large); $echoed = ob_get_clean();
echo strlen($echoed), ':', (int)($echoed === $large), ':', (int)$result, '|';
$flag = $argc > 0;
$returned = print_r($large, $flag);
echo strlen($returned), ':', (int)($returned === $large), '|';
$flag = $argc < 0;
ob_start(); $result = print_r($large, $flag); $echoed = ob_get_clean();
echo strlen($echoed), ':', (int)($echoed === $large), ':', (int)($result === true), '|';
echo strlen(print_r(null, true)), ':', strlen(print_r(false, true)), '|';
$number = $argc > 0 ? 1.25 : 2.5;
echo print_r($number, true);
unset($large, $returned, $echoed, $result, $flag, $number);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "70000:1|70000:1:1|70000:1|70000:1:1|0:0|1.25");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
