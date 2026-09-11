//! Purpose:
//! Applies PHP mbstring INI handlers with diagnostics at their observable mutation points.
//!
//! Called from:
//! - Runtime INI changes, restores, core encoding changes, and startup configuration.
//!
//! Key details:
//! - State borrows end before warnings run, allowing PHP error handlers to reenter mbstring.
//! - Failed writes and throwing PHP handlers can still leave observable state changes.

use super::*;
use super::mutation::Access;
use crate::{encoding::{parse_encoding_list, EncodingList, Substitute, SubstituteMode}, error::MbError, state::Language};

/// Runs one directive handler without publishing raw INI storage or retaining state across warnings.
pub(super) fn apply(state: &mut impl Access, key: Key, value: Option<&[u8]>, caller: &str,
    validate: &mut impl FnMut(&[u8]) -> Result<(), MimeRegexError>, emit: &mut impl FnMut(Diagnostic)) -> bool {
    match key {
        Key::Language => state.with(|state| match Language::lookup(value.unwrap_or_default()) {
            Some(language) => { state.language = language; state.auto_language = language; true },
            None => { state.language = Language::default(); false },
        }),
        Key::Detect => {
            let Some(value) = value else { state.with(|state| state.ini.configured_detect = None); return true; };
            match encoding_list(state, value, caller, emit) {
                Some(order) if !order.is_empty() => { state.with(|state| state.ini.configured_detect = Some(order)); true },
                _ => false,
            }
        },
        Key::Input | Key::Output | Key::Internal => {
            if value.is_some() {
                emit(Diagnostic { level: 8192, message: format!("{}: Use of {} is deprecated", context(caller), DIRECTIVES[key as usize].name).into_bytes() });
            }
            let explicit = value.is_some_and(|value| !value.is_empty());
            state.with(|state| match key {
                Key::Input => state.ini.explicit_input = explicit,
                Key::Output => state.ini.explicit_output = explicit,
                Key::Internal => state.ini.explicit_internal = explicit,
                _ => unreachable!("encoding directives only"),
            });
            let value = if explicit { value.unwrap().to_vec() } else { inherited_value(state, key) };
            match key {
                Key::Input => input(state, &value, caller, emit) || !explicit,
                Key::Output => output(state, &value) || !explicit,
                Key::Internal => { internal(state, &value, caller, emit); true },
                _ => unreachable!("encoding directives only"),
            }
        },
        Key::Substitute => { state.with(|state| substitute(state, value)); true },
        Key::Translation => {
            let Some(value) = value else { return false; };
            state.with(|state| state.encoding_translation = numeric::boolean(value)); true
        },
        Key::Strict => { state.with(|state| state.strict_detection = numeric::boolean(value.unwrap_or_default())); true },
        Key::Stack | Key::Retry => {
            let (number, warning) = numeric::quantity(value.unwrap_or_default());
            if let Some(warning) = warning {
                let mut message = format!("Invalid \"{}\" setting. ", DIRECTIVES[key as usize].name).into_bytes();
                message.extend_from_slice(c_string(&warning));
                emit(Diagnostic { level: 2, message });
            }
            state.with(|state| if key == Key::Stack { state.ini.regex_stack = number; } else { state.ini.regex_retry = number; });
            true
        },
        Key::Mimetypes => {
            let value = value.unwrap_or_else(|| DIRECTIVES[Key::Mimetypes as usize].default.unwrap().as_bytes());
            if let Some(pattern) = crate::state::mime_pattern(value) {
                if let Err(error) = validate(pattern) {
                    let mut message = format!("{}: ", context(caller)).into_bytes();
                    message.extend_from_slice(pattern);
                    message.extend_from_slice(format!(" (offset={}): ", error.offset).as_bytes());
                    message.extend_from_slice(c_string(&error.message));
                    emit(Diagnostic { level: 2, message });
                    return false;
                }
            }
            state.with(|state| state.output_mimetypes = value.to_vec());
            true
        },
    }
}

/// Applies the core encoding hook, checking each explicit flag after preceding callbacks finish.
pub(super) fn inherited(state: &mut impl Access, caller: &str, emit: &mut impl FnMut(Diagnostic)) {
    if state.with(|state| !state.ini.explicit_internal) {
        let value = inherited_value(state, Key::Internal);
        internal(state, &value, caller, emit);
    }
    if state.with(|state| !state.ini.explicit_output) {
        let value = inherited_value(state, Key::Output);
        output(state, &value);
    }
    if state.with(|state| !state.ini.explicit_input) {
        let value = inherited_value(state, Key::Input);
        input(state, &value, caller, emit);
    }
}

