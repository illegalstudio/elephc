//! Purpose:
//! Interpreter tests for core and JSON builtin dispatch.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases assert JSON state, flags, throwable behavior, and callable dispatch.

use super::super::*;
use super::support::*;

/// Verifies dynamic builtin calls inside eval dispatch through runtime value hooks.
#[test]
fn execute_program_dispatches_simple_builtins() {
    let program = parse_fragment(
        br#"echo strlen("abc") . ":" . count([1, [2, 3], [4]]) . ":";
echo count([1, [2, 3], [4]], COUNT_RECURSIVE) . ":";
echo call_user_func("count", [1, [2]]) . ":";
echo call_user_func_array("count", ["value" => [1, [2]], "mode" => COUNT_RECURSIVE]) . ":";
return defined("COUNT_RECURSIVE");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "3:3:6:2:3:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `count()` dispatches to eval-declared `Countable` objects.
#[test]
fn execute_program_counts_eval_countable_objects() {
    let program = parse_fragment(
        br#"class EvalCountableBag implements Countable {
    private int $n;
    public function __construct($n) { $this->n = $n; }
    public function count(): int { echo "count:"; return $this->n; }
}
$bag = new EvalCountableBag(4);
echo count($bag); echo ":";
echo count($bag, COUNT_RECURSIVE); echo ":";
echo call_user_func_array("count", ["value" => $bag]);
return function_exists("count");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "count:4:count:4:count:4");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `json_encode()` serializes scalar, indexed, and associative values.
#[test]
fn execute_program_dispatches_json_encode_builtin() {
    let program = parse_fragment(
            br#"echo json_encode(["a" => 1, "b" => "x/y"]) . ":";
echo json_encode([1, "q", true, null]) . ":";
echo call_user_func("json_encode", "a/b\"c") . ":";
echo call_user_func_array("json_encode", ["value" => ["k" => false], "flags" => 0, "depth" => 4]) . ":";
echo json_encode("a/b", JSON_UNESCAPED_SLASHES) . ":";
echo call_user_func_array("json_encode", ["value" => "x/y", "flags" => JSON_UNESCAPED_SLASHES]) . ":";
$accent = json_decode("\"\\u00e9\"");
$emoji = json_decode("\"\\ud83d\\ude00\"");
echo bin2hex(json_encode($accent . "/" . $emoji)) . ":";
echo bin2hex(json_encode($accent . "/" . $emoji, JSON_UNESCAPED_UNICODE)) . ":";
echo bin2hex(json_encode([$accent => $emoji])) . ":";
echo bin2hex(json_encode([$accent => $emoji], JSON_UNESCAPED_UNICODE)) . ":";
echo json_encode([1, 2], JSON_FORCE_OBJECT) . ":";
echo json_encode([], JSON_FORCE_OBJECT) . ":";
echo call_user_func_array("json_encode", ["value" => [1, 2], "flags" => JSON_FORCE_OBJECT]) . ":";
echo json_encode("<>&\"" . chr(39), JSON_HEX_TAG | JSON_HEX_AMP | JSON_HEX_APOS | JSON_HEX_QUOT) . ":";
echo json_encode(["01", "+12", "1e3", " 7", "7x"], JSON_NUMERIC_CHECK) . ":";
echo json_encode([1.0, 2.5, -3.0], JSON_PRESERVE_ZERO_FRACTION) . ":";
echo (json_encode(INF) === false ? "false" : "json") . ":";
echo json_last_error() . ":" . json_last_error_msg() . ":";
echo json_encode([1.5, INF, NAN], JSON_PARTIAL_OUTPUT_ON_ERROR) . ":";
echo json_last_error() . ":" . json_last_error_msg() . ":";
$bad = "a" . hex2bin("80") . "b";
echo (json_encode($bad) === false ? "utf8-false" : "bad") . ":";
echo json_last_error() . ":";
echo bin2hex(json_encode($bad, JSON_PARTIAL_OUTPUT_ON_ERROR)) . ":";
echo json_last_error() . ":";
echo json_encode($bad, JSON_INVALID_UTF8_IGNORE) . ":";
echo json_last_error() . ":";
echo bin2hex(json_encode($bad, JSON_INVALID_UTF8_SUBSTITUTE)) . ":";
echo json_last_error() . ":";
echo bin2hex(json_encode($bad, JSON_INVALID_UTF8_SUBSTITUTE | JSON_UNESCAPED_UNICODE)) . ":";
echo json_last_error() . ":";
echo json_encode([hex2bin("6b80") => hex2bin("7680")], JSON_PARTIAL_OUTPUT_ON_ERROR) . ":";
echo json_last_error() . ":";
json_encode(3.5);
echo json_last_error() . ":" . json_last_error_msg() . ":";
echo str_replace("\n", "|", json_encode(["a" => [1, 2]], JSON_PRETTY_PRINT)) . ":";
return function_exists("json_encode") && defined("INF") && defined("NAN") && defined("JSON_UNESCAPED_SLASHES") && defined("JSON_UNESCAPED_UNICODE") && defined("JSON_FORCE_OBJECT") && defined("JSON_HEX_TAG") && defined("JSON_HEX_AMP") && defined("JSON_HEX_APOS") && defined("JSON_HEX_QUOT") && defined("JSON_NUMERIC_CHECK") && defined("JSON_PARTIAL_OUTPUT_ON_ERROR") && defined("JSON_PRETTY_PRINT") && defined("JSON_PRESERVE_ZERO_FRACTION") && defined("JSON_INVALID_UTF8_IGNORE") && defined("JSON_INVALID_UTF8_SUBSTITUTE");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        r#"{"a":1,"b":"x\/y"}:[1,"q",true,null]:"a\/b\"c":{"k":false}:"a/b":"x/y":225c75303065395c2f5c75643833645c756465303022:22c3a95c2ff09f988022:7b225c7530306539223a225c75643833645c7564653030227d:7b22c3a9223a22f09f9880227d:{"0":1,"1":2}:{}:{"0":1,"1":2}:"\u003C\u003E\u0026\u0022\u0027":[1,12,1000,7,"7x"]:[1.0,2.5,-3.0]:false:7:Inf and NaN cannot be JSON encoded:[1.5,0,0]:7:Inf and NaN cannot be JSON encoded:utf8-false:5:6e756c6c:5:"ab":0:22615c75666666646222:0:2261efbfbd6222:0:{"k\ufffd":null}:5:0:No error:{|    "a": [|        1,|        2|    ]|}:"#
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `json_decode()` materializes scalars, arrays, and associative arrays.
#[test]
fn execute_program_dispatches_json_decode_builtin() {
    let program = parse_fragment(
            br#"echo json_decode("\"hello\"") . ":";
echo json_decode("42") . ":";
echo (json_decode("true") ? "T" : "bad") . ":";
echo (is_null(json_decode("null")) ? "NULL" : "bad") . ":";
$decoded = json_decode("{\"a\":1,\"b\":[\"x\",false]}", true);
echo $decoded["a"] . ":" . $decoded["b"][0] . ":" . ($decoded["b"][1] ? "bad" : "F") . ":";
$call = call_user_func("json_decode", "[3,4]");
echo $call[1] . ":";
$named = call_user_func_array("json_decode", ["json" => "{\"k\":\"v\"}", "associative" => true, "depth" => 4, "flags" => 0]);
echo $named["k"] . ":";
$badJson = "\"a" . hex2bin("80") . "b\"";
echo (is_null(json_decode($badJson)) ? "utf8-null" : "bad") . ":";
echo json_last_error() . ":";
echo bin2hex(json_decode($badJson, true, 512, JSON_INVALID_UTF8_IGNORE)) . ":";
echo json_last_error() . ":";
echo bin2hex(json_decode($badJson, true, 512, JSON_INVALID_UTF8_SUBSTITUTE)) . ":";
echo json_last_error() . ":";
$objSub = json_decode("{\"k" . hex2bin("80") . "\":\"v" . hex2bin("80") . "\"}", true, 512, JSON_INVALID_UTF8_SUBSTITUTE);
$objSubKeys = array_keys($objSub);
echo bin2hex($objSubKeys[0]) . "=" . bin2hex($objSub[$objSubKeys[0]]) . ":";
$objIgnore = json_decode("{\"k" . hex2bin("80") . "\":\"v" . hex2bin("80") . "\"}", true, 512, JSON_INVALID_UTF8_IGNORE);
$objIgnoreKeys = array_keys($objIgnore);
echo bin2hex($objIgnoreKeys[0]) . "=" . bin2hex($objIgnore[$objIgnoreKeys[0]]) . ":";
echo (is_null(json_decode("bad")) ? "BAD" : "wrong") . ":";
$big = json_decode("[9223372036854775808]", true, 512, JSON_BIGINT_AS_STRING);
echo json_decode("9223372036854775808", true, 512, JSON_BIGINT_AS_STRING) . ":";
echo json_decode("-9223372036854775809", true, 512, JSON_BIGINT_AS_STRING) . ":";
echo gettype($big[0]) . ":" . $big[0] . ":";
echo call_user_func_array("json_decode", ["json" => "9223372036854775808", "associative" => true, "depth" => 512, "flags" => JSON_BIGINT_AS_STRING]) . ":";
return function_exists("json_decode") && defined("JSON_BIGINT_AS_STRING") && defined("JSON_INVALID_UTF8_IGNORE") && defined("JSON_INVALID_UTF8_SUBSTITUTE");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
            values.output,
            "hello:42:T:NULL:1:x:F:4:v:utf8-null:5:6162:0:61efbfbd62:0:6befbfbd=76efbfbd:6b=76:BAD:9223372036854775808:-9223372036854775809:string:9223372036854775808:9223372036854775808:"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `json_decode()` returns `stdClass` objects unless assoc is true.
#[test]
fn execute_program_dispatches_json_decode_stdclass_default() {
    let program = parse_fragment(
        br#"$object = json_decode("{\"a\":1,\"b\":{\"c\":\"x\"}}");
echo $object->a . ":" . $object->b->c . ":";
$objectFalse = json_decode("{\"z\":2}", false);
echo $objectFalse->z . ":";
$objectNull = json_decode("{\"n\":{\"m\":3}}", null);
echo $objectNull->n->m . ":";
$assoc = json_decode("{\"b\":{\"c\":\"y\"}}", true);
echo $assoc["b"]["c"] . ":";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "1:x:2:3:y:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `json_encode()` serializes stdClass dynamic properties.
#[test]
fn execute_program_dispatches_json_encode_stdclass_object() {
    let program = parse_fragment(
        br#"$object = json_decode("{\"a\":1,\"b\":{\"c\":\"x\"}}");
echo json_encode($object) . ":";
echo str_replace("\n", "|", json_encode($object, JSON_PRETTY_PRINT)) . ":";
$empty = json_decode("{}");
echo json_encode($empty) . ":";
$empty->a = 7;
echo json_encode($empty);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        r#"{"a":1,"b":{"c":"x"}}:{|    "a": 1,|    "b": {|        "c": "x"|    }|}:{}:{"a":7}"#
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `json_last_error*()` track JSON parse failures and success resets.
#[test]
fn execute_program_dispatches_json_last_error_builtins() {
    let program = parse_fragment(
            br#"echo json_last_error() . ":" . json_last_error_msg() . ":";
json_decode("bad");
echo json_last_error() . ":" . (json_last_error() === JSON_ERROR_SYNTAX ? "syntax" : "bad") . ":" . json_last_error_msg() . ":";
json_validate("[1]", 1);
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_validate("\"ok\"");
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_validate("\"a" . chr(10) . "b\"");
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_decode("\"\\uD83D\"");
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_decode("\"a" . chr(128) . "b\"");
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_validate("[0]");
echo call_user_func("json_last_error") . ":" . call_user_func_array("json_last_error_msg", []) . ":";
return function_exists("json_last_error") && function_exists("json_last_error_msg") && defined("JSON_ERROR_SYNTAX");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
            values.output,
            "0:No error:4:syntax:Syntax error near location 1:1:1:Maximum stack depth exceeded near location 1:1:0:No error:3:Control character error, possibly incorrectly encoded near location 1:3:10:Single unpaired UTF-16 surrogate in unicode escape near location 1:8:5:Malformed UTF-8 characters, possibly incorrectly encoded near location 1:3:0:No error:"
        );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval JSON throw flags raise catchable Throwable objects.
#[test]
fn execute_program_dispatches_json_throw_on_error() {
    let program = parse_fragment(
        br#"try {
    json_decode("bad", true, 512, JSON_THROW_ON_ERROR);
    echo "bad";
} catch (Throwable) {
    echo "decode:";
}
try {
    json_encode(INF, JSON_THROW_ON_ERROR);
    echo "bad";
} catch (Throwable) {
    echo "encode:";
}
echo json_encode(INF, JSON_THROW_ON_ERROR | JSON_PARTIAL_OUTPUT_ON_ERROR) . ":";
return defined("JSON_THROW_ON_ERROR");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "decode:encode:0:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies eval `json_validate()` validates documents, depth, and dynamic calls.
#[test]
fn execute_program_dispatches_json_validate_builtin() {
    let program = parse_fragment(
            br#"echo (json_validate("{\"a\":[1,true,null,\"caf\\u00e9\"]}") ? "Y" : "N") . ":";
echo (json_validate("bad") ? "bad" : "N") . ":";
echo (json_validate("[1]", 1) ? "bad" : "D") . ":";
echo (call_user_func("json_validate", "\"x\"") ? "C" : "bad") . ":";
echo (call_user_func_array("json_validate", ["json" => "[[1]]", "depth" => 3, "flags" => 0]) ? "A" : "bad") . ":";
echo (json_validate("\"a" . chr(128) . "b\"", 512, JSON_INVALID_UTF8_IGNORE) ? "I" : "bad") . ":";
echo json_last_error() . ":";
echo (json_validate("bad", 512, JSON_INVALID_UTF8_IGNORE) ? "bad" : "S") . ":";
echo json_last_error() . ":";
return function_exists("json_validate") && defined("JSON_INVALID_UTF8_IGNORE");"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "Y:N:D:C:A:I:0:S:4:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
/// Verifies direct eval builtin calls bind named and unpacked arguments.
#[test]
fn execute_program_dispatches_named_and_spread_builtins() {
    let program = parse_fragment(
        br#"echo strlen(string: "abcd");
echo ":" . (array_key_exists(array: ["name" => 1], key: "name") ? "Y" : "N");
echo ":" . (str_contains(...["haystack" => "abc", "needle" => "b"]) ? "Y" : "N");
return round(precision: 1, num: 3.14);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "4:Y:Y");
    assert_eq!(values.get(result), FakeValue::Float(3.1));
}
