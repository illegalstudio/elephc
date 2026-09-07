//! Purpose:
//! End-to-end tests for PHP Core cycle-collector builtins in the AOT backend.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the runtime GC suite.
//!
//! Key details:
//! - Cases cover direct, namespaced, first-class, and runtime string-callable dispatch.
//! - Explicit collection must remain active while automatic safe points are disabled.

use crate::support::*;

/// Compares deep cleanup after repeated eval results, without allocating loop-control temporaries.
fn assert_core_eval_collection_cleanup(setup: &str, body: &str) {
    let outstanding = |iterations| {
        let repeated = body.repeat(iterations);
        let source = format!(r#"<?php
$source = '{setup} {repeated} return 42;' . ' // ' . $argc;
echo eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "42");
        let (allocations, frees) = parse_gc_stats(&output.stderr);
        (allocations, allocations as i128 - frees as i128)
    };
    let (once_allocated, once_live) = outstanding(1);
    let (repeated_allocated, repeated_live) = outstanding(5);
    assert!(repeated_allocated > once_allocated);
    assert_eq!(repeated_live, once_live, "eval collection retained per-call storage: {body}");
}

/// Both nested constant categories and their boxed operands are released with the outer result.
#[test]
fn test_core_eval_categorized_constants_release_nested_results() {
    assert_core_eval_collection_cleanup(
        "define(\"GC_USER_PAYLOAD\", [1, 2, 3]); $categorized = true;",
        "$result = get_defined_constants($categorized); unset($result);",
    );
}

/// Declaration inventories, frame inventories, and GC status free their nested temporary cells.
#[test]
fn test_core_eval_metadata_collections_release_each_result() {
    assert_core_eval_collection_cleanup(
        "$core = \"core\";",
        "$result = get_defined_functions(); unset($result);
         $result = get_extension_funcs($core); unset($result);
         $result = get_included_files(); unset($result);
         $result = get_defined_vars(); unset($result);
         $result = gc_status(); unset($result);
         $result = debug_backtrace(); unset($result);",
    );
}

/// Repeated string-byte reads preserve their length and release no borrowed input storage.
#[test]
fn test_core_eval_extension_name_byte_views_do_not_allocate() {
    assert_core_eval_collection_cleanup(
        "$core = \"CoRe\";",
        "$result = get_extension_funcs($core); unset($result);",
    );
}

/// Native byte-view ABI results preserve the string length and leave caller variables readable.
#[test]
fn test_core_eval_extension_name_byte_views_preserve_contents() {
    let source = r#"<?php
$source = '$core = "CoRe"; $first = get_extension_funcs($core);
$second = get_extension_funcs($core); return count($first) + count($second);' . ' // ' . $argc;
echo eval($source);
"#;
    assert_eq!(compile_and_run(source), "118");
}

/// Repeating opaque eval constant inventories leaves no extra live allocations after cleanup.
#[test]
fn test_core_eval_flat_constant_inventory_releases_each_result() {
    let outstanding = |iterations: usize| {
        let source = format!(r#"<?php
$source = 'define("INVENTORY_PAYLOAD", [1, 2, 3]);
for ($iteration = 0; $iteration < {iterations}; $iteration++) {{
    $flat = get_defined_constants();
    unset($flat);
    $flat = get_defined_constants(false);
    unset($flat);
}}
return 42;' . ' // ' . $argc;
echo eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "42");
        let (allocations, frees) = parse_gc_stats(&output.stderr);
        assert!(allocations > 0, "fixture must exercise the runtime heap");
        (allocations, allocations as i128 - frees as i128)
    };
    let (once_allocated, once_live) = outstanding(1);
    let (repeated_allocated, repeated_live) = outstanding(5);
    assert!(repeated_allocated > once_allocated, "repeat must actually materialize more inventories");
    assert_eq!(repeated_live, once_live, "flat constant inventories leaked per-call storage");
}

