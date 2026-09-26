//! Purpose:
//! Implements independent C host callbacks for shared mbstring invocation regression tests.
//!
//! Called from:
//! - The invocation fixture's MbInvokeHostV1 callback table.
//!
//! Key details:
//! - Owned values use Box<Php>, tracked separately to detect missing and repeated releases.
//! - Metadata and converted strings have owners even on injected failures.
//! - Array readers resolve shared references without mutating or copying the original graph.

use super::*;

/// Copies a borrowed argument into a separately owned value, normalizing PHP reference cells.
pub(super) unsafe extern "C" fn clone_value(context: *mut c_void, input: *const c_void, out: *mut *mut c_void) -> i32 {
    let host = unsafe { &mut *context.cast::<Host>() };
    let fault = host.enter("clone");
    let value = if input.is_null() { Php::Null } else { by_value(unsafe { &*input.cast::<Php>() }) };
    if !fault.is_some_and(|fault| fault.malformed) { unsafe { *out = host.own(value); } }
    fault.map_or(0, |fault| fault.status)
}

/// Describes concrete kinds and publishes independently owned binary class metadata for objects.
pub(super) unsafe extern "C" fn describe(
    context: *mut c_void, input: *const c_void, out: *mut MbCoercionInputV1, owner: *mut *mut c_void,
) -> i32 {
    let host = unsafe { &mut *context.cast::<Host>() };
    let fault = host.enter("describe");
    let value = unsafe { &*input.cast::<Php>() };
    let mut descriptor = MbCoercionInputV1 { kind: HOST_NULL, value: 0, bytes: std::ptr::null(), len: 0, flags: 0 };
    match value {
        Php::Null => {},
        Php::Bool(value) => { descriptor.kind = HOST_BOOL; descriptor.value = *value as u64; },
        Php::Int(value) => { descriptor.kind = HOST_INT; descriptor.value = *value as u64; },
        Php::Float(value) => { descriptor.kind = HOST_FLOAT; descriptor.value = value.to_bits(); },
        Php::String(bytes) => { descriptor.kind = HOST_STRING; descriptor.bytes = bytes.as_ptr(); descriptor.len = bytes.len() as u64; },
        Php::Array(values) => {
            descriptor.kind = HOST_ASSOC_ARRAY;
            descriptor.value = Rc::as_ptr(values) as u64;
            descriptor.flags = u64::from(host.catalog.as_ref().is_some_and(|catalog| Rc::ptr_eq(catalog, values)));
        },
        Php::Object { .. } => {
            let metadata = host.own(string(b"MbOrderText"));
            let Php::String(bytes) = (unsafe { &*metadata.cast::<Php>() }) else { return 1; };
            descriptor.kind = INPUT_OBJECT; descriptor.value = input as u64;
            descriptor.bytes = bytes.as_ptr(); descriptor.len = bytes.len() as u64; descriptor.flags = INPUT_STRINGABLE;
            unsafe { *owner = metadata; }
        },
        Php::Reference(_) => return 1,
    }
    if fault.is_some_and(|fault| fault.malformed) { descriptor.kind = u64::MAX; }
    unsafe { *out = descriptor; }
    fault.map_or(0, |fault| fault.status)
}

/// Publishes binary string bytes and a distinct tracked native owner for the coordinator to consume.
unsafe fn output_string(host: &mut Host, bytes: Vec<u8>, out: *mut MbHostStringV1, fault: Option<Fault>) -> i32 {
    let owner = host.own(Php::String(bytes));
    let Php::String(bytes) = (unsafe { &*owner.cast::<Php>() }) else { return 1; };
    unsafe { *out = MbHostStringV1 { bytes: bytes.as_ptr(), len: bytes.len() as u64, owner }; }
    if fault.is_some_and(|fault| fault.malformed) {
        unsafe { (*out).bytes = std::ptr::null(); (*out).len = 1; }
    }
    fault.map_or(0, |fault| fault.status)
}

/// Executes a fixture Stringable side effect at the coordinator-selected point in argument order.
pub(super) unsafe extern "C" fn stringable(context: *mut c_void, input: *const c_void, out: *mut MbHostStringV1) -> i32 {
    let host = unsafe { &mut *context.cast::<Host>() };
    let fault = host.enter("stringable");
    let Php::Object { label, bytes, action } = (unsafe { &*input.cast::<Php>() }) else { return 1; };
    host.trace.push(json!(["stringify", label]));
    match action {
        Action::None => {},
        Action::Assign(target, value) => { *target.borrow_mut() = (**value).clone(); },
        Action::Cow(target) => {
            let mut target = target.borrow_mut();
            let Php::Array(values) = &mut *target else { return 1; };
            Rc::make_mut(values)[0] = string(b"\xff");
        },
        Action::Language(language) => { call(RuntimeBuiltinId::MbLanguage, &[MbArgV1::string(language)]); },
        Action::Internal => { call(RuntimeBuiltinId::MbInternalEncoding, &[MbArgV1::string(b"8bit")]); },
        Action::Throw => { host.pending = Some(json!(["error", "RuntimeException", hex(b"callback stopped")])); return 2; },
    }
    unsafe { output_string(host, bytes.clone(), out, fault) }
}

