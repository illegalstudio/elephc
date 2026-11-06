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

/// Checks depth failure recovery for both native and eval-declared serializer callbacks.
fn assert_enum_json_depth_recovery() {
    let source = r#"<?php
enum NativeDepthResult implements JsonSerializable {
    case Ready;
    public function jsonSerialize(): mixed { return [[1]]; }
}
$nativeCase = NativeDepthResult::Ready;
$iterations = $argc > 1 ? (int)$argv[1] : 1;
$heapOnly = $argc > 2;
$code = $argc > 3 ? '' : '
enum DynamicDepthResult implements JsonSerializable {
    case Ready;
    public function jsonSerialize(): mixed { return [[1]]; }
}
$cases = [$nativeCase, DynamicDepthResult::Ready];
$flagOptions = $heapOnly
    ? [0, JSON_PARTIAL_OUTPUT_ON_ERROR, JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_THROW_ON_ERROR]
    : [0, JSON_THROW_ON_ERROR, JSON_PARTIAL_OUTPUT_ON_ERROR, JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_THROW_ON_ERROR];
for ($iteration = 0; $iteration < $iterations; $iteration++) {
    foreach ($cases as $case) {
        foreach ($flagOptions as $flags) {
            try {
                $result = json_encode($case, $flags, 1);
                echo ($result === false ? "false" : $result) . ":" . json_last_error() . "\n";
            } catch (JsonException $e) { echo "caught:" . $e->getCode() . "\n"; }
            echo "after:" . json_encode(9) . "\n";
        }
    }
}
';
eval($code);
echo "done";
"#;
    let one_case = concat!(
        "false:1\nafter:9\ncaught:1\nafter:9\n",
        "[[1]]:1\nafter:9\n[[1]]:1\nafter:9\n",
    );
    let one_iteration = format!("{one_case}{one_case}");
    let expected = format!("{one_iteration}done");
    assert_eq!(compile_and_run(source), expected);
    let outputs = compile_and_run_with_heap_debug_per_argv(
        source, &[&["1"], &["1", "heap"], &["9", "heap"]],
    );
    assert!(outputs[0].success, "{}", outputs[0].stderr);
    assert_eq!(outputs[0].stdout, expected);
    let heap_case = "false:1\nafter:9\n[[1]]:1\nafter:9\n[[1]]:1\nafter:9\n";
    let heap_iteration = format!("{heap_case}{heap_case}");
    for (debug, count) in outputs[1..].iter().zip([1, 9]) {
        assert!(debug.success, "{}", debug.stderr);
        assert_eq!(debug.stdout, format!("{}done", heap_iteration.repeat(count)));
    }
    // Enum declarations retain a fixed baseline. Exception retention is tested separately
    // from traversal ownership, so this repetition check uses non-throwing JSON modes.
    let live_counts = |stderr: &str| {
        let line = stderr.lines()
            .find(|line| line.starts_with("HEAP DEBUG: allocs="))
            .expect("heap counters");
        let field = |name: &str| {
            line.split_whitespace().find_map(|part| part.strip_prefix(name))
                .expect("heap field").parse::<usize>().expect("heap count")
        };
        (field("live_blocks="), field("live_bytes="))
    };
    assert_eq!(
        live_counts(&outputs[1].stderr), live_counts(&outputs[2].stderr),
        "repeated JSON failures retain additional owners\n{}\n{}",
        outputs[1].stderr, outputs[2].stderr,
    );
}

/// Runtime eval recovers from enum serializer depth errors with all JSON flag combinations.
#[test]
fn test_dynamic_eval_json_enum_depth_recovery() {
    assert_enum_json_depth_recovery();
}

/// Optimizer-disabled builds preserve the same PHP results and balanced heap ownership.
#[test]
fn test_dynamic_eval_json_enum_depth_recovery_without_ir_opt() {
    without_ir_opt(assert_enum_json_depth_recovery);
}

