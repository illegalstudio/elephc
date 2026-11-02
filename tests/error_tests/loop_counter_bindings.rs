//! Purpose:
//! Checks the checker's binding decisions for `for` initializers and reused numeric counters.
//!
//! Called from:
//! - The diagnostic integration test root.
//!
//! Key details:
//! - A `for` initializer runs unconditionally, so it binds at the statement's depth.
//! - A compatible reassignment between two different numeric types re-binds silently, for
//!   storage only, and only when the assigned value cannot borrow the old binding's cell.

use super::*;

/// Returns the local names re-bound at any recorded retype site of `result`.
fn rebound_names(result: &elephc::types::CheckResult) -> Vec<String> {
    let mut names: Vec<String> = result
        .local_retype_sites
        .values()
        .flat_map(|names| names.iter().cloned())
        .collect();
    names.sort();
    names
}

/// A `for` counter retyped after its loop warns like the same assignment written before it.
///
/// The initializer used to be checked at the loop body's conditional depth, so this valid PHP
/// failed with `cannot reassign $i from int|float to string` while the hoisted form compiled.
#[test]
fn test_for_initializer_counter_retypes_after_the_loop() {
    expect_warning(
        "<?php for ($i = 0; $i < $argc; $i++) {} $i = \"done\"; echo $i;",
        "changes type from int|float to string",
    );
}

/// A straight-line `unset` after the loop now ends the counter binding the initializer created.
#[test]
fn test_for_initializer_counter_is_killed_by_a_later_unset() {
    let result = check_source_full(
        "<?php for ($i = 0; $i < $argc; $i++) {} unset($i); $i = \"fresh\"; echo $i;",
    )
    .expect("a killed counter re-binds at any type");
    assert_eq!(result.local_bind_kill_sites.len(), 1, "{:?}", result.local_bind_kill_sites);
    assert!(
        !result.warnings.iter().any(|warning| warning.message.contains("changes type")),
        "the binding after a kill is fresh, not a retype: {:?}",
        result.warnings
    );
}

/// A counter reused by a later loop with a checked `int - int` seed gets its own binding there.
#[test]
fn test_reused_numeric_counter_records_a_silent_storage_rebind() {
    let result = check_source_full(
        "<?php function f(int $n): int { $s = 0; \
         for ($i = 0; $i < $n; $i++) { $s += 1; } \
         for ($i = $n - 1; $i >= 0; $i--) { $s += 1; } \
         return $s; } echo f($argc);",
    )
    .expect("reused counter must type-check");
    assert_eq!(rebound_names(&result), vec!["i".to_string()]);
    assert!(
        !result.warnings.iter().any(|warning| {
            warning.message.contains("changes type")
                || warning.message.contains("incompatible types")
        }),
        "a storage-only re-bind files no binding diagnostic: {:?}",
        result.warnings
    );
}

/// The storage-only re-bind is not a retype, so `--strict-locals` accepts it.
#[test]
fn test_reused_numeric_counter_compiles_under_strict_locals() {
    expect_no_error_strict(
        "<?php function f(int $n): int { $s = 0; \
         for ($i = 0; $i < $n; $i++) { $s += 1; } \
         for ($i = $n - 1; $i >= 0; $i--) { $s += 1; } \
         return $s; } echo f($argc);",
    );
}

/// No re-bind where the value may still borrow the old cell, where both bindings are already
/// boxed, where a conditional binding remains live at the join, or for a parameter, whose argument value
/// backtrace frames read from its first slot.
#[test]
fn test_numeric_storage_rebind_preserves_incoming_and_live_bindings() {
    for source in [
        // A parameter keeps one slot, so `debug_backtrace()` reports its current value.
        "<?php function f(int $n): int { $n = $n - 1; return (int) $n; } echo f($argc);",
        // A ternary may evaluate to the old binding itself.
        "<?php function f(int $n): int { for ($i = 0; $i < $n; $i++) {} \
         $i = $n > 3 ? $i : $n - 1; return (int) $i; } echo f($argc);",
        // Both bindings are `mixed`: they share a representation anyway.
        "<?php function f(int $n): int { for ($i = $n - 1; $i >= 0; $i--) {} \
         for ($i = $n - 2; $i >= 0; $i--) {} return (int) $i; } echo f($argc);",
        // A read after the conditional keeps both loop bindings shared at the join.
        "<?php function f(int $n): int { if ($n > 0) { for ($i = 0; $i < $n; $i++) {} \
         for ($i = $n - 1; $i >= 0; $i--) {} } return (int) $i; } echo f($argc);",
    ] {
        let result = check_source_full(source)
            .unwrap_or_else(|error| panic!("fixture must type-check: {}\n{source}", error.message));
        assert!(
            result.local_retype_sites.is_empty(),
            "no re-bind may be recorded: {:?} for {source}",
            result.local_retype_sites
        );
    }
}

/// Dead numeric regions inside a conditional may use separate storage without escaping the join.
#[test]
fn test_numeric_storage_rebind_accepts_dead_conditional_regions() {
    let result = check_source_full(
        "<?php function f(int $n): int { if ($n > 0) { for ($i = 0; $i < $n; $i++) {} \
         for ($i = $n - 1; $i >= 0; $i--) {} } return 0; } echo f($argc);",
    ).expect("dead conditional regions must type-check");
    assert_eq!(rebound_names(&result), vec!["i".to_owned()]);
}