/// Models only the oracle's float-format actions, recording the precision visible at conversion.
pub(super) unsafe extern "C" fn format_float(context: *mut c_void, bits: u64, out: *mut MbHostStringV1) -> i32 {
    let host = unsafe { &mut *context.cast::<Host>() };
    let fault = host.enter("float");
    let value = f64::from_bits(bits);
    let bytes = if value.is_nan() { b"NAN".to_vec() }
        else if bits == 1.23456789_f64.to_bits() { if host.precision == 14 { b"1.23456789".to_vec() } else { b"1.23".to_vec() } }
        else { host.errors.push(format!("unexpected fixture float {bits:x}")); return 1; };
    unsafe { output_string(host, bytes, out, fault) }
}

/// Models a PHP error handler that observes, throws, or reenters the shared state setters.
pub(super) unsafe extern "C" fn diagnostic(context: *mut c_void, level: u32, bytes: *const u8, len: u64) -> i32 {
    let host = unsafe { &mut *context.cast::<Host>() };
    let fault = host.enter("diagnostic");
    let bytes = if len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(bytes, len as usize) } };
    host.trace.push(json!(["diagnostic", level, hex(bytes)]));
    if host.handler == "throw" {
        host.pending = Some(json!(["error", "RuntimeException", hex(b"diagnostic stopped")]));
        return 2;
    }
    if host.handler == "entity_map" {
        if let Some((_, target)) = &host.observer { *target.borrow_mut() = Php::Int(255); }
    }
    if host.handler == "regex_reentry" {
        call(RuntimeBuiltinId::MbRegexSetOptions, &[MbArgV1::string(b"r")]);
        let mut nested = Host::new("");
        let (status, result) = run(RuntimeBuiltinId::MbEregMatch, &[string(b"abc"), string(b"ABC")], false, &mut nested);
        host.trace.push(json!(["nested_match", status, result]));
        if !nested.live.is_empty() || !nested.errors.is_empty() { host.errors.push("nested regex ownership failed".into()); }
        call(RuntimeBuiltinId::MbRegexEncoding, &[MbArgV1::string(b"ASCII")]);
    }
    if host.handler == "mutate" {
        call(RuntimeBuiltinId::MbSubstituteCharacter, &[MbArgV1::integer(33)]);
        call(RuntimeBuiltinId::MbInternalEncoding, &[MbArgV1::string(b"8bit")]);
        host.precision = 3;
    }
    fault.map_or(0, |fault| fault.status)
}

/// Consumes each published owner exactly once and can report a destructor failure after consumption.
pub(super) unsafe extern "C" fn release(context: *mut c_void, owner: *mut c_void) -> i32 {
    let host = unsafe { &mut *context.cast::<Host>() };
    let fault = host.enter("release");
    if !host.live.remove(&(owner as usize)) {
        host.errors.push("unowned or repeated native release".into());
        return 1;
    }
    unsafe { drop(Box::from_raw(owner.cast::<Php>())); }
    fault.map_or(0, |fault| fault.status)
}

/// Borrows the fixture's concrete value without copying its binary string storage.
fn descriptor(value: &Php) -> MbHostValueV1 {
    let (tag, lo, hi) = match value {
        Php::Null => (HOST_NULL, 0, 0), Php::Bool(value) => (HOST_BOOL, *value as u64, 0),
        Php::Int(value) => (HOST_INT, *value as u64, 0), Php::Float(value) => (HOST_FLOAT, value.to_bits(), 0),
        Php::String(bytes) => (HOST_STRING, bytes.as_ptr() as u64, bytes.len() as u64),
        Php::Array(values) => (HOST_ASSOC_ARRAY, Rc::as_ptr(values) as u64, 0),
        Php::Reference(value) => return descriptor(&value.borrow()),
        Php::Object { .. } => (HOST_UNSUPPORTED, 0, 0),
    };
    MbHostValueV1 { tag, lo, hi }
}

/// Reads retained array payloads after all outer conversions, following nested PHP references.
pub(super) unsafe extern "C" fn next(
    context: *mut c_void, array: *const MbHostValueV1, cursor: *mut u64,
    key: *mut MbHostValueV1, value: *mut MbHostValueV1,
) -> u64 {
    let host = unsafe { &mut *context.cast::<Host>() };
    if host.enter("read").is_some() { return ITER_ERROR; }
    let values = unsafe { &*((*array).lo as *const Vec<Php>) };
    let index = unsafe { *cursor as usize };
    let Some(entry) = values.get(index) else { return ITER_END; };
    unsafe {
        *key = MbHostValueV1 { tag: HOST_STRING, lo: b"name".as_ptr() as u64, hi: 4 };
        *value = descriptor(entry);
        *cursor += 1;
    }
    ITER_ENTRY
}

/// Publishes an independently copied ordered entry before returning an injected success or failure.
pub(super) unsafe extern "C" fn array_value(
    context: *mut c_void, array: *const MbArraySourceV2, cursor: *mut u64, out: *mut MbArrayEntryV2,
) -> i32 {
    let host = unsafe { &mut *context.cast::<Host>() };
    let fault = host.enter("array_value");
    let Php::Array(values) = (unsafe { &*(*array).retained.cast::<Php>() }) else { return 1; };
    let index = unsafe { *cursor as usize };
    if let Some(value) = values.get(index) {
        unsafe { (*out).kind = ITER_ENTRY; (*out).owner = host.own(by_value(value)); *cursor += 1; }
    }
    if fault.is_some_and(|fault| fault.malformed) { unsafe { (*out).kind = ITER_ERROR; } }
    fault.map_or(0, |fault| fault.status)
}
