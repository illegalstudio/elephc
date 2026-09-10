//! Purpose:
//! Compares complete string-operation results and exceptions to a cross-encoding PHP oracle.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test operations`.
//!
//! Key details:
//! - The compressed JSONL fixture retains every argument and complete expected result.
//! - Original byte strings, malformed inputs, bounds, defaults, and Unicode cases are covered.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{encoding::{Encoding, Substitute, SubstituteMode}, error::{MbError, MbResult}, text};
use flate2::read::GzDecoder;
use serde_json::{json, Value};

/// Decodes raw string bytes from the lossless fixture representation.
fn bytes(value: &Value) -> Vec<u8> {
    let hex = value["bytes"].as_str().expect("encoded string argument");
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Encodes returned bytes in the same lossless form used by the PHP oracle.
fn byte_value(bytes: Vec<u8>) -> Value {
    json!({"bytes": bytes.into_iter().map(|byte| format!("{byte:02x}")).collect::<String>()})
}

/// Converts a shared engine failure into its exact PHP exception class and message.
fn error_value(error: MbError) -> Value {
    match error {
        MbError::Value(message) => json!({"error": ["ValueError", message]}),
        MbError::ValueBytes(message) => json!({"error": ["ValueError", String::from_utf8(message).expect("UTF-8 operation diagnostic")]}),
        MbError::Runtime(message) => json!({"error": ["Error", message]}),
    }
}

/// Applies one captured operation to the Rust engine without running PHP during tests.
fn execute(case: &Value) -> MbResult<Value> {
    let function = case["function"].as_str().unwrap();
    let encoding = Encoding::lookup(case["encoding"].as_str().unwrap().as_bytes()).unwrap();
    let args = case["arguments"].as_array().unwrap();
    let sub = Substitute { mode: SubstituteMode::Character, character: 0xfffd };
    if function == "mb_chr" {
        return Ok(text::chr(args[0].as_i64().unwrap(), encoding)?.map(byte_value).unwrap_or(json!(false)));
    }
    let input = bytes(&args[0]);
    let int = |index: usize| args[index].as_i64().unwrap();
    let result = match function {
        "mb_ord" => text::ord(&input, encoding)?.map(|value| json!(value)).unwrap_or(json!(false)),
        "mb_substr" => byte_value(text::substr(&input, int(1), args[2].as_i64(), encoding, sub)?),
        "mb_strcut" => byte_value(text::strcut(&input, int(1), args[2].as_i64(), encoding, sub)?),
        "mb_str_split" => Value::Array(text::str_split(&input, int(1), encoding, sub)?.into_iter().map(byte_value).collect()),
        "mb_ucfirst" | "mb_lcfirst" => byte_value(text::first_case(&input, function == "mb_ucfirst", encoding, sub)?),
        "mb_trim" | "mb_ltrim" | "mb_rtrim" => {
            let characters = (!args[1].is_null()).then(|| bytes(&args[1]));
            let side = match function { "mb_ltrim" => text::TrimSide::Left, "mb_rtrim" => text::TrimSide::Right, _ => text::TrimSide::Both };
            byte_value(text::trim(&input, characters.as_deref(), side, encoding, sub)?)
        }
        "mb_str_pad" => byte_value(text::str_pad(&input, int(1), &bytes(&args[2]), int(3), encoding, sub)?),
        "mb_strimwidth" => byte_value(text::strimwidth(&input, int(1), int(2), &bytes(&args[3]), encoding, sub)?),
        "mb_substr_count" => json!(text::substr_count(&input, &bytes(&args[1]), encoding)?),
        "mb_strpos" | "mb_stripos" | "mb_strrpos" | "mb_strripos" => {
            let mode = text::SearchMode { reverse: function.starts_with("mb_strr"), insensitive: function.contains('i') };
            text::strpos(&input, &bytes(&args[1]), int(2), encoding, mode)?.map(|value| json!(value)).unwrap_or(json!(false))
        }
        "mb_strstr" | "mb_stristr" | "mb_strrchr" | "mb_strrichr" => {
            let mode = text::SearchMode { reverse: function.starts_with("mb_strr"), insensitive: function.contains('i') };
            text::strstr(&input, &bytes(&args[1]), args[2].as_bool().unwrap(), encoding, mode, sub)?.map(byte_value).unwrap_or(json!(false))
        }
        _ => panic!("unknown fixture function {function}"),
    };
    Ok(result)
}

/// Checks every captured result and reports the first concrete mismatching request.
#[test]
fn text_operations_match_php() {
    let fixture = include_bytes!("fixtures/operations.jsonl.gz");
    let reader = BufReader::new(GzDecoder::new(fixture.as_slice()));
    let mut count = 0;
    for (index, line) in reader.lines().enumerate() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let result = execute(&case).unwrap_or_else(error_value);
        assert_eq!(result, case["result"], "PHP fixture line {}, request {}", index + 1, case);
        count += 1;
    }
    assert!(count > 40000, "the cross-encoding operation matrix must remain populated");
}