/// Checks native runtime-cell reads and callback errors using PHP-verified dynamic eval source.
fn assert_enum_json_callback_error_state() {
    let source = r#"<?php
$code = $argc > 1 ? '' : 'enum LaterKey implements JsonSerializable { case R; public function jsonSerialize():mixed {echo "later:";return 4;} }
enum ThrowKey implements JsonSerializable {case R;public function jsonSerialize():mixed {echo "throw:".json_last_error().":";throw new Exception("user");}}
$bad=chr(255);
foreach ([0,JSON_THROW_ON_ERROR,JSON_PARTIAL_OUTPUT_ON_ERROR,JSON_PARTIAL_OUTPUT_ON_ERROR|JSON_THROW_ON_ERROR] as $flags){
 json_encode(9);
 try{$r=json_encode([$bad=>1,"ok"=>LaterKey::R],$flags);echo ($r===false?"false":$r).":".json_last_error()."\n";}catch(JsonException $e){echo "json:".$e->getCode()."\n";}

 json_encode(9);
 $r=json_encode([$bad=>1,"a"=>INF],$flags & ~JSON_THROW_ON_ERROR);echo ($r===false?"false":$r).":".json_last_error()."\n";
 json_encode(9);
 try{$r=json_encode([[$bad=>1]],$flags,1);echo ($r===false?"false":$r).":".json_last_error()."\n";}catch(JsonException $e){echo "depth:".$e->getCode()."\n";}
 json_encode(9);
 try{$r=json_encode([$bad,LaterKey::R],$flags);echo ($r===false?"false":$r).":".json_last_error()."\n";}catch(JsonException $e){echo "value:".$e->getCode()."\n";}
}

foreach ([0,JSON_PARTIAL_OUTPUT_ON_ERROR,JSON_PARTIAL_OUTPUT_ON_ERROR|JSON_THROW_ON_ERROR] as $flags) {
 json_encode(9);
 try {json_encode([$bad=>ThrowKey::R],$flags);} catch(Exception $e) {echo "user:".$e->getMessage().":".json_last_error()."\n";}
}

enum PartialFailure implements JsonSerializable {case R;public function jsonSerialize():mixed{return [[1]];}}
enum PartialCycle implements JsonSerializable {case R;public function jsonSerialize():mixed{return [$this];}}
enum PartialThrow implements JsonSerializable {
 case R;
 public function jsonSerialize():mixed {echo "inside:".json_last_error().":";throw new Exception("user");}
}
foreach ([JSON_PARTIAL_OUTPUT_ON_ERROR,JSON_PARTIAL_OUTPUT_ON_ERROR|JSON_THROW_ON_ERROR] as $flags) {
 foreach ([1,512] as $limit) {
  json_encode(9);
  $first=$limit===1?PartialFailure::R:PartialCycle::R;
  try {json_encode([$first,PartialThrow::R],$flags,$limit);}
  catch(Exception $e) {echo $e->getMessage().":".json_last_error()."\n";}
 }
}
';
eval($code);"#;
    let expected = concat!(
        "later:false:5\n",
        "false:7\n",
        "false:1\n",
        "false:5\n",
        "later:json:5\n",
        "false:7\n",
        "depth:1\n",
        "value:5\n",
        "later:{\"\":1,\"ok\":4}:5\n",
        "{\"\":1,\"a\":0}:7\n",
        "[{\"\":1}]:1\n",
        "later:[null,4]:5\n",
        "later:{\"\":1,\"ok\":4}:5\n",
        "{\"\":1,\"a\":0}:7\n",
        "[{\"\":1}]:1\n",
        "later:[null,4]:5\n",
        "throw:0:user:user:5\n",
        "throw:0:user:user:5\n",
        "throw:0:user:user:5\n",
        "inside:0:user:1\n",
        "inside:0:user:6\n",
        "inside:0:user:1\n",
        "inside:0:user:6\n",
    );
    assert_eq!(compile_and_run(source), expected);
    let debug = compile_and_run_with_heap_debug(source);
    assert!(debug.success, "{}", debug.stderr);
    assert_eq!(debug.stdout, expected);
}

/// Invalid keys retain callback order and pending errors remain visible after user exceptions.
#[test]
fn test_dynamic_eval_json_enum_callback_error_state() {
    assert_enum_json_callback_error_state();
}

/// Unoptimized EIR preserves the same callback order, error state and exception recovery.
#[test]
fn test_dynamic_eval_json_enum_callback_error_state_without_ir_opt() {
    without_ir_opt(assert_enum_json_callback_error_state);
}

/// Compiles deep runtime containers through the real eval bridge and checks PHP lengths.
fn assert_deep_dynamic_eval_json_stack_safety() {
    let source = r#"<?php
$code = $argc > 1 ? '' : '
$array = 1;
for ($i = 0; $i < 2000; $i++) { $array = [$array]; }
foreach ([0, JSON_PARTIAL_OUTPUT_ON_ERROR, JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_FORCE_OBJECT] as $flags) {
    $result = json_encode($array, $flags, 1);
    echo ($result === false ? "false" : strlen($result)) . ":" . json_last_error() . "\n";
}
$object = 1;
for ($i = 0; $i < 2000; $i++) {
    $next = new stdClass;
    $next->next = $object;
    $object = $next;
}
foreach ([0, JSON_PARTIAL_OUTPUT_ON_ERROR] as $flags) {
    $result = json_encode($object, $flags, 1);
    echo ($result === false ? "false" : strlen($result)) . ":" . json_last_error() . "\n";
}
echo "after";
';
eval($code);
"#;
    let expected = "false:1\n4001:1\n12001:1\nfalse:1\n18001:1\nafter";
    assert_eq!(compile_and_run(source), expected);
    let debug = compile_and_run_with_heap_debug(source);
    assert!(debug.success, "{}", debug.stderr);
    assert_eq!(debug.stdout, expected);
}

/// Dynamic eval encodes deeply nested arrays and objects without native stack recursion.
#[test]
fn test_dynamic_eval_json_deep_containers_keep_native_stack_bounded() {
    assert_deep_dynamic_eval_json_stack_safety();
}

/// Optimizer-disabled builds use the same stack-safe JSON traversal.
#[test]
fn test_dynamic_eval_json_deep_containers_keep_native_stack_bounded_without_ir_opt() {
    without_ir_opt(assert_deep_dynamic_eval_json_stack_safety);
}
