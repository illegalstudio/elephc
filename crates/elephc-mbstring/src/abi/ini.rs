//! Purpose:
//! Exposes shared INI configuration and reentrant mutations through a panic-contained C ABI.
//!
//! Called from:
//! - Native/eval INI adapters after PHP argument coercion and program initialization.
//!
//! Key details:
//! - Arguments borrow coerced bytes or leased identities; result cleanup owns buffers and identity leases.
//! - PHP diagnostics run outside request borrows, and pending exceptions survive handler completion.
//! - Configuration is committed only after native validation succeeds; request reset keeps its defaults.

mod provider;
pub(super) use provider::matches as matches_output_mime;
mod strings;
mod native;
mod core;
pub use native::*;
pub use core::{elephc_mbstring_core_ini_v1, elephc_mbstring_query_configuration_v1};
pub(crate) use strings::release_result;
pub(super) use strings::lookup as lookup_string;
use strings::Reply;

use std::{cell::Cell, sync::OnceLock};
use elephc_builtin_contract::{RuntimeBuiltinStatus, mbstring_abi::ini::*};
use crate::{coercion::Diagnostic, state::{CoreEncodingDefaults, IniRequest, IniString, MimeRegexError}};
use super::*;

/// Immutable validated startup state shared by newly initialized worker threads and request resets.
struct Configuration { inputs: Vec<Vec<u8>>, state: State }

static CONFIGURATION: OnceLock<Configuration> = OnceLock::new();

/// Copies only the startup prototype, so live request mutations never enter another thread's state.
pub(super) fn initial_state() -> State { CONFIGURATION.get().map_or_else(State::default, |configuration| configuration.state.clone()) }

/// Installs a complete process-lifetime PCRE2 provider without changing request settings.
///
/// # Safety
/// `provider` points to a readable aligned MbMimeRegexV1 whose non-unwinding native
/// callbacks remain valid for the process lifetime, even after this table is copied.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_mime_provider_v1(provider: *const MbMimeRegexV1) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| unsafe { provider.as_ref() }.is_some_and(|provider| provider::install(*provider))));
    if matches!(result, Ok(true)) { 0 } else { 1 }
}

/// Executes an INI operation on the same request state used by all AOT/eval text calls.
///
/// # Safety
/// `args` describes `count` live MbArgV1 values with readable payloads, and `out` is writable
/// aligned empty/released result storage. `host` is a complete V1 table with a protected,
/// non-unwinding diagnostic callback and a context alive through final callback delivery.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_ini_v1(op: u32, args: *const MbArgV1, count: u64,
    host: *const MbIniHostV1, out: *mut MbResultV1) -> i32 {
    unsafe { boundary(out, |status| {
        let host = read_host(host)?;
        let args = read_args(args, count)?;
        let mut validate = |pattern: &[u8]| validation(pattern, status);
        let mut emit = |warning| diagnostic(host, warning, status);
        let outcome = match (op, args) {
            (INI_GET, [name]) => {
                let name = string_arg(name)?;
                REQUEST.with(|state| state.borrow().ini_get_string(name).map_or_else(|| Outcome::boolean(false).into(), Reply::string))
            },
            (INI_SET, [name, value]) => {
                let name = string_arg(name)?.to_vec();
                let value = if value.kind == ARG_INI_STRING {
                    if value.value <= 0 || value.len != 0 || !value.bytes.is_null() { return Err(1); }
                    strings::lookup(value.value as u64)?
                } else { IniString::new(string_arg(value)?) };
                let result = REQUEST.with(|state| REGEX.with(|regex|
                    IniRequest { state, regex }.set(&name, value, &mut validate, &mut emit)));
                result.previous.map_or_else(|| Outcome::boolean(false).into(), Reply::string)
            },
            (INI_RESTORE, [name]) => {
                let name = string_arg(name)?;
                REQUEST.with(|state| REGEX.with(|regex|
                    IniRequest { state, regex }.restore(name, &mut validate, &mut emit)));
                Outcome::empty(RESULT_NULL).into()
            },
            (INI_GET_ALL, [details]) if details.kind == ARG_BOOL && matches!(details.value, 0 | 1) => {
                let (graph, strings) = REQUEST.with(|state| state.borrow().ini_get_all_strings(details.value != 0));
                Reply::array(graph, strings)
            },
            (INI_STRING_NEW | INI_STRING_INTERNED, [value]) => {
                let value = string_arg(value)?;
                Reply::string(if op == INI_STRING_NEW { IniString::fresh(value) } else { IniString::interned(value) })
            },
            _ => return Err(1),
        };
        if status.get() == 0 { Ok(outcome) } else { Err(status.get()) }
    }) }
}

