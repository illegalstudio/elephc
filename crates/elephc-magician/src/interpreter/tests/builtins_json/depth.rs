//! Purpose:
//! Verifies recoverable JSON depth failures in enum serializer results.
//!
//! Called from:
//! - The parent interpreter JSON builtin test module.
//!
//! Key details:
//! - Flag combinations match real PHP, including partial output taking precedence.
//! - Owned results are released and user exceptions remain distinguishable from JSON errors.

use super::*;

/// Depth failures return false or a catchable exception, while partial output keeps the tree.
#[test]
fn execute_program_json_enum_depth_flags_recover_and_release_results() {
    let source = br#"
enum DepthResult implements JsonSerializable {
    case Ready;
    public function jsonSerialize(): mixed { return [[1]]; }
}
foreach ([0, JSON_THROW_ON_ERROR, JSON_PARTIAL_OUTPUT_ON_ERROR, JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_THROW_ON_ERROR] as $flags) {
    try {
        $result = json_encode(DepthResult::Ready, $flags, 1);
        echo ($result === false ? "false" : $result) . ":" . json_last_error() . "\n";
    } catch (JsonException $e) { echo "caught:" . $e->getCode() . "\n"; }
    echo "after:" . json_encode(9) . "\n";
}
"#;
    let program = parse_fragment(source).expect("parse depth flag matrix");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("recover from JSON depth errors");
    assert_eq!(values.output, concat!(
        "false:1\nafter:9\ncaught:1\nafter:9\n",
        "[[1]]:1\nafter:9\n[[1]]:1\nafter:9\n",
    ));
    let returned = values.values.iter().filter_map(|(id, value)| {
        let FakeValue::Array(children) = value else { return None; };
        let first = children.first()?;
        matches!(values.get(*first), FakeValue::Array(_)).then_some(*id)
    }).collect::<Vec<_>>();
    assert_eq!(returned.len(), 4);
    for id in returned {
        assert_eq!(values.cell_owners[&id], 0, "owned serializer result {id}");
    }
}

