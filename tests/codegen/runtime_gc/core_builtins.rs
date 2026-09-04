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
