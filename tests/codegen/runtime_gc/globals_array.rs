//! Purpose:
//! Heap-debug coverage for bare `$GLOBALS` materialization and its array-typed values.
//!
//! Called from:
//! - `cargo test` through the runtime GC integration suite.
//!
//! Key details:
//! - The materialized hash owns one reference per initialized global, so each iteration must
//!   release the hash and its values, including a global array payload.
//! - The top-level global bindings themselves must also be retired at shutdown; the fixture
//!   intentionally creates loop variables so the epilogue must cover names interned later.

use crate::support::compile_and_run_with_heap_debug;

/// A materialized `$GLOBALS` snapshot keeps its values alive but leaves no heap blocks behind.
#[test]
fn test_globals_array_materialization_leaves_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1, 2, 3];
$b = "text";
for ($i = 0; $i < 3; $i++) {
    $count = count($GLOBALS);
    foreach ($GLOBALS as $name => $value) {
        if ($name === "a") {
            echo count($value), ":";
        }
    }
}
echo $count > 0 ? "count-ok\n" : "count-bad\n";
"#,
    );
    assert_eq!(out.stdout, "3:3:3:count-ok\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Runtime-created global names are stored in a growable table: each iteration creates,
/// reads, enumerates, and unsets a name that exists only at run time, and the shutdown
/// cleanup must retire the entry's value, name buffer, and block.
#[test]
fn test_runtime_only_global_names_leave_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$i = 0;
while ($i < 3) {
    $name = "g-" . $i;
    $GLOBALS[$name] = [$i, $i + 1];
    $count = count($GLOBALS[$name]);
    $seen = 0;
    foreach ($GLOBALS as $key => $value) {
        if ($key === $name) { $seen = count($value); }
    }
    echo $count, ":", $seen, ":";
    unset($GLOBALS[$name]);
    echo isset($GLOBALS[$name]) ? "set:" : "unset:";
    $i++;
}
echo "ok\n";
"#,
    );
    assert_eq!(out.stdout, "2:2:unset:2:2:unset:2:2:unset:ok\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// A call result used as a computed `$GLOBALS` key owns its string, so the store must
/// release that owner after the lookup copies the name.
#[test]
fn test_dynamic_global_call_result_key_leaves_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function makeKey(): string { return "call-key"; }
$GLOBALS[makeKey()] = [1, 2, 3];
echo count($GLOBALS["call-key"]), ":", $GLOBALS["call-key"][2], "\n";
unset($GLOBALS["call-key"]);
echo isset($GLOBALS["call-key"]) ? "set\n" : "gone\n";
"#,
    );
    assert_eq!(out.stdout, "3:3\ngone\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Literal `unset($GLOBALS['name'])` retires strings, arrays, and scalars through the
/// typed global storage path without leaving a block behind.
#[test]
fn test_literal_global_unset_releases_values_cleanly() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$text = str_repeat("x", 32);
$list = [1, 2, 3];
$number = 7;
$before = count($GLOBALS);
unset($GLOBALS["text"]);
unset($GLOBALS["list"]);
unset($GLOBALS["number"]);
echo isset($text) ? "text-set" : "text-gone", ":";
echo isset($list) ? "list-set" : "list-gone", ":";
echo isset($number) ? "number-set" : "number-gone", ":";
echo count($GLOBALS) < $before ? "count-ok" : "count-bad", "\n";
"#,
    );
    assert_eq!(out.stdout, "text-gone:list-gone:number-gone:count-ok\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// An integer computed key is coerced to the global's string name; store and unset leave
/// no scratch string or boxed value behind.
#[test]
fn test_dynamic_global_integer_key_leaves_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$i = 3;
$GLOBALS[$i] = "v";
echo $GLOBALS[3], ":", $GLOBALS[$i], "\n";
unset($GLOBALS[$i]);
echo isset($GLOBALS[3]) ? "set\n" : "gone\n";
"#,
    );
    assert_eq!(out.stdout, "v:v\ngone\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// A call result used as a computed `$GLOBALS` key on the READ path owns its string too;
/// the load must release that owner while keeping the returned value alive.
#[test]
fn test_dynamic_global_load_call_result_key_leaves_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function loadKey(): string { return "load-key"; }
$GLOBALS["load-key"] = str_repeat("v", 16);
echo strlen($GLOBALS[loadKey()]), "\n";
unset($GLOBALS["load-key"]);
echo isset($GLOBALS["load-key"]) ? "set\n" : "gone\n";
"#,
    );
    assert_eq!(out.stdout, "16\ngone\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}
