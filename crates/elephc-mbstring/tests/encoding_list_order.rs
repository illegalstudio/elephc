//! Purpose:
//! Checks incremental encoding-list resolution against PHP callback-order fixtures.
//!
//! Called from:
//! - Cargo's focused mbstring integration test harness.
//!
//! Key details:
//! - Simulated host callbacks mutate the real engine state between individual list entries.
//! - PHP supplies expected results, binary errors, callback traces, and final request settings.
//! - Outer strict conversion and native callback/reference ownership remain host-adapter requirements.

use elephc_mbstring::{encoding::{Encoding, EncodingListBuilder}, error::MbError, state::State};
use serde_json::{json, Value};

/// Encodes binary strings using the independent PHP fixture's representation.
fn hex(bytes: &[u8]) -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() }

/// Preserves PHP exception class and raw bytes from a shared-engine list failure.
fn error(error: MbError) -> Value {
    match error {
        MbError::Value(message) => json!(["error", "ValueError", hex(message.as_bytes())]),
        MbError::ValueBytes(message) => json!(["error", "ValueError", hex(&message)]),
        MbError::Runtime(message) => json!(["error", "Error", hex(message.as_bytes())]),
    }
}

/// Represents an already evaluated array element whose string conversion has not yet occurred.
enum Element {
    Name(&'static [u8]),
    Callback { label: &'static str, name: &'static [u8], language: Option<&'static [u8]>, throws: bool },
}

/// Records one host Stringable conversion and performs its declared request-language side effect.
fn callback(state: &mut State, trace: &mut Vec<Value>, label: &str, language: Option<&[u8]>, throws: bool) -> Result<(), Value> {
    trace.push(json!(["stringify", label]));
    if let Some(language) = language { state.set_language(language).unwrap(); }
    if throws { return Err(json!(["error", "RuntimeException", hex(b"list callback stopped")])); }
    Ok(())
}

/// Converts and immediately resolves each entry, observing language changes and stopping on failure.
fn run(scenario: &str, state: &mut State, trace: &mut Vec<Value>) -> Value {
    use Element::{Name, Callback};
    let conversion = scenario == "list_conversion_outer_then_elements";
    if conversion { callback(state, trace, "target", Some(b"Japanese"), false).unwrap(); }
    let entries = match scenario {
        "list_stops_at_invalid" => vec![
            Callback { label: "first", name: b"not-an-encoding", language: None, throws: false },
            Callback { label: "later", name: b"UTF-8", language: None, throws: false },
        ],
        "list_auto_before_language_change" => vec![Name(b"auto"),
            Callback { label: "later", name: b"UTF-8", language: Some(b"Japanese"), throws: false }],
        "list_auto_after_language_change" => vec![
            Callback { label: "first", name: b"UTF-8", language: Some(b"Japanese"), throws: false }, Name(b"auto")],
        "list_conversion_outer_then_elements" => vec![Name(b"auto"),
            Callback { label: "source", name: b"UTF-8", language: Some(b"neutral"), throws: false }],
        "list_throws" => vec![Name(b"UTF-8"),
            Callback { label: "failure", name: b"ASCII", language: None, throws: true },
            Callback { label: "later", name: b"ASCII", language: None, throws: false }],
        "list_null_before_invalid" => vec![Name(b""),
            Callback { label: "later", name: b"UTF-8", language: None, throws: false }],
        "list_numeric_before_stringable" => vec![Name(b"1.5"),
            Callback { label: "later", name: b"UTF-8", language: None, throws: false }],
        _ => panic!("unexpected incremental-list scenario"),
    };
    let mut builder = if conversion { EncodingListBuilder::new("mb_convert_encoding", 3, "from_encoding") }
        else { EncodingListBuilder::new("mb_detect_encoding", 2, "encodings") };
    for entry in entries {
        let name = match entry {
            Name(name) => name,
            Callback { label, name, language, throws } => {
                if let Err(error) = callback(state, trace, label, language, throws) { return error; }
                name
            }
        };
        if let Err(failure) = state.push_array_encoding(&mut builder, name) { return error(failure); }
    }
    let candidates = builder.finish().unwrap();
    if conversion {
        let sources = elephc_mbstring::text::ConversionSources::new(candidates).unwrap();
        let output = state.convert_string(b"\x82\xa0", Encoding::lookup(b"UTF-8").unwrap(), &sources).unwrap();
        json!(["string", hex(&output)])
    } else {
        match elephc_mbstring::detect::guess(b"\x82\xa0", &candidates, true, true) {
            Some(encoding) => json!(["string", hex(encoding.name().as_bytes())]),
            None => json!(["bool", false]),
        }
    }
}

/// Compares lazy array-element parsing, callback exceptions, and auto expansion with PHP.
#[test]
fn mbstring_incremental_encoding_lists_match_php_order() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("fixtures/coercion_order.json")).unwrap();
    let mut count = 0;
    for case in cases {
        let scenario = case["scenario"].as_str().unwrap();
        if !scenario.starts_with("list_") { continue; }
        // The pending outer native/eval adapter must reject this Stringable target in strict mode.
        if scenario == "list_conversion_outer_then_elements" && case["strict"] == true { continue; }
        let mut state = State::default();
        let mut trace = Vec::new();
        let result = run(scenario, &mut state, &mut trace);
        assert_eq!(result, case["result"], "{case}");
        assert_eq!(json!(trace), case["trace"], "{case}");
        assert_eq!(state.internal_encoding().name(), case["internal"].as_str().unwrap(), "{case}");
        assert_eq!(state.language().name(), case["language"].as_str().unwrap(), "{case}");
        assert_eq!(state.substitute().character as i64, case["substitution"].as_i64().unwrap(), "{case}");
        assert_eq!(case["precision"], "14", "these list cases do not invoke diagnostic handlers");
        count += 1;
    }
    assert_eq!(count, 39);
}

/// Prevents a failed partial list from becoming a valid list through later appends or finalization.
#[test]
fn mbstring_incremental_encoding_list_failure_is_terminal() {
    let mut state = State::default();
    let mut builder = EncodingListBuilder::new("mb_detect_encoding", 2, "encodings");
    state.push_array_encoding(&mut builder, b"UTF-8").unwrap();
    let first = state.push_array_encoding(&mut builder, b"\xff\0ignored").unwrap_err();
    state.set_language(b"Japanese").unwrap();
    assert_eq!(state.push_array_encoding(&mut builder, b"auto"), Err(first.clone()));
    assert_eq!(builder.finish(), Err(first));
}
