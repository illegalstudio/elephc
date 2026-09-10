//! Purpose:
//! Checks observable state and return values when INI warning handlers reenter mbstring.
//!
//! Called from:
//! - The shared INI integration test binary and independently captured PHP worker traces.
//!
//! Key details:
//! - The fixture models protected PHP exceptions as pending status, never Rust unwinding.
//! - MIME compile failure is supplied at the host boundary; PCRE2 syntax has separate native tests.

use super::*;
use std::cell::RefCell;
use elephc_mbstring::{coercion::Diagnostic, state::IniString};

/// Supplies the native provider's known invalid-pattern result for the sole syntax error in this fixture.
fn validate(pattern: &[u8]) -> Result<(), MimeRegexError> {
    if pattern == b"[" { Err(MimeRegexError { offset: 1, message: b"missing terminating ] for character class".to_vec() }) }
    else { Ok(()) }
}

/// Omits regex limits because PHP exposes their raw text but has no getter for parsed live limits.
fn observed(state: &RefCell<State>) -> Value {
    let Value::Array(mut values) = snapshot(&state.borrow()) else { unreachable!(); };
    values.truncate(4);
    Value::Array(values)
}

/// Preserves getter and interned identities while keeping equal newly constructed strings distinct.
fn ini_text(state: &RefCell<State>, value: &Value) -> IniString {
    if let Some(text) = value.as_str() { return IniString::new(text.as_bytes()); }
    let argument = &value[1];
    match value[0].as_str().unwrap() {
        "get" => state.borrow().ini_get_string(argument.as_str().unwrap().as_bytes()).unwrap(),
        "flat" | "local" | "global" => state.borrow().ini_get_all_string(argument.as_str().unwrap().as_bytes(), value[0] == "global")
            .unwrap_or_else(|| IniString::interned(b"")),
        "copy" => IniString::fresh(&ini_text(state, argument)),
        "lower" | "upper" | "reverse" => {
            let source = ini_text(state, argument);
            let bytes = match value[0].as_str().unwrap() {
                "lower" => source.to_ascii_lowercase(),
                "upper" => source.to_ascii_uppercase(),
                _ => source.iter().copied().rev().collect(),
            };
            if value[0] != "reverse" && bytes == source.as_ref() { source }
            else { IniString::fresh(&bytes) }
        },
        "literal" => IniString::interned(argument.as_str().unwrap().as_bytes()),
        other => panic!("unsupported fixture string {other}"),
    }
}

/// Runs one shared operation with optional warning delivery, always releasing borrows before callbacks.
fn action(state: &RefCell<State>, operation: &Value, emit: impl FnMut(Diagnostic)) -> Value {
    let name = operation[1].as_str().unwrap().as_bytes();
    match operation[0].as_str().unwrap() {
        "set" => State::ini_set_string_reentrant(state, name, ini_text(state, &operation[2]), validate, emit)
            .previous.as_deref().map_or(json!(false), string),
        "restore" => { State::ini_restore_reentrant(state, name, validate, emit); Value::Null },
        "internal" => { state.borrow_mut().set_internal_encoding(name).unwrap(); json!(true) },
        "output" => { state.borrow_mut().set_http_output(name).unwrap(); json!(true) },
        "language" => { state.borrow_mut().set_language(name).unwrap(); json!(true) },
        "detect" => { state.borrow_mut().set_detect_order(EncodingList::CommaSeparated(name)).unwrap(); json!(true) },
        other => panic!("unsupported fixture action {other}"),
    }
}

/// Compares every warning-time snapshot, inner result, final state, old value, and pending-exception outcome.
#[test]
fn ini_reentry_matches_php() {
    let reader = BufReader::new(GzDecoder::new(include_bytes!("../fixtures/ini_reentry.jsonl.gz").as_slice()));
    let mut count = 0;
    for line in reader.lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let settings: Vec<_> = case["startup"].as_array().unwrap().iter().map(|entry|
            (entry[0].as_str().unwrap().as_bytes().to_vec(), entry[1].as_str().unwrap().as_bytes().to_vec())).collect();
        let state = RefCell::new(State::with_ini_configuration(&settings, CoreEncodingDefaults::default(), validate).0);
        for operation in case["before"].as_array().unwrap() { action(&state, operation, |_| {}); }
        let mut pending = false;
        let mut events = Vec::new();
        let output = action(&state, &case["outer"], |warning| {
            if pending { return; }
            let before = observed(&state);
            let inner: Vec<_> = case["callback"].as_array().unwrap().iter().map(|operation| action(&state, operation, |_| {})).collect();
            events.push(json!({"level": warning.level, "message": string(&warning.message), "before": before, "inner": inner, "after": observed(&state)}));
            pending |= case["throw"].as_bool().unwrap();
        });
        let label = format!("reentry {count}, outer {}, callback {}, throwing {}", case["outer"], case["callback"], case["throw"]);
        compare(&json!(events), &case["events"], "events", &label);
        compare(&observed(&state), &case["state"], "state", &label);
        if pending { assert_eq!(case["error"], json!(["Exception", "capture throw"]), "{label}"); }
        else { compare(&output, &case["output"], "output", &label); }
        count += 1;
    }
    assert_eq!(count, 1376);
}