/// Retains an existing INI string identity for a native string copy, returning fatal for an expired ID.
/// The caller pairs each successful retain with elephc_mbstring_ini_string_release_v1.
#[no_mangle]
pub extern "C" fn elephc_mbstring_ini_string_retain_v1(identity: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| strings::retain(identity))).unwrap_or(1)
}

/// Releases one identity lease owned by native string metadata, including after a request reset.
/// This does not release any MbResultV1; those retain their separate result-owned lease.
#[no_mangle]
pub extern "C" fn elephc_mbstring_ini_string_release_v1(identity: u64) -> i32 {
    catch_unwind(AssertUnwindSafe(|| strings::release(identity))).unwrap_or(1)
}

/// Installs startup defaults from three core encodings followed by mbstring/Core query key/value pairs.
/// Threads and request resets inherit the same prototype. Repeated identical installation is a no-op;
/// a different configuration fails closed. Duplicate keys use their final supplied value.
///
/// # Safety
/// Arguments, host, and output obey elephc_mbstring_ini_v1's ownership and callback rules.
/// Every slot is a coerced string. Startup diagnostic callbacks must not run PHP user code.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_configure_v1(args: *const MbArgV1, count: u64,
    host: *const MbIniHostV1, out: *mut MbResultV1) -> i32 {
    unsafe { boundary(out, |status| {
        let host = read_host(host)?;
        let args = read_args(args, count)?;
        if args.len() < 3 || (args.len() - 3) % 2 != 0 { return Err(1); }
        let inputs: Vec<_> = args.iter().map(|argument| string_arg(argument).map(<[u8]>::to_vec)).collect::<Result<_, _>>()?;
        if let Some(configuration) = CONFIGURATION.get() {
            return if configuration.inputs == inputs { Ok(Outcome::empty(RESULT_NULL).into()) } else { Err(1) };
        }
        let defaults = CoreEncodingDefaults { internal: string_arg(&args[0])?.to_vec(),
            input: string_arg(&args[1])?.to_vec(), output: string_arg(&args[2])?.to_vec() };
        let mut settings = Vec::new();
        for pair in args[3..].chunks_exact(2) { settings.push((string_arg(&pair[0])?.to_vec(), string_arg(&pair[1])?.to_vec())); }
        let (state, warnings) = State::with_ini_configuration(&settings, defaults, |pattern| validation(pattern, status));
        if status.get() != 0 { return Err(status.get()); }
        match CONFIGURATION.set(Configuration { inputs, state }) {
            Ok(()) => {},
            Err(candidate) => return if CONFIGURATION.get().unwrap().inputs == candidate.inputs {
                Ok(Outcome::empty(RESULT_NULL).into())
            } else { Err(1) },
        }
        REQUEST.with(|request| *request.borrow_mut() = initial_state());
        REGEX.with(|regex| regex.configure_encoding(initial_state().ini_regex_encoding()));
        for warning in warnings { diagnostic(host, warning, status); }
        if status.get() == 0 { Ok(Outcome::empty(RESULT_NULL).into()) } else { Err(status.get()) }
    }) }
}

