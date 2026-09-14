//! Purpose:
//! Checks shared settings, language defaults, encoding-list parsing, and lookup diagnostics.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test state`.
//!
//! Key details:
//! - The PHP oracle records ordered mutations and exact state after errors.
//! - Binary names, NUL handling, auto expansion, and deprecation caching stay observable.

use std::io::{BufRead, BufReader};
use elephc_mbstring::{encoding::{EncodingList, SubstituteMode}, error::{MbError, MbResult}, state::{Language, State}};
use flate2::read::GzDecoder;
use serde_json::{json, Value};

/// Reads one losslessly captured PHP byte string.
fn bytes(value: &Value) -> Vec<u8> {
    let hex = value["bytes"].as_str().unwrap();
    (0..hex.len()).step_by(2).map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()).collect()
}

/// Writes bytes in the oracle's lossless representation.
fn string(value: impl AsRef<[u8]>) -> Value {
    json!({"bytes": value.as_ref().iter().map(|byte| format!("{byte:02x}")).collect::<String>()})
}

/// Represents the active substitution mode exactly as PHP's getter does.
fn substitute(state: &State) -> Value {
    let sub = state.substitute();
    match sub.mode {
        SubstituteMode::Character => json!(sub.character), SubstituteMode::None => string("none"),
        SubstituteMode::Long => string("long"), SubstituteMode::Entity => string("entity"),
    }
}

/// Reads settings in the same fixed field order as the independent PHP snapshot.
fn snapshot(state: &State) -> Value {
    json!([string(state.language().name()), string(state.internal_encoding().name()), string(state.http_output().name()),
        state.detect_order().iter().map(|encoding| string(encoding.name())).collect::<Vec<_>>(), substitute(state)])
}

/// Executes one setting mutation or ordinary lookup and collects diagnostics separately.
fn execute(state: &mut State, function: &str, arg: &Value, warnings: &mut Vec<Value>) -> MbResult<Value> {
    if arg.is_null() && function != "mb_strlen" {
        let fields = snapshot(state);
        let index = match function { "mb_language" => 0, "mb_internal_encoding" => 1, "mb_http_output" => 2,
            "mb_detect_order" => 3, "mb_substitute_character" => 4, _ => panic!("unknown getter {function}") };
        return Ok(fields[index].clone());
    }
    match function {
        "mb_language" => state.set_language(&bytes(arg))?,
        "mb_internal_encoding" => state.set_internal_encoding(&bytes(arg))?,
        "mb_http_output" => state.set_http_output(&bytes(arg))?,
        "mb_detect_order" => {
            if let Some(values) = arg.as_array() {
                let values = values.iter().map(bytes).collect::<Vec<_>>();
                state.set_detect_order(EncodingList::Array(&values))?;
            } else { state.set_detect_order(EncodingList::CommaSeparated(&bytes(arg)))?; }
        }
        "mb_substitute_character" => {
            if let Some(code) = arg.as_i64() { state.set_substitute_codepoint(code)?; }
            else { state.set_substitute_mode(&bytes(arg))?; }
        }
        "mb_strlen" => {
            let name = (!arg.is_null()).then(|| bytes(arg));
            let resolved = state.resolve_encoding(name.as_deref(), function, 2, "encoding")?;
            if let Some(message) = resolved.deprecation { warnings.push(json!([8192, string(format!("{function}(): {message}"))])); }
            return Ok(json!(resolved.encoding.strlen(b"")));
        }
        _ => panic!("unknown setting {function}"),
    }
    Ok(json!(true))
}

/// Compares complete ordered requests, failures, diagnostic caching, and resulting settings.
#[test]
fn setting_transitions_match_php() {
    let fixture = include_bytes!("fixtures/state.jsonl.gz");
    let reader = BufReader::new(GzDecoder::new(fixture.as_slice()));
    let mut state = State::default();
    let mut count = 0;
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let mut warnings = Vec::new();
        let result = execute(&mut state, case["function"].as_str().unwrap(), &case["argument"], &mut warnings)
            .unwrap_or_else(|error| {
                let (class, message) = match error { MbError::Value(message) => ("ValueError", message.into_bytes()),
                    MbError::ValueBytes(message) => ("ValueError", message), MbError::Runtime(message) => ("Error", message.into_bytes()) };
                json!({"error": [string(class), string(message)]})
            });
        assert_eq!(result, case["result"], "result {case}");
        assert_eq!(json!(warnings), case["warnings"], "warnings {case}");
        assert_eq!(snapshot(&state), case["state"], "state {case}");
        count += 1;
    }
    assert!(count > 1000, "incomplete setting fixture: {count}");
}

/// Covers every captured language alias, default list, and mail encoding tuple.
#[test]
fn language_defaults_match_php() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/languages.json")).unwrap();
    assert_eq!(Language::default().name(), "neutral");
    assert_eq!(fixture["languages"].as_object().unwrap().len(), 12);
    for (name, data) in fixture["languages"].as_object().unwrap() {
        let language = Language::lookup(name.as_bytes()).unwrap();
        assert_eq!(language.name(), name);
        assert_eq!(json!(language.detect_order().iter().map(|encoding| encoding.name()).collect::<Vec<_>>()), data["detect_order"]);
        let mail = language.mail_encodings().map(|encoding| encoding.name());
        for (index, field) in ["mail_charset", "mail_header_encoding", "mail_body_encoding"].iter().enumerate() {
            assert_eq!(mail[index], data[field].as_str().unwrap(), "{name} {field}");
        }
        for alias in data["aliases"].as_array().unwrap() {
            assert_eq!(Language::lookup(alias.as_str().unwrap().to_ascii_uppercase().as_bytes()), Some(language));
        }
    }
}
