//! Purpose:
//! Checks enum JSON serialization across native objects and runtime-declared eval enums.
//!
//! Called from:
//! - The codegen eval integration test module.
//!
//! Key details:
//! - Runtime-selected source forces Magician instead of literal-eval AOT lowering.
//! - Native enum inputs exercise real property reads and method callback ownership.

use super::*;

/// Both native and eval-declared enum cases follow scalar and JsonSerializable rules in eval.
#[test]
fn test_dynamic_eval_json_enum_serialization() {
    let out = compile_and_run(r#"<?php
enum NativeJsonInt: int { case N = 7; }
enum NativeJsonString: string { case Hi = "hi"; }
enum NativeJsonCustom implements JsonSerializable {
    case Ready;
    public function jsonSerialize(): mixed { return "ready"; }
}
$items = [NativeJsonInt::N, NativeJsonString::Hi, NativeJsonCustom::Ready];
echo json_encode($items) . "\n";
$code = $argc > 1 ? '' : '
echo json_encode($items) . ":" . json_last_error() . "\n";
enum DynamicJsonInt: int { case N = 7; }
enum DynamicJsonString: string { case Hi = "hi"; }
enum DynamicJsonCustom implements JsonSerializable {
    case Ready;
    public function jsonSerialize(): mixed { echo "called:"; return "ready"; }
}
echo json_encode([DynamicJsonInt::N, DynamicJsonString::Hi, DynamicJsonCustom::Ready]) . ":" . json_last_error();
';
eval($code);
"#);
    assert_eq!(out, "[7,\"hi\",\"ready\"]\n[7,\"hi\",\"ready\"]:0\ncalled:[7,\"hi\",\"ready\"]:0");
}

/// Compiles one native enum self-return fixture normally and with heap debug.
fn assert_native_enum_self_return_output() {
    let source = r#"<?php
enum NativeJsonSelfUnit implements JsonSerializable {
    case Ready;
    public function jsonSerialize(): mixed { echo "self:"; return $this; }
}
enum NativeJsonSelfInt: int implements JsonSerializable {
    case N = 7;
    public function jsonSerialize(): mixed { return $this; }
}
enum NativeJsonSelfString: string implements JsonSerializable {
    case Hi = "hi";
    public function jsonSerialize(): mixed { return $this; }
}
enum NativeJsonPureUnit { case Ready; }
$cases = [NativeJsonSelfUnit::Ready, NativeJsonSelfInt::N, NativeJsonSelfString::Hi];
$code = $argc > 1 ? "" : '
foreach ($cases as $case) {
    echo json_encode($case, JSON_THROW_ON_ERROR, 1) . "\n";
    echo json_encode([$case], 0, 2) . "\n";
    echo json_encode($case, JSON_PRETTY_PRINT, 1) . "\n";
}
echo json_encode([NativeJsonPureUnit::Ready], JSON_PARTIAL_OUTPUT_ON_ERROR) . ":" . json_last_error();
';
eval($code);
"#;
    let expected = concat!(
        "self:{\"name\":\"Ready\"}\nself:[{\"name\":\"Ready\"}]\nself:{\n    \"name\": \"Ready\"\n}\n",
        "{\"name\":\"N\",\"value\":7}\n[{\"name\":\"N\",\"value\":7}]\n{\n    \"name\": \"N\",\n    \"value\": 7\n}\n",
        "{\"name\":\"Hi\",\"value\":\"hi\"}\n[{\"name\":\"Hi\",\"value\":\"hi\"}]\n{\n    \"name\": \"Hi\",\n    \"value\": \"hi\"\n}\n[0]:11",
    );
    assert_eq!(compile_and_run(source), expected);
    let debug = compile_and_run_with_heap_debug(source);
    assert!(debug.success, "{}", debug.stderr);
    assert_eq!(debug.stdout, expected);
}

/// Real bridge reads expose native enum self-return properties in PHP name/value order.
#[test]
fn test_dynamic_eval_json_enum_serializer_self_returns() {
    assert_native_enum_self_return_output();
}

/// Unoptimized EIR preserves the same enum property reads and owned-value cleanup.
#[test]
fn test_dynamic_eval_json_enum_serializer_self_returns_without_ir_opt() {
    without_ir_opt(assert_native_enum_self_return_output);
}
