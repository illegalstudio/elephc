//! Purpose:
//! Checks that eval (Magician) buffers follow the compiled buffer liveness contract end to end.
//!
//! Called from:
//! - The codegen eval integration test module.
//!
//! Key details:
//! - The fragments call buffer builtins, which keeps literal eval on the Magician bridge.
//! - A freed eval buffer, or any alias of it, must abort with the same use-after-free fatal
//!   compiled `buffer<T>` handles print, and never reach released or reused memory.

use super::*;

/// The fatal compiled code and eval print when a freed buffer handle is used.
const USE_AFTER_FREE_FATAL: &str = "Fatal error: use of buffer after buffer_free()";

/// Verifies `buffer_len()` of an alias of a freed eval buffer aborts with the use-after-free
/// fatal instead of reading the released allocation.
#[test]
fn test_eval_buffer_stale_alias_len_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('$buf = buffer_new(4);
$alias = $buf;
echo buffer_len($alias) . ":";
buffer_free($buf);
echo buffer_len($alias);
echo "unreachable";');
"#,
    );
    assert!(err.contains("4:"), "{err}");
    assert!(err.contains(USE_AFTER_FREE_FATAL), "{err}");
    assert!(!err.contains("unreachable"), "{err}");
}

/// Verifies a stale eval alias stays dead after a new buffer reuses its registry slot and, with
/// the allocator's size-class reuse, its payload address: the new buffer keeps working while
/// `buffer_ptr()` of the alias aborts instead of reaching the new payload.
#[test]
fn test_eval_buffer_stale_alias_stays_dead_after_reuse() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('$first = buffer_new(2);
$stale = $first;
buffer_free($first);
$second = buffer_new(2);
ptr_set(buffer_ptr($second), 7);
echo buffer_len($second) . ":" . ptr_get(buffer_ptr($second)) . ":";
echo ptr_get(buffer_ptr($stale));
echo "unreachable";');
"#,
    );
    assert!(err.contains("2:7:"), "{err}");
    assert!(err.contains(USE_AFTER_FREE_FATAL), "{err}");
    assert!(!err.contains("unreachable"), "{err}");
}

/// Verifies freeing a nulled eval local again is a no-op, as in compiled code, and that other
/// buffers' frees and slot reuse leave a live eval buffer's length and payload intact.
#[test]
fn test_eval_buffer_double_free_of_local_is_noop_and_live_buffers_survive() {
    let out = compile_and_run(
        r#"<?php
echo eval('$keep = buffer_new(3);
ptr_set(ptr_offset(buffer_ptr($keep), 16), 42);
$gone = buffer_new(2);
buffer_free($gone);
buffer_free($gone);
$other = buffer_new(5);
$stale = $other;
buffer_free($other);
$fresh = buffer_new(1);
$result = buffer_len($keep) . ":" . ptr_get(ptr_offset(buffer_ptr($keep), 16)) . ":"
    . ($gone === null ? "nulled" : "bad") . ":" . buffer_len($fresh);
buffer_free($fresh);
buffer_free($keep);
return $result;');
"#,
    );
    assert_eq!(out, "3:42:nulled:1");
}