/// Updates inherited request encodings after a core INI change without modifying startup defaults.
///
/// # Safety
/// Three string slots contain the effective internal/input/output encodings. Host, argument,
/// and result ownership obey elephc_mbstring_ini_v1, including protected warning reentry.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_core_encoding_v1(args: *const MbArgV1, count: u64,
    host: *const MbIniHostV1, out: *mut MbResultV1) -> i32 {
    unsafe { boundary(out, |status| {
        let host = read_host(host)?;
        let [internal, input, output] = read_args(args, count)? else { return Err(1); };
        let defaults = CoreEncodingDefaults { internal: string_arg(internal)?.to_vec(),
            input: string_arg(input)?.to_vec(), output: string_arg(output)?.to_vec() };
        REQUEST.with(|state| REGEX.with(|regex| IniRequest { state, regex }
            .update_core_encoding_defaults(defaults, |warning| diagnostic(host, warning, status))));
        if status.get() == 0 { Ok(Outcome::empty(RESULT_NULL).into()) } else { Err(status.get()) }
    }) }
}

/// Contains Rust panics and transfers a result only on success, retaining pending PHP exceptions.
unsafe fn boundary(out: *mut MbResultV1, action: impl FnOnce(&Cell<i32>) -> Result<Reply, i32>) -> i32 {
    if out.is_null() { return RuntimeBuiltinStatus::RuntimeFatal as i32; }
    unsafe { out.write(MbResultV1::default()); }
    let status = Cell::new(0);
    let result = catch_unwind(AssertUnwindSafe(|| action(&status).and_then(Reply::into_wire))).unwrap_or(Err(1));
    if status.get() == 2 { return RuntimeBuiltinStatus::PendingThrowable as i32; }
    match result {
        Ok(result) => { unsafe { out.write(result); } 0 },
        Err(2) => 2,
        Err(_) => 1,
    }
}

/// Copies a complete host table before callbacks can change any caller-owned memory.
unsafe fn read_host(host: *const MbIniHostV1) -> Result<MbIniHostV1, i32> {
    let host = unsafe { host.as_ref() }.ok_or(1)?;
    if host.version != 1 || host.size as usize != std::mem::size_of::<MbIniHostV1>() || host.diagnostic.is_none() { return Err(1); }
    Ok(*host)
}

/// Borrows only a representable argument range, accepting a null pointer solely for zero count.
unsafe fn read_args<'a>(args: *const MbArgV1, count: u64) -> Result<&'a [MbArgV1], i32> {
    if count > isize::MAX as u64 / std::mem::size_of::<MbArgV1>() as u64 || (count != 0 && args.is_null()) { return Err(1); }
    Ok(if count == 0 { &[] } else { unsafe { std::slice::from_raw_parts(args, count as usize) } })
}

/// Rejects uncoerced slots and malformed string payloads before touching request state.
unsafe fn string_arg(slot: &MbArgV1) -> Result<&[u8], i32> {
    if slot.kind != ARG_STRING { return Err(1); }
    unsafe { super::bytes(slot) }.ok_or(1)
}

/// Converts syntax and unavailable-provider errors into PHP warnings, keeping malformed providers fatal.
fn validation(pattern: &[u8], status: &Cell<i32>) -> Result<(), MimeRegexError> {
    match provider::validate(pattern) {
        Ok(()) => Ok(()),
        Err(provider::Failure::Compile(error)) => Err(error),
        Err(provider::Failure::Unavailable) => Err(MimeRegexError { offset: 0,
            message: b"managed PCRE2 MIME provider is unavailable; run elephc native add pcre2 and use --with-mbstring for opaque custom MIME configuration".to_vec() }),
        Err(provider::Failure::Fatal) => {
            if status.get() != 2 { status.set(1); }
            Err(MimeRegexError { offset: 0, message: Vec::new() })
        },
    }
}

/// Delivers a warning once, outside state borrows, and suppresses further callbacks after failure.
fn diagnostic(host: MbIniHostV1, warning: Diagnostic, status: &Cell<i32>) {
    if status.get() != 0 { return; }
    let result = unsafe { host.diagnostic.unwrap()(host.context, warning.level, warning.message.as_ptr(), warning.message.len() as u64) };
    match result { 0 => {}, 2 => status.set(2), _ => status.set(1) }
}
