//! Purpose:
//! Verifies shared replacement callback matching against independent PHP traces.
//!
//! Called from:
//! - The focused `regex_request` integration test binary with its native provider enabled.
//!
//! Key details:
//! - Ordinary fixtures cover exact capture arrays, encodings, options, errors, and replacement bytes.
//! - Host failures and live request limits are tested separately from PHP value coercion.

use std::{cell::Cell, convert::Infallible};
use elephc_mbstring::regex::{CallbackReplacement, ReplacementError};
use super::{bytes, exception, hex, install_provider, json, regex_provider, registers, string,
    Event, Limits, ReplaceResult, Session, Value};

/// Returns the normal PHP search limits without anchoring-specific conversion rules.
fn limits() -> Limits { Limits::from_ini(100_000, 1_000_000, false) }

/// Compares complete PHP callback traces for binary captures, empty matches, errors, and option precedence.
#[test]
#[ignore = "requires managed Oniguruma archives or pinned 6.9.10 native development files"]
fn regex_callback_matches_php() {
    assert!(unsafe { install_provider(regex_provider::provider()) });
    let cases = include_str!("../fixtures/regex_replace_callback.jsonl");
    let mut count = 0;
    for line in cases.lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let input = &case["input"];
        let session = Session::default();
        session.set_encoding(input["encoding"].as_str().unwrap().as_bytes()).unwrap();
        session.set_options(input["defaults"].as_str().unwrap_or("pr").as_bytes()).unwrap();
        let pattern = bytes(&input["pattern"]).unwrap();
        let subject = bytes(&input["subject"]).unwrap();
        let replacement = bytes(&input["replacement"]).unwrap();
        let options = bytes(&input["options"]);
        let mut warnings = Vec::new();
        let mut error = Value::Null;
        let mut matches = Vec::new();
        let retry = Cell::new(1_000_000);
        let result = session.replace_callback(CallbackReplacement { pattern: &pattern, subject: &subject,
            options: options.as_deref(), encoding: session.encoding() },
            || Limits::from_ini(100_000, retry.get(), false), &mut |event| match event {
                Event::Warning(bytes) => warnings.push(hex(&bytes)),
                Event::Exception(raised) => error = exception(raised, error.take()),
            }, |groups| {
                matches.push(registers(Some(groups)));
                if let Some(options) = input["callback_options"].as_str() { session.set_options(options.as_bytes()).unwrap(); }
                if let Some(value) = input["callback_retry"].as_i64() { retry.set(value); }
                if input["throw_after"].as_u64() == Some(matches.len() as u64) {
                    return Err(json!(["RuntimeException", hex(b"callback failed"), null]));
                }
                Ok(replacement.clone())
            });
        let result = match result {
            Ok(result) => Some(result),
            Err(ReplacementError::Callback(raised)) => { error = raised; None },
            Err(ReplacementError::Regex(error)) => panic!("unexpected integration error: {error:?}"),
        };
        let mut actual = json!({"matches": matches, "warnings": warnings});
        if error.is_null() {
            actual["value"] = match result.unwrap() {
                ReplaceResult::InvalidSubject => Value::Null,
                ReplaceResult::Failed => json!(false),
                ReplaceResult::String(value) => string(&value),
            };
        } else { actual["error"] = error; }
        if input.get("callback_options").is_some() { actual["options"] = json!(session.options().as_string()); }
        assert_eq!(actual, case["expected"], "case {count}: {input}");
        count += 1;
    }
    assert_eq!(count, 32);
}

/// Stops on the first host failure, discards earlier replacement bytes, and preserves the host error.
#[test]
#[ignore = "requires managed Oniguruma archives or pinned 6.9.10 native development files"]
fn regex_callback_error_stops_replacement() {
    assert!(unsafe { install_provider(regex_provider::provider()) });
    let session = Session::default();
    let mut calls = 0;
    let result = session.replace_callback(CallbackReplacement { pattern: b"a", subject: b"a-a-a",
        options: None, encoding: session.encoding() }, limits, &mut |event| panic!("{event:?}"), |_| {
            calls += 1;
            if calls == 2 { Err("callback failed") } else { Ok(b"first".to_vec()) }
        });
    assert_eq!(result, Err(ReplacementError::Callback("callback failed")));
    assert_eq!(calls, 2);
}

/// Reads the updated request limits for every search after a callback changes their values.
#[test]
#[ignore = "requires managed Oniguruma archives or pinned 6.9.10 native development files"]
fn regex_callback_reads_live_limits() {
    assert!(unsafe { install_provider(regex_provider::provider()) });
    let session = Session::default();
    let retry = Cell::new(1_000_000);
    let mut observed = Vec::new();
    let result = session.replace_callback(CallbackReplacement { pattern: b"a", subject: b"aba",
        options: None, encoding: session.encoding() }, || {
            observed.push(retry.get());
            Limits::from_ini(100_000, retry.get(), false)
        }, &mut |event| panic!("{event:?}"), |_| {
            retry.set(retry.get() - 1);
            Ok::<_, Infallible>(b"X".to_vec())
        }).unwrap();
    assert_eq!(result, ReplaceResult::String(b"XbX".to_vec()));
    assert_eq!(observed, [1_000_000, 999_999, 999_998]);
}

/// Preserves the active compiled options while callbacks change the defaults for later operations.
#[test]
#[ignore = "requires managed Oniguruma archives or pinned 6.9.10 native development files"]
fn regex_callback_retains_compiled_options() {
    assert!(unsafe { install_provider(regex_provider::provider()) });
    let session = Session::default();
    let result = session.replace_callback(CallbackReplacement { pattern: b"a", subject: b"aAaA",
        options: None, encoding: session.encoding() }, limits, &mut |event| panic!("{event:?}"), |_| {
            session.set_options(b"i").unwrap();
            Ok::<_, Infallible>(b"X".to_vec())
        }).unwrap();
    assert_eq!(result, ReplaceResult::String(b"XAXA".to_vec()));
    assert_eq!(session.options().as_string(), "ir");
}

/// Validates the subject against the entry encoding even if argument conversions changed live settings.
#[test]
#[ignore = "requires managed Oniguruma archives or pinned 6.9.10 native development files"]
fn regex_callback_retains_entry_encoding() {
    assert!(unsafe { install_provider(regex_provider::provider()) });
    let session = Session::default();
    let encoding = session.encoding();
    session.set_encoding(b"ISO-8859-1").unwrap();
    let result = session.replace_callback(CallbackReplacement { pattern: b".", subject: b"\xff",
        options: None, encoding }, limits, &mut |event| panic!("{event:?}"), |_| -> Result<Vec<u8>, Infallible> {
            panic!("invalid subjects must not call the replacement callback")
        }).unwrap();
    assert_eq!(result, ReplaceResult::InvalidSubject);
}
