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
echo gc_mem_caches();
"#,
    );
    assert_eq!(out, "on:off:on:12:bool:int:10001:16384:float:0");
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
