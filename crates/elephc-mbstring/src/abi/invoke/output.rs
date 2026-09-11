//! Purpose:
//! Connects public output arguments and prepared bytes to response metadata and protected headers.
//!
//! Called from:
//! - Native/eval adapters through elephc_mbstring_output_invoke_v1, or prepared-byte ABI hosts.
//!
//! Key details:
//! - Input and MIME metadata are copied before any protected header callback can execute PHP.
//! - Output destination is captured before headers; internal encoding and substitution remain live.
//! - Pending header exceptions preserve conversion side effects and END reset before returning.

use super::*;
use elephc_builtin_contract::mbstring_abi::output_handler::*;
use crate::state::{OutputEncoding, OutputHeaders};

/// Prepares the complete PHP call using the common value host, then executes its response operation.
/// Arity is checked before reading either host; value owners survive until protected cleanup ends.
///
/// # Safety
/// For valid arity, `args` contains `count` borrowed boxed values and `values_host` is a complete
/// immutable V1 through V5 value table obeying elephc_mbstring_invoke_v1. `response_host` obeys
/// elephc_mbstring_output_v1 and may be null when the prepared phase does not require metadata.
/// `out` is writable aligned empty/released result storage. Callbacks never unwind through Rust.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_output_invoke_v1(args: *const *const c_void, count: u64, strict: u32,
    values_host: *const MbInvokeHostV1, response_host: *const MbOutputHostV1, out: *mut MbResultV1) -> i32 {
    unsafe { boundary(out, |session| {
        if strict > 1 { return Err(Status::Fatal); }
        let count = usize::try_from(count).map_err(|_| Status::Fatal)?;
        let contract = elephc_builtin_contract::lookup_id(RuntimeBuiltinId::MbOutputHandler.builtin_id()).ok_or(Status::Fatal)?;
        if let Some(bytes) = crate::coercion::arity_error_contract(contract, count) {
            return Ok(Outcome { bytes, ..Outcome::empty(PREPARED_ARGUMENT_COUNT_ERROR) }.into());
        }
        let values = match prepare_values(args, count, strict != 0, values_host, contract, session)? {
            Ok(values) => values,
            Err(bytes) => return Ok(Outcome { bytes, ..Outcome::empty(RESULT_TYPE_ERROR) }.into()),
        };
        let [input, Argument::Int(phase)] = values.as_slice() else { return Err(Status::Fatal); };
        let input = match input {
            Argument::String(bytes) => bytes.as_slice(), Argument::IniString(bytes) => bytes.as_ref(),
            _ => return Err(Status::Fatal),
        };
        invoke(input, *phase, response_host).map(Into::into)
    }) }
}

/// Runs the output operation on an already coerced PHP string and phase integer.
/// Argument coercion and PHP arity checks belong to the enclosing invocation adapter.
/// The returned status and owned result use the ordinary mbstring invocation protocol.
///
/// # Safety
/// `input` describes `length` readable bytes copied before callbacks; null is allowed for zero.
/// `out` is aligned writable empty/released result storage. For a START phase with conversion
/// selected, `host` points to a complete immutable V1 table and a context alive until return.
/// Other phases and pass mode do not access `host`, which may be null. Callbacks obey the
/// borrowed-byte, metadata, and non-unwinding contracts in MbOutputHostV1.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_output_v1(input: *const u8, length: u64, phase: i64,
    host: *const MbOutputHostV1, out: *mut MbResultV1) -> i32 {
    unsafe { boundary(out, |_| {
        if length > isize::MAX as u64 || (length != 0 && input.is_null()) { return Err(Status::Fatal); }
        let input = if length == 0 { Vec::new() }
            else { std::slice::from_raw_parts(input, length as usize).to_vec() };
        invoke(&input, phase, host).map(Into::into)
    }) }
}

/// Owns response strings independently of host callbacks and preserves optional empty settings.
#[derive(Default)]
struct Metadata {
    mimetype: Option<Vec<u8>>, default_mimetype: Option<Vec<u8>>,
    send_default_content_type: bool, in_handler: bool,
}

impl Metadata {
    /// Borrows the captured response fields only while the shared engine creates its owned plan.
    fn headers(&self) -> OutputHeaders<'_> {
        OutputHeaders { mimetype: self.mimetype.as_deref(), default_mimetype: self.default_mimetype.as_deref(),
            send_default_content_type: self.send_default_content_type, in_handler: self.in_handler }
    }
}

/// Performs START-only host work, then finishes conversion after a protected header publication.
unsafe fn invoke(input: &[u8], phase: i64, host: *const MbOutputHostV1) -> Result<Outcome, Status> {
    let needs_info = phase & 1 != 0 && REQUEST.with(|state| matches!(state.borrow().http_output(), OutputEncoding::Convert(_)));
    let (host, metadata) = if needs_info {
        let host = unsafe { host.as_ref() }.ok_or(Status::Fatal)?;
        if host.version != 1 || host.size as usize != std::mem::size_of::<MbOutputHostV1>()
            || host.info.is_none() || host.header.is_none() { return Err(Status::Fatal); }
        (Some(*host), unsafe { read_metadata(host)? })
    } else { (None, Metadata::default()) };
    let plan = REQUEST.with(|state| {
        let state = state.borrow();
        let matched = match (metadata.mimetype.as_deref(), crate::state::mime_pattern(state.http_output_conv_mimetypes())) {
            (Some(subject), Some(pattern)) => super::super::ini::matches_output_mime(pattern, subject).map_err(|_| Status::Fatal)?,
            _ => false,
        };
        Ok(state.prepare_output(phase, metadata.headers(), matched))
    })?;
    let mut pending = false;
    if let Some(header) = &plan.header {
        let host = host.ok_or(Status::Fatal)?;
        match unsafe { host.header.unwrap()(host.context, header.as_ptr(), header.len() as u64) } {
            0 => {}, 2 => pending = true, _ => return Err(Status::Fatal),
        }
    }
    let bytes = REQUEST.with(|state| state.borrow_mut().output_chunk(input, plan));
    if pending { return Err(Status::Pending); }
    Ok(Outcome { bytes, ..Outcome::empty(RESULT_STRING) })
}

/// Validates native-only metadata and copies every borrowed string before further host actions.
unsafe fn read_metadata(host: &MbOutputHostV1) -> Result<Metadata, Status> {
    let mut info = MbOutputInfoV1::default();
    if unsafe { host.info.unwrap()(host.context, &mut info) } != 0
        || info.send_default_content_type > 1 || info.in_handler > 1 { return Err(Status::Fatal); }
    Ok(Metadata {
        mimetype: unsafe { copy_optional(info.mimetype, info.mimetype_len)? },
        default_mimetype: unsafe { copy_optional(info.default_mimetype, info.default_mimetype_len)? },
        send_default_content_type: info.send_default_content_type != 0, in_handler: info.in_handler != 0,
    })
}

/// Preserves absent versus present-empty metadata and truncates present bytes at PHP's first NUL.
unsafe fn copy_optional(bytes: *const u8, length: u64) -> Result<Option<Vec<u8>>, Status> {
    if length > isize::MAX as u64 || (bytes.is_null() && length != 0) { return Err(Status::Fatal); }
    if bytes.is_null() { return Ok(None); }
    let bytes = unsafe { std::slice::from_raw_parts(bytes, length as usize) };
    let length = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    Ok(Some(bytes[..length].to_vec()))
}