/// Copies the host's current effective C-string encoding before its handler can emit warnings.
fn inherited_value(state: &mut impl Access, key: Key) -> Vec<u8> {
    state.with(|state| c_string(match key {
        Key::Internal => &state.ini.defaults.internal, Key::Input => &state.ini.defaults.input, Key::Output => &state.ini.defaults.output,
        _ => unreachable!("only inherited encoding directives request core values"),
    }).to_vec())
}

/// Parses a complete list before emitting any failure diagnostic, preserving the current auto language.
fn encoding_list(state: &mut impl Access, value: &[u8], caller: &str, emit: &mut impl FnMut(Diagnostic)) -> Option<Vec<Encoding>> {
    let parsed = state.with(|state| parse_encoding_list(EncodingList::CommaSeparated(value), &state.auto_language.detect_order(), &context(caller), 0, ""));
    match parsed {
        Ok(values) => Some(values),
        Err(error) => {
            let message = match error { MbError::Value(message) | MbError::Runtime(message) => message.into_bytes(), MbError::ValueBytes(message) => message };
            emit(Diagnostic { level: 2, message });
            None
        },
    }
}

/// Commits HTTP candidates only after a complete valid list, with exact lowercase pass support.
fn input(state: &mut impl Access, value: &[u8], caller: &str, emit: &mut impl FnMut(Diagnostic)) -> bool {
    if value == b"pass" { state.with(|state| state.http_input_encodings = vec![OutputEncoding::Pass]); return true; }
    match encoding_list(state, value, caller, emit) {
        Some(values) if !values.is_empty() => { state.with(|state| state.http_input_encodings = values.into_iter().map(OutputEncoding::Convert).collect()); true },
        _ => false,
    }
}

/// Applies PHP's length-aware output lookup, including strncmp pass handling, without diagnostics.
fn output(state: &mut impl Access, value: &[u8]) -> bool {
    let pass = b"pass".starts_with(value) || c_string(value) == b"pass";
    let encoding = if pass { Some(OutputEncoding::Pass) } else { Encoding::lookup(value).map(OutputEncoding::Convert) };
    if let Some(encoding) = encoding { state.with(|state| state.output = encoding); true } else { false }
}

/// Emits an invalid-name warning before assigning UTF-8, so callbacks observe the previous encoding.
fn internal(state: &mut impl Access, value: &[u8], caller: &str, emit: &mut impl FnMut(Diagnostic)) {
    let encoding = Encoding::lookup_c_string(value).unwrap_or_else(|| {
        let mut message = format!("{}: Unknown encoding \"", context(caller)).into_bytes();
        message.extend_from_slice(c_string(value));
        message.extend_from_slice(b"\" in ini setting");
        emit(Diagnostic { level: 2, message });
        Encoding::lookup(b"UTF-8").expect("baseline encoding")
    });
    state.with(|state| {
        state.internal = encoding;
        state.ini.regex_encoding = value.to_vec();
    });
    state.configure_regex(value);
}

/// Retains the remembered replacement for invalid numeric text while selecting character mode.
fn substitute(state: &mut State, value: Option<&[u8]>) {
    let Some(value) = value else { state.substitute = Substitute::default(); return; };
    state.substitute.mode = if value.eq_ignore_ascii_case(b"none") { SubstituteMode::None }
        else if value.eq_ignore_ascii_case(b"long") { SubstituteMode::Long }
        else if value.eq_ignore_ascii_case(b"entity") { SubstituteMode::Entity }
        else {
            if !value.is_empty() {
                if let Some(character) = numeric::substitute(value) { state.substitute.character = character; }
            }
            SubstituteMode::Character
        };
}

/// Borrows the C-string prefix used by PHP's INI diagnostics and core encoding helpers.
fn c_string(value: &[u8]) -> &[u8] { value.split(|&byte| byte == 0).next().unwrap_or_default() }

/// Distinguishes PHP startup diagnostics from runtime function-call contexts.
fn context(caller: &str) -> String { if caller == "PHP Startup" { caller.to_owned() } else { format!("{caller}()") } }
