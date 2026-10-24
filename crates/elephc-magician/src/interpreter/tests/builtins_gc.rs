//! Purpose:
//! Interpreter tests for PHP Core cycle-collector builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - The fake runtime validates control state, indirect dispatch, and status value types.

use super::super::*;
use super::support::*;

/// Verifies eval GC controls and status values through direct and indirect calls.
#[test]
fn execute_program_dispatches_gc_controls_and_status() {
    let program = parse_fragment(
        br#"echo gc_enabled() ? "on" : "bad"; echo ":";
gc_disable();
echo call_user_func("gc_enabled") ? "bad" : "off"; echo ":";
call_user_func_array("gc_enable", []);
echo gc_enabled() ? "on" : "bad"; echo ":";
echo gc_collect_cycles(); echo ":";
echo gc_mem_caches(); echo ":";
$status = call_user_func("gc_status");
echo count($status); echo ":";
echo is_bool($status["running"]) && is_bool($status["protected"]) && is_bool($status["full"]) ? "bool" : "bad"; echo ":";
echo is_int($status["runs"]) && is_int($status["collected"]) && is_int($status["roots"]) ? "int" : "bad"; echo ":";
echo $status["threshold"] . ":" . $status["buffer_size"] . ":";
echo $status["application_time"] > 0.0 && $status["collector_time"] > 0.0 && $status["destructor_time"] > 0.0 && $status["free_time"] > 0.0 ? "timed" : "bad";
return function_exists("gc_status");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "on:off:on:0:0:12:bool:int:0:0:timed"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval memory builtins map `$real_usage` onto the adjacent runtime selectors.
#[test]
fn execute_program_dispatches_memory_usage_builtins() {
    let program = parse_fragment(
        br#"echo memory_get_usage(); echo ":"; echo memory_get_usage(true); echo ":";
echo call_user_func("memory_get_peak_usage"); echo ":"; echo memory_get_peak_usage(true); echo ":";
return memory_reset_peak_usage() === null;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "4096:8192:5000:9000:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
