//! Purpose:
//! Interpreter tests for eval raw pointer and buffer extension builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Eval buffers are opaque registry handles, so payload reads and writes go through
//!   `buffer_ptr()`; a freed handle or a stale alias must die with the compiled-code
//!   use-after-free fatal, which the fake runtime records as a warning.
//! - The buffer registry is thread-local and every test runs on a fresh thread (a fresh
//!   process under nextest), so the slot reuse a test provokes is not disturbed by others.

use super::super::*;
use super::support::*;

/// Verifies pointer probes, type sizes, and callable metadata for raw-memory builtins.
#[test]
fn execute_program_dispatches_pointer_null_offset_and_sizeof_builtins() {
    let program = parse_fragment(
        br#"class EvalPtrSizeBox {
    public $x;
    public $y;
    public static $z;
}
$p = ptr_null();
echo ptr_is_null($p) ? "N" : "bad";
echo ":" . ptr_is_null(ptr_offset($p, 0));
echo ":" . ptr_sizeof("int");
echo ":" . ptr_sizeof("string");
echo ":" . ptr_sizeof("ptr");
echo ":" . ptr_sizeof("EvalPtrSizeBox");
echo ":" . function_exists("ptr_read16") . function_exists("buffer_new");
return is_callable("ptr_write_string");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "N:1:8:16:8:40:11");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval can allocate buffers, take their payload address with `buffer_ptr()`, and use
/// pointer memory helpers on that payload.
#[test]
fn execute_program_dispatches_buffer_and_pointer_memory_builtins() {
    let program = parse_fragment(
        br#"$buf = buffer_new(4);
$payload = buffer_ptr($buf);
echo buffer_len($buf) . ":" . ($payload !== $buf && $payload === buffer_ptr($buf) ? "payload" : "bad") . ":";
ptr_set($payload, 123456789);
echo ptr_get($payload) . ":";
ptr_write8($payload, 255);
ptr_write8(ptr_offset($payload, 1), 1);
echo ptr_read8($payload) . "," . ptr_read8(ptr_offset($payload, 1)) . ":";
call_user_func_array("ptr_write16", ["pointer" => $payload, "value" => 4660]);
echo ptr_read16($payload) . ":";
ptr_write32($payload, 305419896);
echo ptr_read32($payload) . ":";
$written = ptr_write_string($payload, "GET /");
echo $written . ":" . ptr_read_string($payload, $written) . ":";
echo strlen(ptr_read_string($payload, 0));
buffer_free($buf);
echo ":" . (ptr_is_null($buf) ? "freed" : "live");
return function_exists("buffer_free");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "4:payload:123456789:255,1:4660:305419896:5:GET /:0:freed"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies `ptr($var)` remains rejected until eval has an lvalue-aware pointer path.
#[test]
fn execute_program_rejects_ptr_lvalue_builtin_without_storage_address() {
    let program = parse_fragment(br#"$value = 1; return ptr($value);"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values).expect_err("reject ptr lvalue");

    assert_eq!(err, EvalStatus::UnsupportedConstruct);
}

/// The diagnostic compiled code prints when a freed buffer handle is used.
const USE_AFTER_FREE_FATAL: &str = "Fatal error: use of buffer after buffer_free()\n";

/// Runs one fragment that must stop at a freed buffer, then checks the output printed before
/// the fatal and that the use-after-free diagnostic is the only one reported.
fn assert_freed_buffer_use_is_fatal(source: &[u8], expected_output: &str) {
    let program = parse_fragment(source).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program(&program, &mut scope, &mut values)
        .expect_err("a freed buffer use must fail closed");

    assert_eq!(err, EvalStatus::RuntimeFatal);
    assert_eq!(values.output, expected_output);
    assert_eq!(values.warnings, [USE_AFTER_FREE_FATAL]);
}

/// Verifies `buffer_len()` of a freed local, which `buffer_free()` nulled, dies with the
/// compiled-code use-after-free fatal.
#[test]
fn execute_program_rejects_buffer_len_of_freed_local() {
    assert_freed_buffer_use_is_fatal(
        br#"$buf = buffer_new(4);
echo buffer_len($buf) . ":";
buffer_free($buf);
echo buffer_len($buf);
return true;"#,
        "4:",
    );
}

/// Verifies `buffer_len()` of an alias of a freed buffer dies with the use-after-free fatal
/// instead of reading the released allocation.
#[test]
fn execute_program_rejects_buffer_len_of_freed_alias() {
    assert_freed_buffer_use_is_fatal(
        br#"$buf = buffer_new(4);
$alias = $buf;
echo buffer_len($alias) . ":";
buffer_free($buf);
echo buffer_len($alias);
return true;"#,
        "4:",
    );
}

/// Verifies `buffer_ptr()` of an alias of a freed buffer dies with the use-after-free fatal
/// instead of handing out the released payload address.
#[test]
fn execute_program_rejects_buffer_ptr_of_freed_alias() {
    assert_freed_buffer_use_is_fatal(
        br#"$buf = buffer_new(2);
$alias = $buf;
echo (buffer_ptr($alias) === buffer_ptr($buf) ? "same" : "bad") . ":";
buffer_free($buf);
echo buffer_ptr($alias);
return true;"#,
        "same:",
    );
}

/// Verifies `buffer_free()` of an alias of a freed buffer dies with the use-after-free fatal,
/// as in compiled code, instead of releasing the allocation a second time.
#[test]
fn execute_program_rejects_buffer_free_of_freed_alias() {
    assert_freed_buffer_use_is_fatal(
        br#"$buf = buffer_new(2);
$alias = $buf;
buffer_free($buf);
echo "freed:";
buffer_free($alias);
return true;"#,
        "freed:",
    );
}

/// Verifies a stale alias stays dead after a new buffer reuses its registry slot (and, with
/// the allocator's usual size-class reuse, its payload address): the new buffer works and has
/// a different handle, while the alias still fails closed instead of reaching the new payload.
#[test]
fn execute_program_rejects_stale_alias_after_buffer_reuse() {
    assert_freed_buffer_use_is_fatal(
        br#"$first = buffer_new(2);
$stale = $first;
buffer_free($first);
$second = buffer_new(2);
ptr_set(buffer_ptr($second), 7);
echo buffer_len($second) . ":" . ptr_get(buffer_ptr($second)) . ":";
echo ($second === $stale ? "same" : "fresh") . ":";
echo ptr_get(buffer_ptr($stale));
return true;"#,
        "2:7:fresh:",
    );
}

/// Verifies element syntax never reaches eval buffer memory, even for a freed buffer: an eval
/// buffer is an integer handle, so a read gives null and a write throws like on any integer.
#[test]
fn execute_program_element_syntax_on_freed_buffer_alias_never_touches_memory() {
    let program = parse_fragment(
        br#"$buf = buffer_new(2);
$alias = $buf;
buffer_free($buf);
echo $alias[0] === null ? "read-null" : "bad";
try {
    $alias[0] = 5;
    echo ":bad";
} catch (Error $e) {
    echo ":" . $e->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "read-null:Cannot use a scalar value as an array");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies freeing an already nulled local is a no-op, as in compiled code, and that frees of
/// other buffers, a stale alias left untouched, and slot reuse leave a live buffer intact.
#[test]
fn execute_program_keeps_live_buffers_working_across_unrelated_frees() {
    let program = parse_fragment(
        br#"$keep = buffer_new(3);
ptr_set(ptr_offset(buffer_ptr($keep), 16), 42);
$gone = buffer_new(2);
buffer_free($gone);
buffer_free($gone);
$other = buffer_new(5);
$stale = $other;
buffer_free($other);
$fresh = buffer_new(1);
echo buffer_len($keep) . ":" . ptr_get(ptr_offset(buffer_ptr($keep), 16)) . ":";
echo ($gone === null ? "nulled" : "bad") . ":" . buffer_len($fresh);
buffer_free($fresh);
buffer_free($keep);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:42:nulled:1");
    assert!(values.warnings.is_empty(), "{:?}", values.warnings);
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