/// Verifies GC controls and the PHP 8 status schema across callable forms.
#[test]
fn test_core_gc_controls_and_status_schema() {
    let out = compile_and_run(
        r#"<?php
namespace Demo;
echo \GC_ENABLED() ? "on" : "bad"; echo ":";
gc_disable();
echo call_user_func("gc_enabled") ? "bad" : "off"; echo ":";
$enable = gc_enable(...);
$enable();
echo gc_enabled() ? "on" : "bad"; echo ":";
$status = call_user_func("gc_status");
echo count($status); echo ":";
echo is_bool($status["running"]) && is_bool($status["protected"]) && is_bool($status["full"]) ? "bool" : "bad"; echo ":";
echo is_int($status["runs"]) && is_int($status["collected"]) && is_int($status["roots"]) ? "int" : "bad"; echo ":";
echo $status["threshold"] . ":" . $status["buffer_size"] . ":";
echo is_float($status["application_time"]) && is_float($status["collector_time"]) && is_float($status["destructor_time"]) && is_float($status["free_time"]) ? "float" : "bad"; echo ":";
echo $status["application_time"] >= 0.0 && $status["collector_time"] >= 0.0 && $status["destructor_time"] >= 0.0 && $status["free_time"] >= 0.0 ? "nonnegative" : "bad"; echo ":";
echo gc_mem_caches() >= 0 ? "cache" : "bad";
"#,
    );
    assert_eq!(out, "on:off:on:12:bool:int:0:0:float:nonnegative:cache");
}

/// Verifies explicit collection reclaims a cycle while automatic collection is disabled.
#[test]
fn test_core_gc_explicit_collection_bypasses_disable() {
    let out = compile_and_run(
        r#"<?php
class CoreGcNode { public $next = null; }
gc_disable();
$node = new CoreGcNode();
$node->next = $node;
unset($node);
$collected = gc_collect_cycles();
$status = gc_status();
echo $collected > 0 ? "collected" : "bad"; echo ":";
echo $status["runs"] > 0 ? "ran" : "bad"; echo ":";
echo $status["collected"] >= $collected ? "counted" : "bad"; echo ":";
echo gc_enabled() ? "bad" : "disabled";
gc_enable();
"#,
    );
    assert_eq!(out, "collected:ran:counted:disabled");
}

/// Verifies collector roots and phase clocks report live runtime work.
#[test]
fn test_core_gc_status_reports_live_roots_and_timings() {
    let out = compile_and_run(
        r#"<?php
class TimedGcNode {
    public $next = null;
    public function __destruct() { usleep(2000); }
}
$node = new TimedGcNode();
$node->next = $node;
$before = gc_status();
unset($node);
gc_collect_cycles();
$after = gc_status();
echo $before["roots"] > 0 ? "roots" : "bad"; echo ":";
echo $after["application_time"] >= 0.0 ? "app" : "bad"; echo ":";
echo $after["collector_time"] > 0.0 ? "collector" : "bad"; echo ":";
echo $after["destructor_time"] > 0.0 ? "destructor" : "bad"; echo ":";
echo $after["free_time"] > 0.0 ? "free" : "bad";
"#,
    );
    assert_eq!(out, "roots:app:collector:destructor:free");
}

/// Verifies cache reclamation drains small bins once and leaves reusable heap state valid.
#[test]
fn test_core_gc_mem_caches_drains_small_bins_once() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$first = str_repeat("a", 8);
$guard = str_repeat("b", 8);
unset($first);
$released = gc_mem_caches();
$again = gc_mem_caches();
$reuse = str_repeat("c", 8);
echo $released > 0 ? "released" : "bad"; echo ":";
echo $again === 0 ? "empty" : "bad"; echo ":";
echo $guard . $reuse;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "released:empty:bbbbbbbbcccccccc");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "cache drain corrupted allocator state: {}",
        out.stderr
    );
}
