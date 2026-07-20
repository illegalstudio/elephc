//! Purpose:
//! Interpreter tests for eval raw pointer and buffer extension builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Buffer tests use the AOT-shaped header pointer and offset by 16 bytes for
//!   payload pointer reads and writes.

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

/// Verifies eval can allocate buffers and use pointer memory helpers on their payload.
#[test]
fn execute_program_dispatches_buffer_and_pointer_memory_builtins() {
    let program = parse_fragment(
        br#"$buf = buffer_new(4);
$payload = ptr_offset($buf, 16);
echo buffer_len($buf) . ":";
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
        "4:123456789:255,1:4660:305419896:5:GET /:0:freed"
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