/// Replacement containers consume their own depth, including empty arrays and public objects.
#[test]
fn execute_program_json_enum_depth_container_boundaries() {
    for (body, depth, expected) in [
        ("7", 1, "7:0"),
        ("[1]", 1, "[1]:0"),
        ("[[1]]", 1, "false:1"),
        ("[[1]]", 2, "[[1]]:0"),
        ("[[]]", 1, "false:1"),
        ("[\"key\" => [1]]", 1, "false:1"),
        ("json_decode(\"{\\\"key\\\":[1]}\")", 1, "false:1"),
        ("json_decode(\"{\\\"key\\\":[1]}\")", 2, "{\"key\":[1]}:0"),
    ] {
        let source = format!(r#"
enum BoundaryResult implements JsonSerializable {{
    case Ready;
    public function jsonSerialize(): mixed {{ return {body}; }}
}}
$result = json_encode(BoundaryResult::Ready, 0, {depth});
echo ($result === false ? "false" : $result) . ":" . json_last_error();
"#);
        let program = parse_fragment(source.as_bytes()).expect("parse depth boundary");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        execute_program(&program, &mut scope, &mut values).expect("encode depth boundary");
        assert_eq!(values.output, expected, "{source}");
    }
}

/// Nonpartial failures stop later callbacks while partial output continues through siblings.
#[test]
fn execute_program_json_enum_depth_callback_order() {
    let program = parse_fragment(br#"
enum DeepResult implements JsonSerializable {
    case Ready;
    public function jsonSerialize(): mixed { return [[1]]; }
}
enum LaterResult implements JsonSerializable {
    case Ready;
    public function jsonSerialize(): mixed { echo "later:"; return 2; }
}
foreach ([0, JSON_THROW_ON_ERROR, JSON_PARTIAL_OUTPUT_ON_ERROR, JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_THROW_ON_ERROR] as $flags) {
    try {
        $result = json_encode([DeepResult::Ready, LaterResult::Ready], $flags, 1);
        echo ($result === false ? "false" : $result) . ":" . json_last_error() . "\n";
    } catch (JsonException $e) { echo "caught:" . $e->getCode() . "\n"; }
}
"#).expect("parse depth callback order");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("preserve callback order");
    assert_eq!(values.output,
        "false:1\ncaught:1\nlater:[[[1]],2]:1\nlater:[[[1]],2]:1\n");
}

/// Child UTF-8 failures precede depth unless partial output reaches the container exit.
#[test]
fn execute_program_json_enum_depth_error_precedence() {
    let program = parse_fragment(br#"
enum InvalidResult implements JsonSerializable {
    case Ready;
    public function jsonSerialize(): mixed { return [[chr(255)]]; }
}
foreach ([0, JSON_PARTIAL_OUTPUT_ON_ERROR, JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_THROW_ON_ERROR] as $flags) {
    $result = json_encode(InvalidResult::Ready, $flags, 1);
    echo ($result === false ? "false" : $result) . ":" . json_last_error() . "\n";
}
"#).expect("parse JSON error precedence");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("preserve JSON error precedence");
    assert_eq!(values.output, "false:5\n[[null]]:1\n[[null]]:1\n");
}

/// User exceptions after a partial depth failure escape as their original throwable.
#[test]
fn execute_program_json_enum_depth_does_not_mask_user_exceptions() {
    let program = parse_fragment(br#"
enum PartialDepthResult implements JsonSerializable {
    case Ready;
    public function jsonSerialize(): mixed { return [[1]]; }
}
enum ThrowingDepthResult implements JsonSerializable {
    case Ready;
    public function jsonSerialize(): mixed {
        echo "inside:" . json_last_error() . ":";
        throw new Exception("user");
    }
}
try {
    json_encode([PartialDepthResult::Ready, ThrowingDepthResult::Ready], JSON_PARTIAL_OUTPUT_ON_ERROR, 1);
} catch (JsonException $e) { echo "wrong"; }
catch (Exception $e) { echo $e->getMessage() . ":" . json_last_error() . ":"; }
echo json_encode(3);
"#).expect("parse user exception after partial JSON failure");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("recover from user exception");
    assert_eq!(values.output, "inside:0:user:1:3");
}

/// Recursive object results produce JSON recursion errors instead of unbounded traversal.
#[test]
fn execute_program_json_enum_depth_recursive_object_result() {
    let program = parse_fragment(br#"
enum RecursiveObjectResult implements JsonSerializable {
    case Ready;
    public function jsonSerialize(): mixed {
        $object = new stdClass();
        $object->self = $object;
        return $object;
    }
}
foreach ([0, JSON_THROW_ON_ERROR, JSON_PARTIAL_OUTPUT_ON_ERROR, JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_THROW_ON_ERROR] as $flags) {
    try {
        $result = json_encode(RecursiveObjectResult::Ready, $flags, 1);
        echo ($result === false ? "false" : $result) . ":" . json_last_error() . "\n";
    } catch (JsonException $e) { echo "caught:" . $e->getCode() . "\n"; }
}
"#).expect("parse recursive object serializer");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("recover from recursive objects");
    assert_eq!(values.output, "false:6\ncaught:6\n{\"self\":null}:6\n{\"self\":null}:6\n");
}

/// A cyclic array read retains its owner while partial output replaces its recursive edge.
#[test]
fn execute_program_json_enum_depth_recursive_array_payload() {
    for flags in [0, 512, 512 | 4194304] {
        let mut context = ElephcEvalContext::new();
        let mut values = FakeOps::default();
        let array = values.alloc(FakeValue::Array(Vec::new()));
        values.values.insert(array.as_ptr() as usize, FakeValue::Array(vec![array]));
        let flag_value = values.int(flags).expect("allocate JSON flags");
        let depth = values.int(1).expect("allocate JSON depth");
        let result = eval_json_encode_values_result(&[array, flag_value, depth], &mut context, &mut values)
            .expect("recover from recursive array payload");
        assert_eq!(context.json_last_error(), 6);
        if flags == 0 {
            assert_eq!(values.get(result), FakeValue::Bool(false));
        } else {
            assert_eq!(values.get(result), FakeValue::String("[null]".to_string()));
        }
        assert_eq!(values.cell_owners[&(array.as_ptr() as usize)], 1);
    }
}

/// A cleanup exception after a JSON depth failure keeps its runtime status.
#[test]
fn execute_program_json_enum_depth_cleanup_failure_is_not_json() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let element = values.int(1).expect("allocate child value");
    let inner = values.alloc(FakeValue::Array(vec![element]));
    let outer = values.alloc(FakeValue::Array(vec![inner]));
    let flags = values.int(0).expect("allocate JSON flags");
    let depth = values.int(1).expect("allocate JSON depth");
    // Skip flag/depth conversions and the scalar read; fail the inner array cleanup.
    values.fail_release_call = Some(3);
    let result = eval_json_encode_values_result(&[outer, flags, depth], &mut context, &mut values);
    assert!(matches!(result, Err(EvalStatus::UncaughtThrowable)));
    assert_eq!(values.releases.last(), Some(&inner));
    assert_eq!(context.json_last_error(), 1);
}
