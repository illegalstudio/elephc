//! Purpose:
//! Covers native property-hash ownership of incomplete unserialized objects.
//!
//! Called from:
//! - Focused CLI runtime GC regressions.
//!
//! Key details:
//! - Unknown and disallowed class names use a reserved native object layout.
//! - A known destructor inside the same cycle must still run under protected cleanup.

use crate::support::*;

/// Incomplete property hashes participate in ordinary and exceptional cycle destruction.
#[test]
fn test_gc_incomplete_object_cycles_release_unknown_and_blocked_classes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Plain {
    public static int $destroyed = 0;
    public static bool $fail = false;
    public mixed $back = null;
    public function __destruct() {
        self::$destroyed++;
        if (self::$fail) { throw new RuntimeException('incomplete cycle cleanup'); }
    }
}
echo class_exists('Plain') ? 'known|' : 'missing|';
$value = unserialize('O:7:"Missing":1:{s:4:"self";r:1;}');
echo get_class($value), '|';
$weak = WeakReference::create($value);
unset($value);
gc_collect_cycles();
echo $weak->get() === null ? 'dead|' : 'alive|';
unset($weak);
$blocked = unserialize('O:5:"Plain":1:{s:4:"self";r:1;}', ['allowed_classes' => false]);
echo get_class($blocked), '|';
$weak = WeakReference::create($blocked);
unset($blocked);
gc_collect_cycles();
echo $weak->get() === null ? 'dead|' : 'alive|';
unset($weak);
$nested = unserialize('O:7:"Missing":1:{s:1:"c";O:5:"Plain":1:{s:4:"back";r:1;}}');
$weak = WeakReference::create($nested);
unset($nested);
gc_collect_cycles();
echo Plain::$destroyed, ':', $weak->get() === null ? 'dead|' : 'alive|';
unset($weak);
Plain::$fail = true;
$nested = unserialize('O:7:"Missing":1:{s:1:"c";O:5:"Plain":1:{s:4:"back";r:1;}}');
$weak = WeakReference::create($nested);
try { unset($nested); gc_collect_cycles(); } catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
echo Plain::$destroyed, ':', $weak->get() === null ? 'dead' : 'alive';
unset($weak, $error);
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "known|__PHP_Incomplete_Class|dead|__PHP_Incomplete_Class|dead|1:dead|incomplete cycle cleanup|2:dead", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
