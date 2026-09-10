//! Purpose:
//! Connects PHP mbregex settings and searches to one thread-local session shared by AOT and eval.
//!
//! Called from:
//! - The versioned mbstring operation dispatcher and request initialization.
//!
//! Key details:
//! - These settings need no native provider; matching installs Oniguruma separately.
//! - Null means getter, encoding setters return true, and option setters return the old options.

use super::*;
use crate::regex::{Session, Event, Limits, RegexError, RegisterKey, Registers, Replacement, ReplaceResult, RegexEncoding};
use elephc_builtin_contract::mbstring_abi::array::{Array, ArrayGraph, Key, Value};
use elephc_builtin_contract::mbstring_abi::regex::MbRegexProviderV1;

/// Installs one complete, process-lifetime Oniguruma provider after checking its readable ABI header.
///
/// # Safety
/// The pointer is null or addresses an aligned readable version/size header. A matching header
/// guarantees the complete table is readable and its non-unwinding callbacks remain mapped.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_regex_provider_v1(provider: *const MbRegexProviderV1) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if provider.is_null() { return false; }
        let header = provider.cast::<u32>();
        let (version, size) = unsafe { (header.read(), header.add(1).read()) };
        if version != 1 || size as usize != std::mem::size_of::<MbRegexProviderV1>() { return false; }
        unsafe { crate::regex::install_provider(provider.read()) }
    }));
    if matches!(result, Ok(true)) { 0 } else { 1 }
}

/// Reports shared provider availability without creating a second eval-owned regex engine.
#[no_mangle]
pub extern "C" fn elephc_mbstring_regex_available_v1() -> i32 { i32::from(crate::regex::provider_available()) }

/// Executes coerced regex arguments and buffers warnings for the callback-free wire entry.
pub(super) fn dispatch(operation: RuntimeBuiltinId, args: &Arguments<'_>, state: &State) -> Outcome {
    let (stack, retry) = state.ini_regex_limits();
    let mut failures = Vec::new();
    let mut warnings = Vec::new();
    let result = run(operation, args, REGEX.with(|session| session.encoding()),
        Limits::from_ini(stack, retry, operation == RuntimeBuiltinId::MbEregMatch),
        &mut |event| match event {
            Event::Warning(bytes) if failures.is_empty() => {
                warnings.extend_from_slice(b"Warning: ");
                warnings.extend(bytes);
                warnings.push(b'\n');
            },
            Event::Warning(_) => {},
            Event::Exception(error) => failures.push(error),
        });
    let mut outcome = match (result, Outcome::error_chain(failures)) {
        (_, Some(error)) => error,
        (Ok(value), None) => value,
        (Err(_), None) => Outcome::fatal(),
    };
    outcome.diagnostics = warnings;
    outcome
}

/// Runs every shared regex operation without borrowing text state across a diagnostic callback.
pub(super) fn run(operation: RuntimeBuiltinId, args: &Arguments<'_>, entry_encoding: RegexEncoding, limits: Limits,
    emit: &mut impl FnMut(Event)) -> Result<Outcome, RegexError> {
    REGEX.with(|session| match operation {
        RuntimeBuiltinId::MbEregReplace | RuntimeBuiltinId::MbEregiReplace => session.replace(Replacement {
            pattern: args.string(0), replacement: args.string(1), subject: args.string(2), options: args.nullable_string(3),
            encoding: entry_encoding, ignore_case: operation == RuntimeBuiltinId::MbEregiReplace,
        }, limits, emit).map(|result| match result {
            ReplaceResult::InvalidSubject => Outcome::empty(RESULT_NULL),
            ReplaceResult::Failed => Outcome::boolean(false), ReplaceResult::String(bytes) => Outcome::string(Ok(bytes)),
        }),
        RuntimeBuiltinId::MbEregMatch => session.is_match(args.string(0), args.string(1),
            args.nullable_string(2), limits, emit).map(Outcome::boolean),
        RuntimeBuiltinId::MbSplit => session.split(args.string(0), args.string(1), args.integer(2), limits, emit)
            .map(|fields| fields.map_or_else(|| Outcome::boolean(false), |fields| array(fields.into_iter()
                .enumerate().map(|(index, field)| (Key::Int(index as i64), Value::String(field))).collect()))),
        RuntimeBuiltinId::MbEregSearchInit => session.initialize(args.string(0), args.nullable_string(1),
            args.nullable_string(2), emit).map(Outcome::boolean),
        RuntimeBuiltinId::MbEregSearchGetpos => Ok(Outcome::integer(session.position() as i64)),
        RuntimeBuiltinId::MbEregSearchGetregs => Ok(registers(session.registers())),
        RuntimeBuiltinId::MbEregSearchSetpos => Ok(match session.set_position(args.integer(0)) {
            Ok(()) => Outcome::boolean(true), Err(error) => Outcome::error(error),
        }),
        RuntimeBuiltinId::MbEregSearch | RuntimeBuiltinId::MbEregSearchPos | RuntimeBuiltinId::MbEregSearchRegs => {
            let matched = session.search(args.nullable_string(0), args.nullable_string(1), args.contract.name, limits, emit)?;
            Ok(match (operation, matched) {
                (_, None) => Outcome::boolean(false),
                (RuntimeBuiltinId::MbEregSearch, Some(_)) => Outcome::boolean(true),
                (RuntimeBuiltinId::MbEregSearchRegs, Some(matched)) => registers(Some(matched.registers(true))),
                (_, Some(matched)) => {
                    let (offset, length) = matched.position();
                    array(vec![(Key::Int(0), Value::Int(offset as i64)), (Key::Int(1), Value::Int(length as i64))])
                },
            })
        },
        _ => Ok(Outcome::unsupported()),
    })
}

/// Copies retained capture groups into an independently owned graph with PHP's exact key/value types.
pub(super) fn registers(registers: Option<Registers>) -> Outcome {
    let Some(registers) = registers else { return Outcome::boolean(false); };
    array(registers.into_iter().map(|(key, value)| {
        let key = match key { RegisterKey::Index(index) => Key::Int(index as i64), RegisterKey::Name(name) => Key::String(name) };
        let value = value.map_or(Value::Bool(false), Value::String);
        (key, value)
    }).collect())
}

/// Encodes one ordered native/eval result without exporting engine or subject pointers.
fn array(entries: Array) -> Outcome {
    match ArrayGraph::new(0, vec![entries]) {
        Some(graph) => Outcome { bytes: graph.encode(), ..Outcome::empty(RESULT_ARRAY) },
        None => Outcome::fatal(),
    }
}

/// Creates a worker session using the exact configured INI alias, before public text setters run.
pub(super) fn initial_session() -> Session {
    let session = Session::default();
    session.configure_encoding(ini::initial_state().ini_regex_encoding());
    session
}

/// Applies already coerced arguments to shared settings without holding a session borrow across PHP code.
pub(super) fn settings(operation: RuntimeBuiltinId, args: &Arguments<'_>) -> Outcome {
    REGEX.with(|session| match operation {
        RuntimeBuiltinId::MbRegexEncoding => match args.nullable_string(0) {
            None => Outcome::string(Ok(session.encoding().name().as_bytes().to_vec())),
            Some(name) => match session.set_encoding(name) {
                Ok(()) => Outcome::boolean(true), Err(error) => Outcome::error(error),
            },
        },
        RuntimeBuiltinId::MbRegexSetOptions => Outcome::string(match args.nullable_string(0) {
            None => Ok(session.options().as_string().into_bytes()),
            Some(options) => session.set_options(options).map(|previous| previous.as_string().into_bytes()),
        }),
        _ => Outcome::unsupported(),
    })
}
