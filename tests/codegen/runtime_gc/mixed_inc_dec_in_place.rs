//! Purpose:
//! Exercises the unique-cell in-place fast path for boxed `++`/`--` and its COW fallback.
//!
//! Called from:
//! - The codegen integration suite through `runtime_gc`.
//!
//! Key details:
//! - A copied cell has refcount 2 and must keep the copy unchanged.
//! - Overflowing int increments promote to float through the generic path.
//! - Heap poisoning verifies the in-place result retains exactly one owner.

use crate::support::compile_and_run_with_heap_debug;

/// Unique int cells update in place; aliases, post/pre forms and overflow stay PHP-exact.
#[test]
fn test_mixed_inc_dec_in_place_and_cow_fallback() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$i = 5;
$a = $i;
$i++;
echo $i, '|', $a, '|';
$j = $i++;
echo $i, '|', $j, '|';
$k = ++$i;
echo $i, '|', $k, '|';
$m = PHP_INT_MAX;
$m++;
echo gettype($m), ':', $m, '|';
$n = PHP_INT_MIN;
$n--;
echo gettype($n), ':', $n, '|';
$f = 1.5;
$f++;
echo gettype($f), ':', $f, '|';
$s = 'az';
$s++;
echo $s, '|';
$r = 10;
$q = &$r;
$r++;
echo $r, '|', $q, '|';
$seen = 0;
$inc = function() use (&$seen): void { ++$seen; };
$inc(); $inc(); $inc();
echo $seen, '|';
$bound = $argc + 99;
$sum = 0;
for ($x = 1; $x <= $bound; $x++) { $sum += $x; }
echo $sum, "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "6|5|7|6|8|8|double:9.2233720368548E+18|double:-9.2233720368548E+18|double:2.5|ba|11|11|3|5050\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}
